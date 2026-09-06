//! Placeholder templates.
//!
//! A `$N` template's placeholder census is knowable the moment the template
//! meets the values it substitutes, so this module resolves the two against
//! each other up front and hands back a flat sequence of [`Segment`]s that
//! carries no indices at all. Rendering a resolved sequence is a walk over
//! text and values; there is nothing left to look up, and so nothing left to
//! be out of range.

use crate::backend::QueryBuilder;
use crate::error::{Error, Result, TemplateError};
use crate::expr::SimpleExpr;
use crate::token::{Token, Tokenizer};

/// One piece of a scanned template: literal text, or a reference to the `N`th
/// substitution as the template wrote it (1-based, unvalidated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Chunk {
    Text(String),
    Placeholder(usize),
}

/// One piece of a resolved template. The placeholder indices are gone: each
/// reference has already been paired with the value it names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Segment<T> {
    Text(String),
    Value(T),
}

/// Which `$` convention a scan follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grammar {
    /// Custom-expression templates: `$$` escapes a literal `$`, and a `$` that
    /// is neither an escape nor a placeholder index is refused, because a
    /// template is authored for this machinery and a stray `$` in one is a
    /// mistake rather than data.
    Template,
    /// Real SQL being re-inlined: only `$N` is a placeholder and every other
    /// token — `$$` included, which opens a dollar-quoted body in PostgreSQL —
    /// is reproduced verbatim.
    Sql,
}

/// Split `input` into literal text and placeholder references.
// [spec:pgorm:req:sql.render.custom-expr+1] (the `Template` grammar: `$$` escape, `$N` index)
// [spec:pgorm:sem:sql.render.inject+2] (the `Sql` grammar: only `$N`, everything else verbatim)
pub(crate) fn scan(input: &str, grammar: Grammar) -> Result<Vec<Chunk>> {
    let mark = QueryBuilder.placeholder().0;
    let tokens: Vec<Token> = Tokenizer::new(input).iter().collect();

    let mut offsets = Vec::with_capacity(tokens.len());
    let mut offset = 0usize;
    for token in &tokens {
        offsets.push(offset);
        offset += token.as_str().chars().count();
    }

    let mut chunks = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < tokens.len() {
        let Token::Punctuation(punct) = &tokens[i] else {
            text.push_str(tokens[i].as_str());
            i += 1;
            continue;
        };
        if punct.as_str() != mark {
            text.push_str(punct);
            i += 1;
            continue;
        }

        let next = tokens.get(i + 1);
        let index = match next {
            Some(Token::Unquoted(word)) => word.parse::<usize>().ok(),
            _ => None,
        };
        if let Some(index) = index {
            if !text.is_empty() {
                chunks.push(Chunk::Text(std::mem::take(&mut text)));
            }
            chunks.push(Chunk::Placeholder(index));
            i += 2;
            continue;
        }

        let escaped = matches!(next, Some(Token::Punctuation(p)) if p.as_str() == mark);
        match grammar {
            Grammar::Template if escaped => {
                text.push_str(mark);
                i += 2;
            }
            Grammar::Template => {
                return Err(Error::Template {
                    template: input.to_owned(),
                    reason: TemplateError::MalformedPlaceholder {
                        position: offsets.get(i).copied().unwrap_or_default(),
                    },
                });
            }
            Grammar::Sql => {
                text.push_str(punct);
                i += 1;
            }
        }
    }
    if !text.is_empty() {
        chunks.push(Chunk::Text(text));
    }
    Ok(chunks)
}

/// Check the census of `chunks` against `supplied`: the distinct placeholder
/// indices referenced must be exactly `1..=supplied`.
// [spec:pgorm:req:sql.render.custom-expr+1]
// [spec:pgorm:sem:sql.render.inject+2]
fn census(input: &str, chunks: &[Chunk], supplied: usize) -> Result<()> {
    let mut referenced: Vec<usize> = chunks
        .iter()
        .filter_map(|chunk| match chunk {
            Chunk::Placeholder(index) => Some(*index),
            Chunk::Text(_) => None,
        })
        .collect();
    referenced.sort_unstable();
    referenced.dedup();

    let fail = |reason| Error::Template {
        template: input.to_owned(),
        reason,
    };

    for &index in &referenced {
        if index == 0 {
            return Err(fail(TemplateError::ZeroIndex));
        }
        if index > supplied {
            return Err(fail(TemplateError::IndexOutOfRange { index, supplied }));
        }
    }
    for index in 1..=supplied {
        if referenced.binary_search(&index).is_err() {
            return Err(fail(TemplateError::UnreferencedValue { index, supplied }));
        }
    }
    Ok(())
}

