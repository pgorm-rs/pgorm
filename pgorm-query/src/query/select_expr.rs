use crate::{WindowStatement, expr::SimpleExpr, types::*};

/// Window type in [`SelectExpr`]
// [spec:pgorm:def:sql.ast.window-statement+2]
#[derive(Debug, Clone, PartialEq)]
pub enum WindowSelectType {
    /// Name in [`SelectStatement`][crate::SelectStatement]
    Name(DynIden),
    /// Inline query in [`SelectExpr`]
    Query(WindowStatement),
}

/// Select expression used in select statement
///
/// The projected expression and its window are read-only after construction:
/// PostgreSQL admits `OVER` only after a function call, so the pairing is
/// established by the
/// [`SelectStatement::expr_window`][crate::SelectStatement::expr_window]
/// family — which takes a [`FunctionCall`][crate::FunctionCall] — and cannot
/// be taken apart afterwards.
// [spec:pgorm:def:sql.ast.window-statement+2]
#[derive(Debug, Clone, PartialEq)]
pub struct SelectExpr {
    pub(crate) expr: SimpleExpr,
    pub alias: Option<DynIden>,
    pub(crate) window: Option<WindowSelectType>,
}

impl SelectExpr {
    /// A projection of `expr` under no alias and no window.
    // [spec:pgorm:def:sql.ast.window-statement+2]
    pub fn new<T>(expr: T) -> Self
    where
        T: Into<SimpleExpr>,
    {
        Self {
            expr: expr.into(),
            alias: None,
            window: None,
        }
    }

    /// A projection of `expr` under `alias`.
    // [spec:pgorm:def:sql.ast.window-statement+2]
    pub fn new_as<T, A>(expr: T, alias: A) -> Self
    where
        T: Into<SimpleExpr>,
        A: IntoIden,
    {
        Self {
            expr: expr.into(),
            alias: Some(alias.into_iden()),
            window: None,
        }
    }

    /// The projected expression.
    // [spec:pgorm:def:sql.ast.window-statement+2]
    pub fn expr(&self) -> &SimpleExpr {
        &self.expr
    }

    /// The window this projection is evaluated over, if any.
    // [spec:pgorm:def:sql.ast.window-statement+2]
    pub fn window(&self) -> Option<&WindowSelectType> {
        self.window.as_ref()
    }
}

impl<T> From<T> for SelectExpr
where
    T: Into<SimpleExpr>,
{
    fn from(expr: T) -> Self {
        SelectExpr::new(expr)
    }
}
