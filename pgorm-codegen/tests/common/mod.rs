//! Shared helpers for the `pgorm-codegen` spec-verification tests.
//!
//! Everything here drives the public generation pipeline —
//! `EntityTransformer::transform` followed by `EntityWriter::generate` — which
//! is the only surface reachable from outside the crate: `Entity`, `Column`,
//! `Relation` and friends all keep `pub(crate)` fields.
#![allow(dead_code)]

use pgorm_codegen::{EntityTransformer, EntityWriterContext, EntityWriterOptions};
use pgorm_query::{
    Alias, ColumnDef, ColumnType, ForeignKey, ForeignKeyAction, Index, Table, TableCreateStatement,
};
use proc_macro2::{Delimiter, TokenStream, TokenTree};

/// The full `EntityWriterContext::new` option set; `Opts::default()` is the
/// shape a caller gets with no flags: compact format, no serde, chrono,
/// `mod.rs`.
pub type Opts = EntityWriterOptions;

/// The same set with the expanded format selected.
pub fn expanded() -> Opts {
    Opts {
        expanded_format: true,
        ..Default::default()
    }
}

pub fn context(opts: Opts) -> EntityWriterContext {
    EntityWriterContext::new(opts).expect("options should build a context")
}

/// The in-memory `WriterOutput`, keyed for convenient lookup while preserving
/// the emission order (which several rules constrain).
pub struct Generated {
    pub files: Vec<(String, String)>,
}

impl Generated {
    /// File contents by name; panics with the available names when missing.
    pub fn file(&self, name: &str) -> &str {
        self.files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.as_str())
            .unwrap_or_else(|| panic!("no generated file {name:?}; got {:?}", self.names()))
    }

    pub fn names(&self) -> Vec<&str> {
        self.files.iter().map(|(n, _)| n.as_str()).collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.files.iter().any(|(n, _)| n == name)
    }
}

/// Run the whole pipeline: schema statements in, generated files out.
pub fn generate(stmts: Vec<TableCreateStatement>, opts: Opts) -> Generated {
    let ctx = context(opts);
    let writer = EntityTransformer::transform(stmts).expect("transform should succeed");
    Generated {
        files: writer
            .generate(&ctx)
            .files
            .into_iter()
            .map(|f| (f.name, f.content))
            .collect(),
    }
}

/// Canonical token text — every token separated by exactly one space — so
/// expectations can be written as readable Rust instead of reproducing
/// `TokenStream::to_string`'s joint/alone spacing by hand.
pub fn norm(src: &str) -> String {
    let stream: TokenStream = src
        .parse()
        .expect("generated output should lex as Rust tokens");
    let mut tokens = Vec::new();
    flatten(stream, &mut tokens);
    tokens.join(" ")
}

fn flatten(stream: TokenStream, out: &mut Vec<String>) {
    for tree in stream {
        match tree {
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                if !open.is_empty() {
                    out.push(open.to_owned());
                }
                flatten(group.stream(), out);
                if !close.is_empty() {
                    out.push(close.to_owned());
                }
            }
            other => out.push(other.to_string()),
        }
    }
}

#[track_caller]
pub fn assert_contains(haystack: &str, needle: &str) {
    let (h, n) = (norm(haystack), norm(needle));
    assert!(h.contains(&n), "expected to find\n  {n}\nin\n  {h}");
}

#[track_caller]
pub fn assert_not_contains(haystack: &str, needle: &str) {
    let (h, n) = (norm(haystack), norm(needle));
    assert!(!h.contains(&n), "expected NOT to find\n  {n}\nin\n  {h}");
}

#[track_caller]
pub fn assert_starts_with(haystack: &str, prefix: &str) {
    let (h, p) = (norm(haystack), norm(prefix));
    assert!(
        h.starts_with(&p),
        "expected block to start with\n  {p}\ngot\n  {h}"
    );
}

/// Index of a fragment inside the normalized haystack, for ordering assertions.
#[track_caller]
pub fn position_of(haystack: &str, needle: &str) -> usize {
    let (h, n) = (norm(haystack), norm(needle));
    h.find(&n)
        .unwrap_or_else(|| panic!("expected to find\n  {n}\nin\n  {h}"))
}

