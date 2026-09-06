//! The pipeline's typed failure channel.

/// Why a finished [`Pipeline`](super::Pipeline) could not become SQL.
///
/// Construction itself is infallible; everything that can go wrong is
/// reported here, at the [`into_sql`](super::Pipeline::into_sql) boundary,
/// never as a panic.
// [spec:pgorm:req:pipeline.errors+2]
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PipelineError {
    /// An alias collides with a name the PRQL standard library binds.
    ///
    /// The colliding set is closed: the top-level names of prqlc's `std`
    /// module plus the language's keywords, listed in the
    /// `pipeline.errors` spec rule. An alias in that set would shadow the
    /// built-in for the rest of the pipeline, so it is refused up front
    /// with the name in hand rather than surfacing later as an opaque
    /// resolution failure.
    #[error("alias `{0}` collides with a PRQL built-in name; choose another alias")]
    ReservedAlias(String),
    /// prqlc rejected the pipeline during lowering.
    ///
    /// Name-resolution failures — a PRQL built-in used as a value, a column
    /// that is not in scope after `select` — surface here, carrying the
    /// compiler's own diagnostic text.
    ///
    /// A reference to a name no stage introduced is *not* among them: with
    /// no catalog it resolves as a column of the source relation, and the
    /// server answers for it at execution.
    #[error("PRQL compilation failed: {0}")]
    Compile(String),
    /// [`select_sources`](super::Pipeline::select_sources) was asked to
    /// project entity models out of a pipeline that no longer carries its
    /// sources' column namespaces.
    ///
    /// The named stage — `select`, `group().aggregate()`, `intersect` or
    /// `remove` — replaced, collapsed or renamed the sources' own columns,
    /// so a per-source projection can no longer resolve. Refused before
    /// prqlc compiles, so the answer names the stage rather than an opaque
    /// unresolved-name diagnostic; decode a reshaped pipeline with
    /// [`into_model`](super::Pipeline::into_model) or
    /// [`into_tuple`](super::Pipeline::into_tuple) instead, or move the
    /// reshaping after the terminal cannot-follow boundary by not doing it
    /// at all — `filter`, `derive`, `sort`, `take`, `join`, `window`,
    /// `distinct` and `append` all leave the sources addressable.
    // [spec:pgorm:sem:pipeline.select-sources]
    #[error(
        "select_sources after `{0}`: the stage replaced the sources' own column namespaces, \
         so entity models can no longer be projected; list sources before reshaping, or decode \
         with into_model / into_tuple"
    )]
    ReshapedSources(&'static str),
}

// [spec:pgorm:req:pipeline.errors+2]
impl From<PipelineError> for crate::Error {
    fn from(err: PipelineError) -> Self {
        crate::Error::Query(crate::error::RuntimeError::Internal(err.to_string()))
    }
}

/// The closed set of names an alias must not take: every top-level binding of
/// prqlc 0.13's `std` module, its submodule names, and the PRQL keywords.
// [spec:pgorm:req:pipeline.errors+2]
pub(super) const RESERVED: &[&str] = &[
    "_append_by_name",
    "_eq",
    "_is_null",
    "_param",
    "add",
    "aggregate",
    "all",
    "and",
    "any",
    "append",
    "as",
    "average",
    "case",
    "coalesce",
    "concat_array",
    "count",
    "count_distinct",
    "date",
    "default_db",
    "derive",
    "div_f",
    "div_i",
    "eq",
    "false",
    "filter",
    "first",
    "from",
    "from_text",
    "func",
    "group",
    "gt",
    "gte",
    "in",
    "internal",
    "intersect",
    "into",
    "join",
    "lag",
    "last",
    "lead",
    "let",
    "loop",
    "lt",
    "lte",
    "main",
    "math",
    "max",
    "min",
    "mod",
    "module",
    "mul",
    "ne",
    "neg",
    "not",
    "null",
    "or",
    "prql",
    "rank",
    "rank_dense",
    "read_csv",
    "read_json",
    "read_parquet",
    "regex_search",
    "remove",
    "row_number",
    "select",
    "sort",
    "std",
    "stddev",
    "sub",
    "sum",
    "take",
    "text",
    "that",
    "this",
    "true",
    "tuple_map",
    "tuple_reduce",
    "tuple_reverse",
    "tuple_uniq",
    "tuple_zip",
    "window",
];
