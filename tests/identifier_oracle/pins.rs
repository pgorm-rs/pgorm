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
        node: TYPE_PART_KEYWORDS,
        site: "pgorm/pipeline.select-sources.read-cast",
        labels: &[
            "keyword-select",
            "keyword-from",
            "keyword-user",
            "keyword-not",
        ],
    },
];

/// The pin covering `site` × `label`, if any.
pub fn pinned(site: &str, label: &str) -> Option<&'static Pin> {
    PINS.iter()
        .find(|pin| pin.site == site && pin.labels.contains(&label))
}
