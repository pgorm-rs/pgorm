//! A window's frame: its mode, its bounds, and the `EXCLUDE` tail. Every
//! frame reaching here is one the grammar admits (`sql.ast.window-statement`),
//! so each part writes what it holds without a guard.

use super::*;
use crate::query::FrameBound;

impl QueryBuilder {
    /// Translate a [`FrameClause`] into ` <mode> <start>` or ` <mode> BETWEEN
    /// <start> AND <end>`, then ` EXCLUDE …` when an exclusion is set.
    // [spec:pgorm:req:sql.render.window+5]
    pub(super) fn prepare_frame_clause(&self, frame: &FrameClause, sql: &mut dyn SqlWriter) {
        match frame.r#type {
            FrameType::Range => write!(sql, " RANGE ").unwrap(),
            FrameType::Rows => write!(sql, " ROWS ").unwrap(),
            FrameType::Groups => write!(sql, " GROUPS ").unwrap(),
        };
        if let Some(end) = &frame.end {
            write!(sql, "BETWEEN ").unwrap();
            self.prepare_frame_bound(&frame.start, sql);
            write!(sql, " AND ").unwrap();
            self.prepare_frame_bound(end, sql);
        } else {
            self.prepare_frame_bound(&frame.start, sql);
        }
        if let Some(exclusion) = frame.exclusion {
            write!(sql, " EXCLUDE {}", exclusion.keyword()).unwrap();
        }
    }

    /// Translate one frame bound. An offset renders through the expression
    /// path — a value as a placeholder or an inline literal, anything else as
    /// itself — and needs no parentheses, because the keyword after it ends it.
    // [spec:pgorm:req:sql.render.window+5] (frame bounds)
    fn prepare_frame_bound(&self, bound: &FrameBound, sql: &mut dyn SqlWriter) {
        match bound {
            FrameBound::UnboundedPreceding => write!(sql, "UNBOUNDED PRECEDING").unwrap(),
            FrameBound::Preceding(offset) => {
                self.prepare_simple_expr(offset, sql);
                write!(sql, " PRECEDING").unwrap();
            }
            FrameBound::CurrentRow => write!(sql, "CURRENT ROW").unwrap(),
            FrameBound::Following(offset) => {
                self.prepare_simple_expr(offset, sql);
                write!(sql, " FOLLOWING").unwrap();
            }
            FrameBound::UnboundedFollowing => write!(sql, "UNBOUNDED FOLLOWING").unwrap(),
        }
    }
}