/// Scan `input` and pair every placeholder with the value it names, or refuse
/// the pairing. The result holds values, not indices.
// [spec:pgorm:req:sql.render.custom-expr+1]
// [spec:pgorm:sem:sql.render.inject+2]
pub(crate) fn resolve<T>(input: &str, grammar: Grammar, values: &[T]) -> Result<Vec<Segment<T>>>
where
    T: Clone,
{
    let chunks = scan(input, grammar)?;
    census(input, &chunks, values.len())?;

    chunks
        .into_iter()
        .map(|chunk| match chunk {
            Chunk::Text(text) => Ok(Segment::Text(text)),
            Chunk::Placeholder(index) => index
                .checked_sub(1)
                .and_then(|i| values.get(i))
                .map(|value| Segment::Value(value.clone()))
                .ok_or_else(|| Error::Template {
                    template: input.to_owned(),
                    reason: TemplateError::IndexOutOfRange {
                        index,
                        supplied: values.len(),
                    },
                }),
        })
        .collect()
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]
// [spec:pgorm:sem:sql.render.inject+2/test]
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn text(s: &str) -> Chunk {
        Chunk::Text(s.to_owned())
    }

    #[test]
    fn scans_placeholders_and_text() {
        assert_eq!(
            scan("6 = $1 * $2", Grammar::Template).unwrap(),
            vec![
                text("6 = "),
                Chunk::Placeholder(1),
                text(" * "),
                Chunk::Placeholder(2),
            ]
        );
    }

    #[test]
    fn escaped_dollar_is_literal_text() {
        assert_eq!(
            scan("$1 $$ $2", Grammar::Template).unwrap(),
            vec![Chunk::Placeholder(1), text(" $ "), Chunk::Placeholder(2),]
        );
    }

    #[test]
    fn quoted_placeholders_are_opaque() {
        assert_eq!(
            scan("'$1' = $1", Grammar::Template).unwrap(),
            vec![text("'$1' = "), Chunk::Placeholder(1)]
        );
    }

    #[test]
    fn template_refuses_a_bare_dollar() {
        assert_eq!(
            scan("a $ b", Grammar::Template).unwrap_err(),
            Error::Template {
                template: "a $ b".to_owned(),
                reason: TemplateError::MalformedPlaceholder { position: 2 },
            }
        );
    }

    #[test]
    fn template_refuses_a_trailing_dollar() {
        assert_eq!(
            scan("a $", Grammar::Template).unwrap_err(),
            Error::Template {
                template: "a $".to_owned(),
                reason: TemplateError::MalformedPlaceholder { position: 2 },
            }
        );
    }

    #[test]
    fn template_refuses_a_non_numeric_placeholder() {
        assert_eq!(
            scan("$abc", Grammar::Template).unwrap_err(),
            Error::Template {
                template: "$abc".to_owned(),
                reason: TemplateError::MalformedPlaceholder { position: 0 },
            }
        );
    }

    #[test]
    fn sql_grammar_passes_stray_dollars_through() {
        assert_eq!(
            scan("$$body$$ $1", Grammar::Sql).unwrap(),
            vec![text("$$body$$ "), Chunk::Placeholder(1)]
        );
    }

    #[test]
    fn resolve_pairs_repeated_references() {
        assert_eq!(
            resolve("$1 + $1", Grammar::Template, &["a"]).unwrap(),
            vec![
                Segment::Value("a"),
                Segment::Text(" + ".to_owned()),
                Segment::Value("a"),
            ]
        );
    }

    #[test]
    fn resolve_refuses_under_supply() {
        assert_eq!(
            resolve("$1 + $2", Grammar::Template, &["a"]).unwrap_err(),
            Error::Template {
                template: "$1 + $2".to_owned(),
                reason: TemplateError::IndexOutOfRange {
                    index: 2,
                    supplied: 1
                },
            }
        );
    }

    #[test]
    fn resolve_refuses_over_supply() {
        assert_eq!(
            resolve("$1", Grammar::Template, &["a", "b"]).unwrap_err(),
            Error::Template {
                template: "$1".to_owned(),
                reason: TemplateError::UnreferencedValue {
                    index: 2,
                    supplied: 2
                },
            }
        );
    }

    #[test]
    fn resolve_refuses_an_arity_hole() {
        assert_eq!(
            resolve("$1 + $3", Grammar::Template, &["a", "b", "c"]).unwrap_err(),
            Error::Template {
                template: "$1 + $3".to_owned(),
                reason: TemplateError::UnreferencedValue {
                    index: 2,
                    supplied: 3
                },
            }
        );
    }

    #[test]
    fn resolve_refuses_a_zero_index() {
        assert_eq!(
            resolve("$0", Grammar::Template, &["a"]).unwrap_err(),
            Error::Template {
                template: "$0".to_owned(),
                reason: TemplateError::ZeroIndex,
            }
        );
    }

    #[test]
    fn resolve_accepts_a_template_with_no_placeholders() {
        assert_eq!(
            resolve::<&str>("now()", Grammar::Template, &[]).unwrap(),
            vec![Segment::Text("now()".to_owned())]
        );
    }
}

