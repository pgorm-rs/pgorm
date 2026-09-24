//! Defects the oracle found, filed as plan nodes and pinned here until they
//! are fixed.
//!
//! A pin names the site, the corpus entries that fail there today, and the
//! node that fixes them. The main property skips pinned pairs; a separate test
//! asserts each still fails and prints the failure, so the defect is reported
//! on every run and the pin cannot outlive its fix.

/// One pinned defect.
pub struct Pin {
    /// The plan node that fixes it.
    pub node: &'static str,
    pub site: &'static str,
    /// Corpus labels that fail at `site` today.
    pub labels: &'static [&'static str],
}

/// `TypeName::prepare_part` writes any `^[a-z_][a-z0-9_]*$` part bare, and
/// keywords match: `Func::named("not")` renders `not(1)`, a boolean NOT
/// rather than a call, and reserved words at type, function, schema and
/// access-method positions are syntax errors, so an object so named cannot be
/// referenced. The type-sugar reading (`integer` as a type) is intended and is
/// not pinned; the oracle accepts it as `Verdict::TypeKeyword`.
const TYPE_PART_KEYWORDS: &str = "type-part-keyword-names";

/// prqlc writes a name bare whenever its `valid_ident` accepts it, and that
/// admits a leading `$` and a lone `*`: `col(t, "$1")` renders `SELECT $1`, a
/// bound parameter where a column was named; two `$$` names open and close a
/// dollar-quoted string that swallows the SQL between them; `*` is the
/// wildcard. pgorm's screen refuses only `"` and NUL.
const PIPELINE_DOLLAR_NAMES: &str = "pipeline-bare-dollar-names";

/// The four corpus names prqlc leaves bare that PostgreSQL does not read as a
/// name.
const DOLLAR_NAMES: &[&str] = &["placeholder", "dollar-dollar", "dollar-tag", "asterisk"];

/// `select_sources` hands a column's read-cast type to prqlc as raw text in a
/// `noresolve` slot, which prqlc writes verbatim: the type name is SQL.
const READ_CAST_VERBATIM: &str = "pipeline-read-cast-verbatim";

/// The pins.
pub const PINS: &[Pin] = &[
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "query/select.from-function.name",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
            "keyword-integer",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "query/expr.func.named",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
            "keyword-integer",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "query/expr.cast.type",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "query/expr.cast.schema",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
            "keyword-integer",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "query/expr.cast.array",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "query/expr.as-enum.type",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "ddl/create-table.column-type-named",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "ddl/create-table.column-type-schema",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
            "keyword-integer",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "ddl/create-table.column-type-array",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "ddl/create-table.column-type-enum",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
        ],
    },
    Pin {
        node: TYPE_PART_KEYWORDS,
        site: "ddl/create-index.access-method",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
            "keyword-left",
        ],
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.from.relation",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.from-schema.schema",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.from-schema.table",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.col.column",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.col.table",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.name.filter",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.name.sort",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.name.group",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.this.column",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.that.column",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.join.relation",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.append.relation",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.as-runtime.derive",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.as-runtime.select",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.named-runtime.alias",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.select-sources.qualifier",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: PIPELINE_DOLLAR_NAMES,
        site: "pgorm/pipeline.entity.table-name",
        labels: DOLLAR_NAMES,
    },
    Pin {
        node: READ_CAST_VERBATIM,
        site: "pgorm/pipeline.select-sources.read-cast",
        labels: &[
            "semicolon",
            "line-comment",
            "block-comment-open",
            "block-comment-close",
            "placeholder",
            "dollar-dollar",
            "dollar-tag",
            "single-quote",
            "escape-string-open",
            "backslash",
            "space",
            "inner-space",
            "newline",
            "inner-newline",
            "tab",
            "carriage-return",
            "mixed-case",
            "asterisk",
            "dotted",
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
            "keyword-integer",
            "empty",
        ],
    },
];

/// The pin covering `site` × `label`, if any.
pub fn pinned(site: &str, label: &str) -> Option<&'static Pin> {
    PINS.iter()
        .find(|pin| pin.site == site && pin.labels.contains(&label))
}
