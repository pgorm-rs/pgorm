//! A window frame: which rows around the current one a window function sees.
//!
//! PostgreSQL writes a frame as a mode, a start bound, an optional end bound
//! and an optional exclusion, and its grammar refuses — before any table is
//! read — every frame whose end would come before its start: a start of
//! `UNBOUNDED FOLLOWING`, an end of `UNBOUNDED PRECEDING`, `CURRENT ROW` then
//! a preceding end, an offset `FOLLOWING` then `CURRENT ROW` or a preceding
//! end, and an offset `FOLLOWING` standing alone (which reads as `BETWEEN …
//! AND CURRENT ROW`). The builder follows that shape so that none of those has
//! a construction (`[dec:pgorm:invalid-states-unrepresentable]`): a
//! [`FrameType`] names the start and yields a [`FrameStart`] whose type says
//! which side of the current row it is on, and each side offers only the ends
//! that may follow it. `EXCLUDE` is a method of the finished frame, so it
//! cannot be set on a window that has none.

use std::marker::PhantomData;

use crate::expr::SimpleExpr;

/// The unit a frame's offsets are counted in — PostgreSQL's three frame
/// modes — and where a frame is begun.
///
/// Each of the four methods names the frame's start bound and returns a
/// [`FrameStart`]. There is no `unbounded_following` start: PostgreSQL's
/// grammar refuses one.
///
/// ```compile_fail,E0599
/// use pgorm_query::*;
///
/// FrameType::Rows.unbounded_following();
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::select()
///     .from(Char::Table)
///     .expr_window(
///         Func::sum(Expr::col(Char::SizeW)),
///         WindowStatement::new()
///             .order_by(Char::FontSize, Order::Asc)
///             .frame(FrameType::Groups.preceding(1).and_current_row())
///             .take(),
///     )
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"SELECT SUM("size_w") OVER (  ORDER BY "font_size" ASC GROUPS BETWEEN 1 PRECEDING AND CURRENT ROW ) FROM "character""#
/// );
/// ```
// [spec:pgorm:def:sql.ast.window-statement+5]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// Offsets are values, compared against the ordering column: every peer of
    /// a row is inside the frame with it. An offset's type is the one
    /// PostgreSQL pairs with the ordering column's — an `interval` over a
    /// timestamp, a `numeric` over a numeric — and the window MUST order by
    /// exactly one column for an offset to be admitted at all.
    Range,
    /// Offsets are row counts, so peers are split wherever the count falls.
    Rows,
    /// Offsets are counts of *peer groups*: `GROUPS 1 PRECEDING` reaches back
    /// one whole group of ties rather than one row, which `Rows` cannot say
    /// and `Range` can only say for a value distance.
    Groups,
}

impl FrameType {
    /// Starts the frame at the partition's first row: `UNBOUNDED PRECEDING`.
    pub fn unbounded_preceding(self) -> FrameStart<FramePreceding> {
        FrameStart::new(self, FrameBound::UnboundedPreceding)
    }

    /// Starts the frame `offset` before the current row: `<offset>
    /// PRECEDING`. Under `Rows` and `Groups` the offset is a count, under
    /// `Range` a distance in the ordering column's values; either way it may
    /// be any expression that holds no column reference, which PostgreSQL
    /// refuses in a frame offset.
    pub fn preceding<T>(self, offset: T) -> FrameStart<FramePreceding>
    where
        T: Into<SimpleExpr>,
    {
        FrameStart::new(self, FrameBound::Preceding(Box::new(offset.into())))
    }

    /// Starts the frame at the current row — under `Range` and `Groups`, at
    /// the first of its peers.
    pub fn current_row(self) -> FrameStart<FrameCurrentRow> {
        FrameStart::new(self, FrameBound::CurrentRow)
    }

