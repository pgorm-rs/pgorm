//! A SELECT's WINDOW clause: the windows a statement names, in the order it
//! names them, each a window of its own or one built on an earlier entry, and
//! the check that refuses the clauses PostgreSQL refuses.

use inherent::inherent;

use crate::{
    FrameClause, OrderedStatement, SelectStatement, WindowSelectType, WindowStatement,
    error::{Error, Result},
    expr::SimpleExpr,
    types::*,
};

/// A window built on an earlier entry of the same WINDOW clause, rendered
/// `( "base" ORDER BY … <frame> )`.
///
/// PostgreSQL copies the base window's PARTITION BY, and its ORDER BY when it
/// has one, so this window adds only what the base leaves open: an ORDER BY
/// where the base has none, and a frame. It has no PARTITION BY of its own,
/// because PostgreSQL refuses one (`42P20`). What depends on the base — that
/// it is an earlier entry, that it has no frame, and that it does not order
/// its rows when this window does — is read by
/// [`SelectStatement::check_windows`].
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::select()
///     .from(Char::Table)
///     .expr_window_name(Func::count(Expr::col(Char::Id)), Name::runtime("running"))
///     .window(Name::runtime("by_size"), WindowStatement::partition_by(Char::FontSize))
///     .window(
///         Name::runtime("running"),
///         ExtendedWindow::new(Name::runtime("by_size"))
///             .order_by(Char::Id, Order::Asc)
///             .frame(FrameType::Rows.unbounded_preceding())
///             .to_owned(),
///     )
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     [
///         r#"SELECT COUNT("id") OVER "running" FROM "character""#,
///         r#"WINDOW "by_size" AS ( PARTITION BY "font_size" ),"#,
///         r#""running" AS ( "by_size" ORDER BY "id" ASC ROWS UNBOUNDED PRECEDING )"#,
///     ]
///     .join(" ")
/// );
/// assert_eq!(query.check_windows(), Ok(()));
/// ```
///
/// There is no PARTITION BY to give it:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// ExtendedWindow::new(Name::runtime("by_size")).partition_by(Char::SizeW);
/// ```
// [spec:pgorm:def:sql.ast.window-statement+8]
#[derive(Debug, Clone, PartialEq)]
pub struct ExtendedWindow {
    pub(crate) base: Name,
    pub(crate) order_by: Vec<OrderExpr>,
    pub(crate) frame: Option<FrameClause>,
}

impl ExtendedWindow {
    /// A window copying the partitioning, and any ordering, of the entry the
    /// WINDOW clause names `base`.
    // [spec:pgorm:def:sql.ast.window-statement+8]
    pub fn new<B>(base: B) -> Self
    where
        B: IntoName,
    {
        Self {
            base: base.into_name(),
            order_by: Vec::new(),
            frame: None,
        }
    }

    /// Sets the window's frame, replacing any frame already set, as
    /// [`WindowStatement::frame`] does.
    // [spec:pgorm:def:sql.ast.window-statement+8]
    pub fn frame<F>(&mut self, frame: F) -> &mut Self
    where
        F: Into<FrameClause>,
    {
        self.frame = Some(frame.into());
        self
    }
}

#[inherent]
impl OrderedStatement for ExtendedWindow {
    pub fn add_order_by(&mut self, order: OrderExpr) -> &mut Self {
        self.order_by.push(order);
        self
    }

    pub fn clear_order_by(&mut self) -> &mut Self {
        self.order_by = Vec::new();
        self
    }

    pub fn order_by<T>(&mut self, col: T, order: Order) -> &mut Self
    where
        T: IntoColumnRef;

    pub fn order_by_expr(&mut self, expr: SimpleExpr, order: Order) -> &mut Self;
    pub fn order_by_columns<I, T>(&mut self, cols: I) -> &mut Self
    where
        T: IntoColumnRef,
        I: IntoIterator<Item = (T, Order)>;
    pub fn order_by_with_nulls<T>(
        &mut self,
        col: T,
        order: Order,
        nulls: NullOrdering,
    ) -> &mut Self
    where
        T: IntoColumnRef;
    pub fn order_by_expr_with_nulls(
        &mut self,
        expr: SimpleExpr,
        order: Order,
        nulls: NullOrdering,
    ) -> &mut Self;
    pub fn order_by_columns_with_nulls<I, T>(&mut self, cols: I) -> &mut Self
    where
        T: IntoColumnRef,
        I: IntoIterator<Item = (T, Order, NullOrdering)>;
}

/// What an entry of a WINDOW clause defines: a window of its own, or one
/// built on an earlier entry.
// [spec:pgorm:def:sql.ast.window-statement+8]
#[derive(Debug, Clone, PartialEq)]
pub enum WindowDefinition {
    /// `( PARTITION BY … ORDER BY … <frame> )`.
    Spec(WindowStatement),
    /// `( "base" ORDER BY … <frame> )`.
    Extended(ExtendedWindow),
}

impl From<WindowStatement> for WindowDefinition {
    fn from(window: WindowStatement) -> Self {
        Self::Spec(window)
    }
}

impl From<ExtendedWindow> for WindowDefinition {
    fn from(window: ExtendedWindow) -> Self {
        Self::Extended(window)
    }
}

/// A definition written inline after `OVER`, as a projection holds it.
impl From<WindowDefinition> for WindowSelectType {
    fn from(window: WindowDefinition) -> Self {
        match window {
            WindowDefinition::Spec(window) => Self::Query(window),
            WindowDefinition::Extended(window) => Self::Extended(window),
        }
    }
}

/// One entry of a WINDOW clause, `"name" AS ( … )`.
// [spec:pgorm:def:sql.ast.window-statement+8]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NamedWindow {
    pub(crate) name: Name,
    pub(crate) definition: WindowDefinition,
}

