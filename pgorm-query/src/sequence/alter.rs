//! `ALTER SEQUENCE`: a sequence awaiting its first clause, and the statement
//! choosing one makes.

use crate::{IntoName, IntoTableName, TableName};

use super::{SequenceClauses, SequenceOptions, SequenceOwner, SequenceType};

/// A sequence awaiting its first clause.
///
/// PostgreSQL has no spelling for an `ALTER SEQUENCE` that changes nothing, so
/// this is what [`Sequence::alter`](super::Sequence::alter) returns: naming the
/// sequence is not yet a statement. Each clause method consumes it and yields a
/// [`SequenceAlterStatement`], which carries the name and at least one clause
/// for the rest of its life.
///
/// ```compile_fail,E0599
/// use pgorm_query::*;
///
/// Sequence::alter(Name::runtime("s")).to_string();
/// ```
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone)]
pub struct PendingSequenceAlter {
    name: TableName,
}

impl PendingSequenceAlter {
    pub(super) fn new(name: TableName) -> Self {
        Self { name }
    }

    fn with(self, first: impl FnOnce(&mut SequenceClauses)) -> SequenceAlterStatement {
        let mut clauses = SequenceClauses::default();
        first(&mut clauses);
        SequenceAlterStatement {
            name: self.name,
            if_exists: false,
            clauses,
        }
    }

    /// `AS <type>`: count in another integer type. A bound sitting at the old
    /// type's limit moves to the new type's.
    pub fn as_type(self, ty: SequenceType) -> SequenceAlterStatement {
        self.with(|c| c.as_type = Some(ty))
    }

    /// Change the definition's options.
    pub fn options<O>(self, options: O) -> SequenceAlterStatement
    where
        O: Into<SequenceOptions>,
    {
        self.with(|c| c.add_options(options))
    }

    /// `RESTART`: hand out the start value next.
    pub fn restart(self) -> SequenceAlterStatement {
        self.with(|c| c.restart = Some(None))
    }

    /// `RESTART WITH n`: hand out `n` next.
    pub fn restart_with(self, value: i64) -> SequenceAlterStatement {
        self.with(|c| c.restart = Some(Some(value)))
    }

    /// `OWNED BY "table"."column"`
    pub fn owned_by<T, C>(self, table: T, column: C) -> SequenceAlterStatement
    where
        T: IntoTableName,
        C: IntoName,
    {
        self.with(|c| c.own(table, column))
    }

    /// `OWNED BY NONE`: release the sequence from its column.
    pub fn owned_by_none(self) -> SequenceAlterStatement {
        self.with(|c| c.owned_by = Some(SequenceOwner::Nothing))
    }
}

/// Alter a sequence's definition
///
/// A statement of this type always names a sequence and always carries at
/// least one clause: it is reachable only by choosing one on a
/// [`PendingSequenceAlter`]. Further clauses chain on it, and each renders in a
/// fixed order — `AS`, the options, `RESTART`, `OWNED BY`.
///
/// An identity column's sequence is altered here too, under the name
/// PostgreSQL gave it (`pg_get_serial_sequence` reports it), for every clause
/// but `OWNED BY`: the server refuses to move an identity sequence's ownership
/// (`0A000`).
///
/// ```
/// use pgorm_query::*;
///
/// assert_eq!(
///     Sequence::alter(Name::runtime("ticket"))
///         .restart_with(1)
///         .options(SequenceOption::IncrementBy(5))
///         .if_exists()
///         .to_string(),
///     r#"ALTER SEQUENCE IF EXISTS "ticket" INCREMENT BY 5 RESTART WITH 1"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone)]
pub struct SequenceAlterStatement {
    pub(crate) name: TableName,
    pub(crate) if_exists: bool,
    pub(crate) clauses: SequenceClauses,
}

impl SequenceAlterStatement {
    /// `IF EXISTS`: a missing sequence is a notice rather than an error.
    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }

    /// `AS <type>`
    pub fn as_type(&mut self, ty: SequenceType) -> &mut Self {
        self.clauses.as_type = Some(ty);
        self
    }

    /// Change further options, each replacing one already given for its
    /// clause.
    pub fn options<O>(&mut self, options: O) -> &mut Self
    where
        O: Into<SequenceOptions>,
    {
        self.clauses.add_options(options);
        self
    }

    /// `RESTART`, replacing a `RESTART WITH` already given.
    pub fn restart(&mut self) -> &mut Self {
        self.clauses.restart = Some(None);
        self
    }

    /// `RESTART WITH n`, replacing a `RESTART` already given.
    pub fn restart_with(&mut self, value: i64) -> &mut Self {
        self.clauses.restart = Some(Some(value));
        self
    }

    /// `OWNED BY "table"."column"`
    pub fn owned_by<T, C>(&mut self, table: T, column: C) -> &mut Self
    where
        T: IntoTableName,
        C: IntoName,
    {
        self.clauses.own(table, column);
        self
    }

    /// `OWNED BY NONE`
    pub fn owned_by_none(&mut self) -> &mut Self {
        self.clauses.owned_by = Some(SequenceOwner::Nothing);
        self
    }
}