    /// Starts the frame `offset` after the current row: `<offset>
    /// FOLLOWING`. Such a start cannot stand alone — PostgreSQL reads a lone
    /// start as running to the current row, which lies behind it — so the
    /// [`FrameStart`] this returns must be given a following end.
    ///
    /// ```compile_fail,E0277
    /// use pgorm_query::*;
    ///
    /// WindowStatement::new().frame(FrameType::Rows.following(1));
    /// ```
    pub fn following<T>(self, offset: T) -> FrameStart<FrameFollowing>
    where
        T: Into<SimpleExpr>,
    {
        FrameStart::new(self, FrameBound::Following(Box::new(offset.into())))
    }
}

/// One end of a frame, as the renderer writes it. The offsets are boxed so a
/// window without one stays the size it was when they were counts.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FrameBound {
    UnboundedPreceding,
    Preceding(Box<SimpleExpr>),
    CurrentRow,
    Following(Box<SimpleExpr>),
    UnboundedFollowing,
}

/// The side of a [`FrameStart`] that begins before the current row:
/// `UNBOUNDED PRECEDING` or an offset `PRECEDING`. Every end may follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePreceding {}

/// The side of a [`FrameStart`] that begins at the current row. Any end but a
/// preceding one may follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameCurrentRow {}

/// The side of a [`FrameStart`] that begins after the current row. Only a
/// following end may follow it, and it cannot stand alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFollowing {}

mod sealed {
    /// The start sides that may stand alone as a whole frame, and that may
    /// end at the current row — the same two, because a lone start is read
    /// as running to the current row.
    pub trait EndsAtCurrentRow {}

    impl EndsAtCurrentRow for super::FramePreceding {}
    impl EndsAtCurrentRow for super::FrameCurrentRow {}
}

use sealed::EndsAtCurrentRow;

/// A frame's mode and start bound, typed by the side of the current row the
/// start is on, as returned by [`FrameType`]'s methods.
///
/// Its methods give the frame an end — `BETWEEN <start> AND <end>` — and
/// offer only the ends that may follow the start: after a preceding start any
/// end, after `CURRENT ROW` any but a preceding one, after a following start
/// only a following one. A preceding or current-row start also stands alone
/// as a whole frame, converting into a [`FrameClause`] wherever one is
/// accepted.
///
/// ```compile_fail,E0599
/// use pgorm_query::*;
///
/// // `ROWS BETWEEN CURRENT ROW AND 1 PRECEDING` ends before it starts.
/// FrameType::Rows.current_row().and_preceding(1);
/// ```
///
/// ```compile_fail,E0599
/// use pgorm_query::*;
///
/// // `ROWS BETWEEN 1 FOLLOWING AND CURRENT ROW` does too.
/// FrameType::Rows.following(1).and_current_row();
/// ```
///
/// ```compile_fail,E0599
/// use pgorm_query::*;
///
/// // No frame ends at `UNBOUNDED PRECEDING`.
/// FrameType::Rows.unbounded_preceding().and_unbounded_preceding();
/// ```
// [spec:pgorm:def:sql.ast.window-statement+5]
#[derive(Debug, Clone, PartialEq)]
pub struct FrameStart<S> {
    r#type: FrameType,
    start: FrameBound,
    side: PhantomData<S>,
}

impl<S> FrameStart<S> {
    fn new(r#type: FrameType, start: FrameBound) -> Self {
        Self {
            r#type,
            start,
            side: PhantomData,
        }
    }

    fn between(self, end: FrameBound) -> FrameClause {
        FrameClause {
            r#type: self.r#type,
            start: self.start,
            end: Some(end),
            exclusion: None,
        }
    }

    /// Ends the frame `offset` after the current row: `BETWEEN <start> AND
    /// <offset> FOLLOWING`.
    pub fn and_following<T>(self, offset: T) -> FrameClause
    where
        T: Into<SimpleExpr>,
    {
        self.between(FrameBound::Following(Box::new(offset.into())))
    }

    /// Ends the frame at the partition's last row: `BETWEEN <start> AND
    /// UNBOUNDED FOLLOWING`.
    pub fn and_unbounded_following(self) -> FrameClause {
        self.between(FrameBound::UnboundedFollowing)
    }
}

impl<S: EndsAtCurrentRow> FrameStart<S> {
    /// Ends the frame at the current row — under `Range` and `Groups`, at the
    /// last of its peers: `BETWEEN <start> AND CURRENT ROW`.
    pub fn and_current_row(self) -> FrameClause {
        self.between(FrameBound::CurrentRow)
    }

