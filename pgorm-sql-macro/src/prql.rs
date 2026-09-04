//! The `prql!` half: compile PRQL text, hold the result to the oracle, and
//! check the placeholders against the arguments.
//!
//! The census deliberately reads the *emitted SQL's* parse tree rather than
//! the PRQL AST: an s-string can carry a raw `$N` straight into the SQL
//! without any `Param` node existing on the PRQL side, so the only complete
//! account of what the server will ask for is the oracle's own view of the
//! finished statement.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Error, Expr, LitStr, Token};

use crate::oracle;

/// The macro's input: one PRQL string literal, then one argument per `$N`.
pub(crate) struct PrqlInput {
    literal: LitStr,
    args: Vec<Expr>,
}

impl Parse for PrqlInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let literal = input.parse()?;
        let mut args = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            args.push(input.parse()?);
        }

        Ok(Self { literal, args })
    }
}

/// Compile, validate, and expand — or refuse with a spanned error.
///
/// The expansion is a plain tuple expression: the emitted SQL as a string
/// literal (spanned on the PRQL literal it came from) and a
/// `::pgorm::Values` holding each argument converted through
/// `Into<::pgorm::Value>`, in placeholder order. No runtime helper, no new
/// types — a failed conversion or a missing `pgorm` dependency is an
/// ordinary type error at the call site.
// [spec:pgorm:def:macros.prql]
pub(crate) fn expand(input: &PrqlInput) -> Result<TokenStream, Error> {
    let sql = compile(&input.literal)?;
    let placeholders = census(&input.literal, &sql)?;
    check_contiguity(&input.literal, &placeholders)?;
    check_arity(input, placeholders.len())?;

    let literal = LitStr::new(&sql, input.literal.span());
    let values = input
        .args
        .iter()
        .map(|arg| quote!(::std::convert::Into::<::pgorm::Value>::into(#arg)));

    Ok(quote! {
        (#literal, ::pgorm::Values(::std::vec![#(#values),*]))
    })
}

/// PRQL text → PostgreSQL SQL, through the same staged path as the runtime
/// pipeline adapter, with prqlc's own diagnostics on refusal.
///
/// This is where PRQL's `take $1` rejection surfaces: prqlc refuses a
/// parameterized limit with "`take` expected int or range, but found $1",
/// and that message lands on the literal like any other compile failure.
/// S-strings splice through unscreened — their boundary is the oracle, over
/// the finished statement.
// [spec:pgorm:req:macros.prql.reject]    PRQL rejections carry prqlc's message
// [spec:pgorm:sem:macros.prql.sstring]    pass-through; the oracle is the gate
fn compile(literal: &LitStr) -> Result<String, Error> {
    let options = prqlc::Options::default()
        .with_target(prqlc::Target::Sql(Some(prqlc::sql::Dialect::Postgres)))
        .with_display(prqlc::DisplayOptions::Plain)
        .no_format()
        .no_signature();

    prqlc::compile(&literal.value(), &options).map_err(|errors| {
        Error::new(
            literal.span(),
            format!("PRQL rejected this query:\n{errors}"),
        )
    })
}

/// Every `$N` the emitted SQL asks for, straight from the oracle's parse tree.
///
/// `pg_query::parse` is both the grammar gate and the census: a statement it
/// refuses is a compile error, and a statement it accepts is walked for
/// `ParamRef` nodes, which is exactly the set the server will demand at
/// prepare time — `$5` inside a string literal does not count, `$N` smuggled
/// in through an s-string does.
// [spec:pgorm:req:macros.prql.reject]    the emitted SQL must pass the oracle
// [spec:pgorm:sem:macros.prql.census]
fn census(literal: &LitStr, sql: &str) -> Result<BTreeSet<i64>, Error> {
    let parsed = pg_query::parse(sql).map_err(|error| oracle_rejection(literal, sql, &error))?;
    let tree = serde_json::to_value(&parsed.protobuf).map_err(|error| {
        Error::new(
            literal.span(),
            format!("pgorm bug: the accepted parse tree did not serialize: {error}"),
        )
    })?;

    let mut found = BTreeSet::new();
    collect_params(&tree, &mut found);

    Ok(found)
}

/// Walk the serialized parse tree for `ParamRef` nodes.
///
/// The tree is walked as JSON because pg_query's visitor covers only the node
/// types its table-extraction cares about; serde sees every field, so a
/// placeholder cannot hide in a clause the visitor skips.
// [spec:pgorm:sem:macros.prql.census]
fn collect_params(node: &serde_json::Value, found: &mut BTreeSet<i64>) {
    match node {
        serde_json::Value::Object(fields) => {
            if let Some(serde_json::Value::Object(param)) = fields.get("ParamRef")
                && let Some(number) = param.get("number").and_then(serde_json::Value::as_i64)
            {
                found.insert(number);
            }
            for value in fields.values() {
                collect_params(value, found);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_params(item, found);
            }
        }
        _ => {}
    }
}

/// The emitted SQL failed the grammar — an s-string's contents, or a pgorm bug.
///
/// The caller never wrote this SQL, so the message always shows it: with a
/// caret under the column the parser named when it named one, bare otherwise.
// [spec:pgorm:req:macros.prql.reject]    an oracle rejection shows the emitted SQL
fn oracle_rejection(literal: &LitStr, sql: &str, error: &pg_query::Error) -> Error {
    let message = oracle::parser_message(error);
    let mut text = format!("PostgreSQL rejected the SQL compiled from this PRQL: {message}");
    match oracle::token_offset(sql, &message).and_then(|offset| oracle::caret_block(sql, offset)) {
        Some(block) => text.push_str(&block),
        None => {
            text.push_str("\n\n  ");
            text.push_str(sql);
        }
    }

    Error::new(literal.span(), text)
}

/// The placeholders must run `$1..=$max` with no gaps.
///
/// prqlc passes any `$N` through untouched, so `$1, $3` compiles into SQL
/// happily — and would then bind `$3` to the second argument. A gap is
/// refused by name before arity is even considered, as is `$0`, which
/// PostgreSQL's grammar accepts but no bind protocol can satisfy.
// [spec:pgorm:req:macros.prql.reject]    gaps and `$0` refused by name
fn check_contiguity(literal: &LitStr, placeholders: &BTreeSet<i64>) -> Result<(), Error> {
    if placeholders.contains(&0) {
        return Err(Error::new(
            literal.span(),
            "PostgreSQL placeholders start at $1: $0 can never be bound",
        ));
    }
    let Some(max) = placeholders.last().copied() else {
        return Ok(());
    };
    if let Some(missing) = (1..max).find(|number| !placeholders.contains(number)) {
        return Err(Error::new(
            literal.span(),
            format!(
                "the query uses ${max} but never ${missing}: placeholders must be contiguous from $1"
            ),
        ));
    }

    Ok(())
}

/// One argument per distinct placeholder, in placeholder order.
///
/// Too few arguments is an error on the literal naming every unbound `$N`;
/// too many is an error on each surplus argument naming the `$N` the query
/// does not have. Reuse is not arity: `$1` twice is one argument.
// [spec:pgorm:req:macros.prql.reject]    arity errors name the placeholder
fn check_arity(input: &PrqlInput, expected: usize) -> Result<(), Error> {
    let given = input.args.len();
    if given < expected {
        let unbound = ((given + 1)..=expected)
            .map(|number| format!("${number}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(Error::new(
            input.literal.span(),
            format!(
                "the query has {expected} placeholder(s) but {given} argument(s) were given: nothing binds {unbound}"
            ),
        ));
    }

    let mut surplus = input
        .args
        .iter()
        .enumerate()
        .skip(expected)
        .map(|(index, argument)| {
            Error::new_spanned(
                argument,
                format!(
                    "no ${} in the query for this argument: it has {expected} placeholder(s)",
                    index + 1
                ),
            )
        });
    if let Some(mut error) = surplus.next() {
        for another in surplus {
            error.combine(another);
        }
        return Err(error);
    }

    Ok(())
}
