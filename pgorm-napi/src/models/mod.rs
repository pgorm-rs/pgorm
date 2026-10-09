//! The native half of the models JavaScript declares.
//!
//! A model is JavaScript data lowered into the statement builders, as
//! pgorm-python's are Python data over its native builders; what it needs
//! from Rust is small and lives here: one spelling of a value's kind, which
//! a declared column and a decoded result column are compared by; the bounded
//! composition of a prefixed result-column name pgorm's own graph writer
//! mints, so a model graph's aliases are the ones pgorm writes; and the
//! statement pgorm's paginator counts a query's rows with.

#[cfg(test)]
mod parity;

use neon::prelude::*;
use pgorm::pgorm_query::{Asterisk, Expr, Func, Name, Query, SelectStatement};

use crate::{
    codec::Codec,
    statements::{self, Node},
    values::{Tag, TypeName, read, scalar_name},
};

pub(crate) fn export(cx: &mut ModuleContext) -> NeonResult<()> {
    cx.export_function("modelKindKey", kind_key_of_kind)?;
    cx.export_function("modelValueKey", kind_key_of_value)?;
    cx.export_function("modelResultName", result_name)?;
    cx.export_function("modelCount", count)?;
    Ok(())
}

/// A kind as one string, the same for a declared column and a decoded one:
/// a scalar kind's name (`i32`, `tstzrange`), an enum's type as SQL quotes
/// it, a created range's type and subtype, and an array's element followed
/// by `[]`.
// [spec:pgorm:req:napi.model-records]
pub(crate) fn kind_key(tag: &Tag) -> String {
    match tag {
        Tag::Scalar(scalar) => scalar.name().to_owned(),
        Tag::Enum(name) => format!("enum {}", quoted(name)),
        Tag::Created(kind) => format!(
            "{} {} of {}",
            if kind.multirange {
                "multirange"
            } else {
                "range"
            },
            quoted(&kind.name),
            scalar_name(&kind.subtype)
        ),
        Tag::Array(element) => format!("{}[]", kind_key(element)),
    }
}

fn quoted(name: &TypeName) -> String {
    let part = |part: &str| format!("\"{}\"", part.replace('"', "\"\""));
    match &name.schema {
        Some(schema) => format!("{}.{}", part(schema), part(&name.name)),
        None => part(&name.name),
    }
}

/// `modelKindKey(kind, array)`: the key of a declared column's kind, an
/// array of it when `array` is true.
fn kind_key_of_kind(mut cx: FunctionContext) -> JsResult<JsString> {
    let kind = cx.argument::<JsValue>(0)?;
    let array = cx.argument::<JsBoolean>(1)?.value(&mut cx);
    let codec = Codec::get(&mut cx)?;
    let tag = read::kind(&mut cx, codec, kind)?;
    let tag = if array {
        if matches!(tag, Tag::Created(_)) {
            return read::refuse(&mut cx, "arrays of a created range type are not supported");
        }
        Tag::Array(Box::new(tag))
    } else {
        tag
    };
    Ok(cx.string(kind_key(&tag)))
}

/// `modelValueKey(value)`: the key of the kind a `Value` declares.
fn kind_key_of_value(mut cx: FunctionContext) -> JsResult<JsString> {
    let value = cx.argument::<JsValue>(0)?;
    let codec = Codec::get(&mut cx)?;
    match read::infer(&mut cx, codec, value)? {
        Some(tagged) => Ok(cx.string(kind_key(&tagged.tag))),
        None => read::refuse(&mut cx, "null declares no kind"),
    }
}

/// PostgreSQL's identifier bound: `NAMEDATALEN - 1` bytes.
const IDENT_MAX_BYTES: usize = 63;

/// The result-set name of `column` under `prefix`, composed as pgorm's graph
/// writer composes it (`result_column_name`): the plain concatenation when it
/// is under 63 bytes, otherwise its longest whole-character head within 47
/// bytes, padded with `_` to 47, and the 64-bit FNV-1a hash of the whole name
/// in 16 hex digits. The parity tests hold this to the aliases pgorm's own
/// `SelectGraph` writes.
// [spec:pgorm:req:napi.graphs]
pub(crate) fn result_column_name(prefix: &str, column: &str) -> String {
    let full = format!("{prefix}{column}");
    if full.len() < IDENT_MAX_BYTES {
        return full;
    }
    let head = IDENT_MAX_BYTES - 16;
    let mut end = head;
    while !full.is_char_boundary(end) {
        end -= 1;
    }
    let hash = full.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{}{}{hash:016x}", &full[..end], "_".repeat(head - end))
}

/// `modelResultName(prefix, column)`.
fn result_name(mut cx: FunctionContext) -> JsResult<JsString> {
    let prefix = cx.argument::<JsString>(0)?.value(&mut cx);
    let column = cx.argument::<JsString>(1)?.value(&mut cx);
    Ok(cx.string(result_column_name(&prefix, &column)))
}

/// The statement counting the rows `query` reads, as pgorm's paginator
/// counts them: its limit, offset and ordering dropped, and the rest read as
/// a subquery, `SELECT COUNT(*) AS "num_items" FROM (..) AS "sub_query"`.
// [spec:pgorm:req:napi.pagination]
pub(crate) fn counted(mut query: SelectStatement) -> SelectStatement {
    query.reset_limit().reset_offset().clear_order_by();
    Query::select()
        .expr_as(Func::count(Expr::col(Asterisk)), Name::runtime("num_items"))
        .from_subquery(query, Name::runtime("sub_query"))
        .to_owned()
}

/// `modelCount(select)`: the counting statement of a `Select`.
fn count(mut cx: FunctionContext) -> JsResult<JsBox<Node>> {
    let value = cx.argument::<JsValue>(0)?;
    match statements::node(&mut cx, value) {
        Some(Node::Select(select)) => Ok(cx.boxed(Node::Select(counted(select)))),
        _ => read::refuse(&mut cx, "only a Select's rows are counted"),
    }
}