/// Whether two names are one window's. PostgreSQL compares the identifiers'
/// text, and pgorm quotes every identifier, so two names spelling the same
/// text are one window whichever types minted them.
fn same_window(name: &Name, other: &Name) -> bool {
    name.to_string() == other.to_string()
}

impl SelectStatement {
    /// Append a named window to the WINDOW clause, `"name" AS ( … )`, which a
    /// projection reads with
    /// [`expr_window_name`](Self::expr_window_name): a [`WindowStatement`],
    /// or an [`ExtendedWindow`] built on an entry appended before it.
    ///
    /// Each call adds an entry, and the clause renders them in call order.
    /// A name the clause already holds, and an extended window whose base is
    /// not an earlier entry, are refused by
    /// [`check_windows`](Self::check_windows) rather than at this call, which
    /// has no error to return.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .from(Char::Table)
    ///     .expr_window_name_as(Func::count(Expr::col(Char::Id)), Name::runtime("w"), Name::runtime("C"))
    ///     .expr_window_name_as(Func::max(Expr::col(Char::Id)), Name::runtime("v"), Name::runtime("M"))
    ///     .window(Name::runtime("w"), WindowStatement::partition_by(Char::FontSize))
    ///     .window(Name::runtime("v"), WindowStatement::partition_by(Char::SizeW))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     [
    ///         r#"SELECT COUNT("id") OVER "w" AS "C", MAX("id") OVER "v" AS "M" FROM "character""#,
    ///         r#"WINDOW "w" AS ( PARTITION BY "font_size" ), "v" AS ( PARTITION BY "size_w" )"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    // [spec:pgorm:def:sql.ast.window-statement+8]
    pub fn window<A, W>(&mut self, name: A, window: W) -> &mut Self
    where
        A: IntoName,
        W: Into<WindowDefinition>,
    {
        self.windows.push(NamedWindow {
            name: name.into_name(),
            definition: window.into(),
        });
        self
    }

