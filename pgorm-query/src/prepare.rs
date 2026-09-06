//! Helper for preparing SQL statements.

use crate::error::Result;
use crate::template::{self, Grammar, Segment};
use crate::*;
pub use std::fmt::Write;

// [spec:pgorm:def:sql.render.writer+2]
pub trait SqlWriter: Write + ToString {
    fn push_param(&mut self, value: Value);

    /// Push a parameter that sits where Postgres would infer the placeholder's
    /// type from the surrounding expression rather than from the value being
    /// bound. Sinks that emit placeholders pin the type; sinks that render the
    /// value inline have nothing to pin and fall back to [`Self::push_param`].
    // [spec:pgorm:req:sql.render.cast-param-type]
    fn push_param_source_typed(&mut self, value: Value) {
        self.push_param(value)
    }

    fn as_writer(&mut self) -> &mut dyn Write;
}

// [spec:pgorm:def:sql.render.writer+2] (inline-rendering sink for the Display path)
impl SqlWriter for String {
    fn push_param(&mut self, value: Value) {
        self.push_str(&QueryBuilder.value_to_string(&value))
    }

    fn as_writer(&mut self) -> &mut dyn Write {
        self as _
    }
}

#[derive(Debug, Clone)]
pub struct SqlWriterValues {
    counter: usize,
    placeholder: String,
    numbered: bool,
    string: String,
    values: Vec<Value>,
}

impl SqlWriterValues {
    pub fn new<T>(placeholder: T, numbered: bool) -> Self
    where
        T: Into<String>,
    {
        Self {
            counter: 0,
            placeholder: placeholder.into(),
            numbered,
            string: String::with_capacity(256),
            values: Vec::new(),
        }
    }

    pub fn into_parts(self) -> (String, Values) {
        (self.string, Values(self.values))
    }
}

impl Write for SqlWriterValues {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        write!(self.string, "{s}")
    }
}

impl std::fmt::Display for SqlWriterValues {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}

// [spec:pgorm:req:sql.render.placeholders] (counter increments then emits $N; value collected)
// [spec:pgorm:def:sql.render.writer+2]
impl SqlWriter for SqlWriterValues {
    fn push_param(&mut self, value: Value) {
        self.counter += 1;
        if self.numbered {
            let counter = self.counter;
            write!(self.string, "{}{}", self.placeholder, counter).unwrap();
        } else {
            write!(self.string, "{}", self.placeholder).unwrap();
        }
        self.values.push(value)
    }

    // [spec:pgorm:req:sql.render.cast-param-type]
    fn push_param_source_typed(&mut self, value: Value) {
        let source_type = value.source_type_name();
        self.push_param(value);
        if let Some(source_type) = source_type {
            write!(self.string, "::{source_type}").unwrap();
        }
    }

    fn as_writer(&mut self) -> &mut dyn Write {
        self as _
    }
}

/// Convert a parameterized SQL string back into inline SQL, or refuse the
/// pairing.
///
/// A `$` followed by an unquoted integer `N` references parameter `N`,
/// counting from 1; every other token — including a `$` that opens a
/// dollar-quoted body — is reproduced verbatim, because what arrives here is
/// real SQL rather than a template authored for this machinery. Quoted tokens
/// are opaque to the tokenizer, so a `$N` inside a string literal or a quoted
/// identifier is neither substituted nor counted.
///
/// The census is settled before anything is written: the distinct indices the
/// SQL references must be exactly `1..=params.len()`, so `$0`, a reference
/// past the end, or a parameter the SQL never names comes back as an
/// [`Error`](crate::error::Error) instead of panicking on the vector index.
/// Each reference is paired with its parameter up front, which is why the
/// writing walk holds no indices at all. A parameter may be referenced any
/// number of times.
// [spec:pgorm:sem:sql.render.inject+2]
pub fn inject_parameters<I>(sql: &str, params: I) -> Result<String>
where
    I: IntoIterator<Item = Value>,
{
    let params: Vec<Value> = params.into_iter().collect();
    let segments = template::resolve(sql, Grammar::Sql, &params)?;

    let mut output = String::with_capacity(sql.len());
    for segment in segments {
        match segment {
            Segment::Text(text) => output.push_str(&text),
            Segment::Value(value) => output.push_str(&QueryBuilder.value_to_string(&value)),
        }
    }
    Ok(output)
}

// [spec:pgorm:sem:sql.render.inject+2/test]
#[cfg(test)]
mod tests_postgres {
    use super::*;
    use crate::error::{Error, TemplateError};
    use pretty_assertions::assert_eq;

    #[test]
    fn inject_parameters_5() {
        assert_eq!(
            inject_parameters("WHERE A = $1 AND C = $2", ["B".into(), "D".into()]).unwrap(),
            "WHERE A = 'B' AND C = 'D'"
        );
    }

    #[test]
    fn inject_parameters_6() {
        assert_eq!(
            inject_parameters("WHERE A = $2 AND C = $1", ["B".into(), "D".into()]).unwrap(),
            "WHERE A = 'D' AND C = 'B'"
        );
    }

    #[test]
    fn inject_parameters_7() {
        assert_eq!(
            inject_parameters("WHERE A = $1", ["B'C".into()]).unwrap(),
            "WHERE A = E'B\\'C'"
        );
    }

    #[test]
    fn a_parameter_may_be_referenced_repeatedly() {
        assert_eq!(
            inject_parameters("WHERE A = $1 OR B = $1", ["B".into()]).unwrap(),
            "WHERE A = 'B' OR B = 'B'"
        );
    }

    #[test]
    fn a_stray_dollar_is_reproduced_verbatim() {
        assert_eq!(
            inject_parameters("WHERE A = $$body$$ AND C = $1", ["D".into()]).unwrap(),
            "WHERE A = $$body$$ AND C = 'D'"
        );
    }

    #[test]
    fn a_quoted_placeholder_is_not_a_reference() {
        assert_eq!(
            inject_parameters("WHERE A = '$2' AND C = $1", ["D".into()]).unwrap(),
            "WHERE A = '$2' AND C = 'D'"
        );
    }

    #[test]
    fn an_out_of_range_reference_is_refused() {
        assert_eq!(
            inject_parameters("WHERE A = $2", ["B".into()]).unwrap_err(),
            Error::Template {
                template: "WHERE A = $2".to_owned(),
                reason: TemplateError::IndexOutOfRange {
                    index: 2,
                    supplied: 1
                },
            }
        );
    }

    #[test]
    fn an_unreferenced_parameter_is_refused() {
        assert_eq!(
            inject_parameters("WHERE A = $1", ["B".into(), "D".into()]).unwrap_err(),
            Error::Template {
                template: "WHERE A = $1".to_owned(),
                reason: TemplateError::UnreferencedValue {
                    index: 2,
                    supplied: 2
                },
            }
        );
    }

    #[test]
    fn a_zero_reference_is_refused() {
        assert_eq!(
            inject_parameters("WHERE A = $0", ["B".into()]).unwrap_err(),
            Error::Template {
                template: "WHERE A = $0".to_owned(),
                reason: TemplateError::ZeroIndex,
            }
        );
    }
}