    /// This start as a whole frame with an `EXCLUDE` clause, as
    /// [`FrameClause::exclude`] adds one to a frame with an end.
    pub fn exclude(self, exclusion: FrameExclusion) -> FrameClause {
        FrameClause::from(self).exclude(exclusion)
    }
}

impl FrameStart<FramePreceding> {
    /// Ends the frame `offset` before the current row: `BETWEEN <start> AND
    /// <offset> PRECEDING`. Only a preceding start admits a preceding end.
    pub fn and_preceding<T>(self, offset: T) -> FrameClause
    where
        T: Into<SimpleExpr>,
    {
        self.between(FrameBound::Preceding(Box::new(offset.into())))
    }
}

impl<S: EndsAtCurrentRow> From<FrameStart<S>> for FrameClause {
    /// The start alone, `<mode> <start>`, which PostgreSQL reads as running
    /// to the current row.
    fn from(start: FrameStart<S>) -> Self {
        FrameClause {
            r#type: start.r#type,
            start: start.start,
            end: None,
            exclusion: None,
        }
    }
}

/// A whole window frame: a mode, a start, an optional end and an optional
/// `EXCLUDE` clause, set on a window by
/// [`WindowStatement::frame`](crate::WindowStatement::frame).
///
/// Built from a [`FrameStart`], either by giving it an end or by letting a
/// preceding or current-row start stand alone, so every frame this type holds
/// is one PostgreSQL's grammar admits.
// [spec:pgorm:def:sql.ast.window-statement+5]
#[derive(Debug, Clone, PartialEq)]
pub struct FrameClause {
    pub(crate) r#type: FrameType,
    pub(crate) start: FrameBound,
    pub(crate) end: Option<FrameBound>,
    pub(crate) exclusion: Option<FrameExclusion>,
}

impl FrameClause {
    /// Removes rows around the current one from its frame: `EXCLUDE CURRENT
    /// ROW`, `GROUP`, `TIES` or `NO OTHERS`. A second call replaces the first.
    ///
    /// The clause exists only on a frame, so a window without one has no way
    /// to take it:
    ///
    /// ```compile_fail,E0599
    /// use pgorm_query::*;
    ///
    /// WindowStatement::new().exclude(FrameExclusion::Ties);
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .from(Char::Table)
    ///     .expr_window(
    ///         Func::sum(Expr::col(Char::SizeW)),
    ///         WindowStatement::new()
    ///             .order_by(Char::FontSize, Order::Asc)
    ///             .frame(
    ///                 FrameType::Rows
    ///                     .unbounded_preceding()
    ///                     .and_unbounded_following()
    ///                     .exclude(FrameExclusion::Ties),
    ///             )
    ///             .take(),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT SUM("size_w") OVER (  ORDER BY "font_size" ASC ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING EXCLUDE TIES ) FROM "character""#
    /// );
    /// ```
    pub fn exclude(mut self, exclusion: FrameExclusion) -> Self {
        self.exclusion = Some(exclusion);
        self
    }
}

/// The rows an `EXCLUDE` clause removes from a frame, each relative to the
/// current row and its peers — the rows its window's `ORDER BY` ties with it.
// [spec:pgorm:def:sql.ast.window-statement+5]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameExclusion {
    /// `EXCLUDE CURRENT ROW` — the current row, but not its peers.
    CurrentRow,
    /// `EXCLUDE GROUP` — the current row and every peer.
    Group,
    /// `EXCLUDE TIES` — every peer, but not the current row.
    Ties,
    /// `EXCLUDE NO OTHERS` — nothing, which is also what a frame without the
    /// clause excludes; it is the explicit spelling of the default.
    NoOthers,
}

impl FrameExclusion {
    /// The words after `EXCLUDE`.
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            Self::CurrentRow => "CURRENT ROW",
            Self::Group => "GROUP",
            Self::Ties => "TIES",
            Self::NoOthers => "NO OTHERS",
        }
    }
}