    /// Refuse a WINDOW clause, or a reference to one, that PostgreSQL would
    /// refuse:
    ///
    /// - a name two entries share ([`Error::WindowRedefined`], `42P20`);
    /// - an extended window whose base is no earlier entry
    ///   ([`Error::WindowBaseUndefined`], `42704`), whose base has a frame
    ///   ([`Error::WindowFrameCopied`], `42P20`), or which adds an ORDER BY to
    ///   a base that orders its rows, by its own ORDER BY or one it copied
    ///   ([`Error::WindowOrderOverridden`], `42P20`);
    /// - a projection over a window name no entry declares
    ///   ([`Error::WindowUndefined`], `42704`), and one over an inline
    ///   extended window whose base no entry declares, has a frame, or orders
    ///   its rows when the inline window adds an ORDER BY — the refusals of
    ///   an extended entry, except that the base may be any entry, the inline
    ///   window being read after the whole clause.
    ///
    /// Rendering is infallible, so it writes what the statement holds and
    /// leaves the refusal to this check; pgorm's executors and the Python
    /// and JavaScript bindings call it before they render. It reads this
    /// statement's own clauses only: a SELECT nested in it names its windows
    /// for itself and is checked by the server.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, error::Error, tests_cfg::*};
    ///
    /// let mut query = Query::select();
    /// query
    ///     .expr_window_name(Func::count(Expr::col(Char::Id)), Name::runtime("w"))
    ///     .from(Char::Table);
    /// assert_eq!(
    ///     query.check_windows(),
    ///     Err(Error::WindowUndefined { name: "w".to_owned() })
    /// );
    ///
    /// query.window(Name::runtime("w"), WindowStatement::partition_by(Char::FontSize));
    /// assert_eq!(query.check_windows(), Ok(()));
    ///
    /// query.window(Name::runtime("w"), WindowStatement::new());
    /// assert_eq!(
    ///     query.check_windows(),
    ///     Err(Error::WindowRedefined { name: "w".to_owned() })
    /// );
    /// ```
    // [spec:pgorm:def:sql.ast.window-statement+8]
    pub fn check_windows(&self) -> Result<()> {
        // Whether each entry so far orders its rows: an extended window
        // carries its base's ORDER BY, so the answer is read down the chain.
        let mut ordered = Vec::with_capacity(self.windows.len());
        for (at, entry) in self.windows.iter().enumerate() {
            let earlier = &self.windows[..at];
            if earlier
                .iter()
                .any(|other| same_window(&other.name, &entry.name))
            {
                return Err(Error::WindowRedefined {
                    name: entry.name.to_string(),
                });
            }
            ordered.push(match &entry.definition {
                WindowDefinition::Spec(window) => !window.order_by.is_empty(),
                WindowDefinition::Extended(window) => {
                    check_base(Some(&entry.name), window, earlier, &ordered)?
                }
            });
        }
        for select in &self.selects {
            match select.window() {
                Some(WindowSelectType::Name(name))
                    if !self
                        .windows
                        .iter()
                        .any(|entry| same_window(&entry.name, name)) =>
                {
                    return Err(Error::WindowUndefined {
                        name: name.to_string(),
                    });
                }
                Some(WindowSelectType::Extended(window)) => {
                    check_base(None, window, &self.windows, &ordered)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Refuse what the window `name` — an entry, or `None` for one written
/// inline after `OVER` — extending `window.base`, cannot copy from the
/// `earlier` entries, whose orderings `ordered` holds; answer whether the
/// extended window orders its rows.
fn check_base(
    name: Option<&Name>,
    window: &ExtendedWindow,
    earlier: &[NamedWindow],
    ordered: &[bool],
) -> Result<bool> {
    let refused = |error: fn(Option<String>, String) -> Error| {
        Err(error(
            name.map(|name| name.to_string()),
            window.base.to_string(),
        ))
    };
    let Some(at) = earlier
        .iter()
        .position(|entry| same_window(&entry.name, &window.base))
    else {
        return refused(|window, base| Error::WindowBaseUndefined { window, base });
    };
    let base_frame = match &earlier[at].definition {
        WindowDefinition::Spec(base) => base.frame.is_some(),
        WindowDefinition::Extended(base) => base.frame.is_some(),
    };
    if base_frame {
        return refused(|window, base| Error::WindowFrameCopied { window, base });
    }
    let orders = !window.order_by.is_empty();
    if orders && ordered[at] {
        return refused(|window, base| Error::WindowOrderOverridden { window, base });
    }
    Ok(orders || ordered[at])
}