/// The blank-line-separated blocks of a generated entity file, minus the
/// generated-file header.
pub fn blocks(content: &str) -> Vec<&str> {
    content
        .split("\n\n")
        .map(str::trim)
        .filter(|b| !b.is_empty() && !b.starts_with("//!"))
        .collect()
}

pub fn alias(name: &str) -> Alias {
    Alias::new(name)
}

pub fn col(name: &str) -> ColumnDef {
    ColumnDef::new(Alias::new(name))
}

/// `id` integer, not null, auto-increment, primary key.
pub fn serial_pk(name: &str) -> ColumnDef {
    ColumnDef::new(Alias::new(name))
        .integer()
        .not_null()
        .auto_increment()
        .primary_key()
        .to_owned()
}

/// `cake`: serial pk + nullable text name.
pub fn cake() -> TableCreateStatement {
    Table::create()
        .table(Alias::new("cake"))
        .col(serial_pk("id"))
        .col(ColumnDef::new(Alias::new("name")).text().to_owned())
        .to_owned()
}

/// `fruit`: serial pk, not-null name, nullable `cake_id` FK to `cake`.
pub fn fruit() -> TableCreateStatement {
    Table::create()
        .table(Alias::new("fruit"))
        .col(serial_pk("id"))
        .col(
            ColumnDef::new(Alias::new("name"))
                .string()
                .not_null()
                .to_owned(),
        )
        .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
        .foreign_key(
            ForeignKey::create()
                .from(Alias::new("fruit"), Alias::new("cake_id"))
                .to(Alias::new("cake"), Alias::new("id"))
                .on_delete(ForeignKeyAction::Cascade)
                .on_update(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

/// `filling`: serial pk + not-null name.
pub fn filling() -> TableCreateStatement {
    Table::create()
        .table(Alias::new("filling"))
        .col(serial_pk("id"))
        .col(
            ColumnDef::new(Alias::new("name"))
                .string()
                .not_null()
                .to_owned(),
        )
        .to_owned()
}

/// `cake_filling`: the classic junction table — two FK columns that together
/// form the primary key.
pub fn cake_filling() -> TableCreateStatement {
    Table::create()
        .table(Alias::new("cake_filling"))
        .col(
            ColumnDef::new(Alias::new("cake_id"))
                .integer()
                .not_null()
                .primary_key()
                .to_owned(),
        )
        .col(
            ColumnDef::new(Alias::new("filling_id"))
                .integer()
                .not_null()
                .primary_key()
                .to_owned(),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Alias::new("cake_filling"), Alias::new("cake_id"))
                .to(Alias::new("cake"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Alias::new("cake_filling"), Alias::new("filling_id"))
                .to(Alias::new("filling"), Alias::new("id")),
        )
        .to_owned()
}

/// The cake / fruit / filling / cake_filling schema used by most tests.
pub fn cake_schema() -> Vec<TableCreateStatement> {
    vec![cake(), cake_filling(), filling(), fruit()]
}

/// A single table built from an explicit column list.
pub fn table_with(table: &str, columns: Vec<ColumnDef>) -> TableCreateStatement {
    let mut stmt = Table::create();
    stmt.table(Alias::new(table));
    for column in columns {
        stmt.col(column);
    }
    stmt
}

/// A single-column unique index over `column`, which the transformer reads to
/// mark the column unique.
pub fn unique_index(table: &str, column: &str) -> pgorm_query::IndexCreateStatement {
    Index::create(Alias::new(column))
        .name(format!("idx_{table}_{column}"))
        .table(Alias::new(table))
        .unique()
        .to_owned()
}

pub fn enum_col(name: &str, enum_name: &str, variants: &[&str]) -> ColumnDef {
    ColumnDef::new(Alias::new(name))
        .enumeration(
            Alias::new(enum_name),
            variants.iter().map(|v| Alias::new(*v)).collect::<Vec<_>>(),
        )
        .not_null()
        .to_owned()
}

pub fn typed(name: &str, ty: ColumnType) -> ColumnDef {
    ColumnDef::new_with_type(Alias::new(name), ty)
        .not_null()
        .to_owned()
}

pub fn typed_null(name: &str, ty: ColumnType) -> ColumnDef {
    ColumnDef::new_with_type(Alias::new(name), ty)
}
