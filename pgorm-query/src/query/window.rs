use crate::{expr::*, query::*, types::*};
use inherent::inherent;

pub trait OverStatement {
    #[doc(hidden)]
    // Implementation for the trait.
    fn add_partition_by(&mut self, partition: SimpleExpr) -> &mut Self;

    /// Partition by column.
    fn partition_by<T>(&mut self, col: T) -> &mut Self
    where
        T: IntoColumnRef,
    {
        self.add_partition_by(SimpleExpr::Column(col.into_column_ref()))
    }

    /// Partition by vector of columns.
    fn partition_by_columns<I, T>(&mut self, cols: I) -> &mut Self
    where
        T: IntoColumnRef,
        I: IntoIterator<Item = T>,
    {
        cols.into_iter().for_each(|c| {
            self.add_partition_by(SimpleExpr::Column(c.into_column_ref()));
        });
        self
    }
}

/// frame_start or frame_end clause
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    UnboundedPreceding,
    Preceding(u32),
    CurrentRow,
    Following(u32),
    UnboundedFollowing,
}

/// The unit a frame's offsets are counted in — PostgreSQL's three frame modes.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameType {
    /// Offsets are values, compared against the ordering column: every peer of
    /// a row is inside the frame with it.
    Range,
    /// Offsets are row counts, so peers are split wherever the count falls.
    Rows,
    /// Offsets are counts of *peer groups*: `GROUPS 1 PRECEDING` reaches back
    /// one whole group of ties rather than one row, which `Rows` cannot say
    /// and `Range` can only say for a value distance.
    Groups,
}

/// Frame clause
#[derive(Debug, Clone, PartialEq)]
pub struct FrameClause {
    pub(crate) r#type: FrameType,
    pub(crate) start: Frame,
    pub(crate) end: Option<Frame>,
}

/// Window expression
///
/// # Reference
///
/// <https://www.postgresql.org/docs/current/tutorial-window.html>
// [spec:pgorm:def:sql.ast.window-statement+4]
#[derive(Default, Debug, Clone, PartialEq)]
pub struct WindowStatement {
    pub(crate) partition_by: Vec<SimpleExpr>,
    pub(crate) order_by: Vec<OrderExpr>,
    pub(crate) frame: Option<FrameClause>,
}

impl WindowStatement {
    /// Construct a new [`WindowStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take(&mut self) -> Self {
        Self {
            partition_by: std::mem::take(&mut self.partition_by),
            order_by: std::mem::take(&mut self.order_by),
            frame: self.frame.take(),
        }
    }

    /// Construct a new [`WindowStatement`] with PARTITION BY column
    pub fn partition_by<T>(col: T) -> Self
    where
        T: IntoColumnRef,
    {
        let mut window = Self::new();
        window.add_partition_by(SimpleExpr::Column(col.into_column_ref()));
        window
    }

    /// frame clause for frame_start
    /// # Examples:
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .from(Char::Table)
    ///     .expr_window_as(
    ///         Func::count(Expr::col(Char::Id)),
    ///         WindowStatement::partition_by(Char::FontSize)
    ///             .frame_start(FrameType::Rows, Frame::UnboundedPreceding)
    ///             .take(),
    ///         Name::runtime("C"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT COUNT("id") OVER ( PARTITION BY "font_size" ROWS UNBOUNDED PRECEDING ) AS "C" FROM "character""#
    /// );
    /// ```
    pub fn frame_start(&mut self, r#type: FrameType, start: Frame) -> &mut Self {
        self.frame(r#type, start, None)
    }

    /// frame clause for BETWEEN frame_start AND frame_end
    ///
    /// # Examples:
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .from(Char::Table)
    ///     .expr_window_as(
    ///         Func::count(Expr::col(Char::Id)),
    ///         WindowStatement::partition_by(Char::FontSize)
    ///             .frame_between(FrameType::Rows, Frame::UnboundedPreceding, Frame::UnboundedFollowing)
    ///             .take(),
    ///         Name::runtime("C"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT COUNT("id") OVER ( PARTITION BY "font_size" ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING ) AS "C" FROM "character""#
    /// );
    /// ```
    pub fn frame_between(&mut self, r#type: FrameType, start: Frame, end: Frame) -> &mut Self {
        self.frame(r#type, start, Some(end))
    }

    /// frame clause
    pub fn frame(&mut self, r#type: FrameType, start: Frame, end: Option<Frame>) -> &mut Self {
        let frame_clause = FrameClause { r#type, start, end };
        self.frame = Some(frame_clause);
        self
    }
}

impl OverStatement for WindowStatement {
    fn add_partition_by(&mut self, partition: SimpleExpr) -> &mut Self {
        self.partition_by.push(partition);
        self
    }
}

#[inherent]
impl OrderedStatement for WindowStatement {
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