/// A custom SQL template and the expressions it substitutes, already resolved
/// against each other.
///
/// A template's placeholder census — which `$N` it names, and how many values
/// it therefore needs — is knowable the moment the template meets its values,
/// so [`CustomExpr::new`] settles it there. What survives construction is a
/// flat sequence of literal text and resolved expressions carrying no indices
/// at all, which is why rendering a custom expression cannot reach for a
/// substitution that was never supplied.
///
/// Reached through [`Expr::cust_with_values`], [`Expr::cust_with_expr`] and
/// [`Expr::cust_with_exprs`].
// [spec:pgorm:req:sql.render.custom-expr+1]
#[derive(Debug, Clone, PartialEq)]
pub struct CustomExpr {
    segments: Vec<Segment<SimpleExpr>>,
}

impl CustomExpr {
    /// Pair `template` with `values`, or refuse the pair.
    ///
    /// The template is tokenized, so placeholder-like text inside quoted
    /// tokens is left alone. `$$` writes a literal `$`; `$N` names the `N`th
    /// value counting from 1, and may be written as many times as you like.
    /// Any other `$` — trailing, `$0`, `$abc`, or one standing before a space
    /// or punctuation — is a malformed placeholder and is refused.
    ///
    /// The distinct indices the template references MUST be exactly
    /// `1..=values.len()`: a reference past the end
    /// ([`TemplateError::IndexOutOfRange`](crate::error::TemplateError::IndexOutOfRange))
    /// and a value the template never names
    /// ([`TemplateError::UnreferencedValue`](crate::error::TemplateError::UnreferencedValue),
    /// which is also what an arity hole like `$1, $3` over three values
    /// reports) are both refused rather than silently rendered.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let paired = CustomExpr::new("$1 * $2", [Expr::val(2).into(), Expr::val(3).into()]);
    /// assert!(paired.is_ok());
    ///
    /// let short = CustomExpr::new("$1 * $2", [Expr::val(2).into()]);
    /// assert!(short.is_err());
    /// ```
    // [spec:pgorm:req:sql.render.custom-expr+1]
    pub fn new<T, I>(template: T, values: I) -> Result<Self>
    where
        T: Into<String>,
        I: IntoIterator<Item = SimpleExpr>,
    {
        let template = template.into();
        let values: Vec<SimpleExpr> = values.into_iter().collect();
        Ok(Self {
            segments: resolve(&template, Grammar::Template, &values)?,
        })
    }

    pub(crate) fn segments(&self) -> &[Segment<SimpleExpr>] {
        &self.segments
    }
}
