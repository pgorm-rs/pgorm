//! Error types used in pgorm-query.

/// Result type for pgorm-query
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Column and value vector having different length
    ColValNumMismatch { col_len: usize, val_len: usize },
    /// A placeholder template could not be paired with the substitutions
    /// supplied for it.
    Template {
        /// The template as written.
        template: String,
        /// What about the pairing was rejected.
        reason: TemplateError,
    },
}

/// Why a placeholder template and its substitutions do not pair up.
///
/// A template's placeholder census is fully known the moment the template
/// meets its values, so every one of these is reported when the pair is built
/// — see [`CustomExpr::new`](crate::CustomExpr::new) and
/// [`inject_parameters`](crate::inject_parameters).
#[derive(Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// A `$` is followed by neither `$` nor a placeholder index.
    MalformedPlaceholder {
        /// Character offset of the offending `$` within the template.
        position: usize,
    },
    /// The template references `$0`; placeholders are numbered from 1.
    ZeroIndex,
    /// The template references a placeholder beyond the substitutions supplied.
    IndexOutOfRange {
        /// The index referenced.
        index: usize,
        /// How many substitutions were supplied.
        supplied: usize,
    },
    /// A supplied substitution is never referenced by the template.
    UnreferencedValue {
        /// The 1-based index of the substitution left unused.
        index: usize,
        /// How many substitutions were supplied.
        supplied: usize,
    },
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::ColValNumMismatch { col_len, val_len } => write!(
                f,
                "Columns and values length mismatch: {col_len} != {val_len}"
            ),
            Self::Template { template, reason } => {
                write!(f, "Invalid placeholder template \"{template}\": {reason}")
            }
        }
    }
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::MalformedPlaceholder { position } => write!(
                f,
                "the `$` at character {position} is not a placeholder; write `$$` for a literal `$`"
            ),
            Self::ZeroIndex => {
                write!(f, "placeholders are numbered from 1, so `$0` names nothing")
            }
            Self::IndexOutOfRange { index, supplied } => write!(
                f,
                "`${index}` is referenced but only {supplied} substitution(s) were supplied"
            ),
            Self::UnreferencedValue { index, supplied } => write!(
                f,
                "{supplied} substitution(s) were supplied but `${index}` is never referenced"
            ),
        }
    }
}
