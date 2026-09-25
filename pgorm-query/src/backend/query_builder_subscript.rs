//! Array subscripts: `base[i]` and `base[lower:upper]`.

use super::*;

impl QueryBuilder {
    /// Translate a [`SimpleExpr::Subscript`] into SQL.
    ///
    /// PostgreSQL's grammar admits a subscript directly after a column
    /// reference, a parameter, or a parenthesised expression or subquery, and
    /// nowhere else: `f(x)[1]`, `CAST(x AS int[])[1]` and `ARRAY[1,2][1]` are
    /// all syntax errors. So the base is written bare only when it is a column
    /// reference, or is itself subscripted — `"a"[1][2]` is one
    /// multi-dimensional access, where `("a"[1])[2]` would subscript the first
    /// access's element-typed result, which the server refuses — and
    /// parenthesised otherwise. A bound value is
    /// parenthesised too: `$1[1]` would parse, but the same value inlined by
    /// `to_string` is a literal, which would not.
    // [spec:pgorm:req:sql.render.subscript]
    pub(super) fn prepare_subscript(
        &self,
        base: &SimpleExpr,
        subscript: &Subscript,
        sql: &mut dyn SqlWriter,
    ) {
        let bare = matches!(base, SimpleExpr::Column(_) | SimpleExpr::Subscript(..));
        if bare {
            self.prepare_simple_expr(base, sql);
        } else {
            write!(sql, "(").unwrap();
            self.prepare_simple_expr(base, sql);
            write!(sql, ")").unwrap();
        }

        write!(sql, "[").unwrap();
        match subscript {
            Subscript::Index(index) => self.prepare_simple_expr(index, sql),
            Subscript::Slice(lower, upper) => {
                if let Some(lower) = lower {
                    self.prepare_simple_expr(lower, sql);
                }
                write!(sql, ":").unwrap();
                if let Some(upper) = upper {
                    self.prepare_simple_expr(upper, sql);
                }
            }
        }
        write!(sql, "]").unwrap();
    }
}
