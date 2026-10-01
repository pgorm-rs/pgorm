//! The option vocabulary a sequence is defined by, shared by `CREATE SEQUENCE`,
//! `ALTER SEQUENCE` and an identity column's `( ... )` tail.

/// One clause of a sequence's definition: its step, a bound, where it starts,
/// how many values it hands out at a time, or whether it wraps.
///
/// Each is written as PostgreSQL spells it, the number inline as an integer
/// literal. Whether a number makes sense — a zero step, a cache below one,
/// bounds that cross, a start outside them, a bound outside the counting type
/// — is checked by the server against the whole definition, which refuses it
/// with `22023`; the type cannot see the other clauses a value is checked
/// against, so it does not pretend to.
///
/// ```
/// use pgorm_query::*;
///
/// assert_eq!(
///     Sequence::create(Name::runtime("ticket"))
///         .options(SequenceOption::StartWith(100).and(SequenceOption::IncrementBy(10)))
///         .to_string(),
///     r#"CREATE SEQUENCE "ticket" INCREMENT BY 10 START WITH 100"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceOption {
    /// `INCREMENT BY n`: the step between two values, negative for a
    /// descending sequence.
    IncrementBy(i64),
    /// `MINVALUE n`.
    MinValue(i64),
    /// `NO MINVALUE`: the default lower bound — 1 ascending, the counting
    /// type's minimum descending.
    NoMinValue,
    /// `MAXVALUE n`.
    MaxValue(i64),
    /// `NO MAXVALUE`: the default upper bound — the counting type's maximum
    /// ascending, -1 descending.
    NoMaxValue,
    /// `START WITH n`: the first value handed out.
    StartWith(i64),
    /// `CACHE n`: how many values a session preallocates.
    Cache(i64),
    /// `CYCLE`: wrap to the opposite bound instead of failing at the end.
    Cycle,
    /// `NO CYCLE`: fail at the end (`2200H`), the default.
    NoCycle,
}

/// How many clauses a definition has room for: one per option, a bound and
/// its `NO` form sharing one, as `CYCLE` and `NO CYCLE` do.
const CLAUSES: usize = 6;

impl SequenceOption {
    /// This option and `next`, as a set of options.
    #[must_use]
    pub fn and(self, next: SequenceOption) -> SequenceOptions {
        SequenceOptions::from(self).and(next)
    }

    /// The clause this option fills. `MINVALUE n` and `NO MINVALUE` fill the
    /// same one, because PostgreSQL refuses both together as conflicting.
    const fn clause(self) -> usize {
        match self {
            Self::IncrementBy(_) => 0,
            Self::MinValue(_) | Self::NoMinValue => 1,
            Self::MaxValue(_) | Self::NoMaxValue => 2,
            Self::StartWith(_) => 3,
            Self::Cache(_) => 4,
            Self::Cycle | Self::NoCycle => 5,
        }
    }
}

/// One or more [`SequenceOption`]s, at most one per clause.
///
/// There is no empty set: one is built from its first option and grows by
/// [`and`](Self::and), so a set always writes at least one clause. That is
/// what lets an `ALTER SEQUENCE` take one as its whole action, and an identity
/// column write its parentheses only when it has a set — `AS IDENTITY ()` is a
/// syntax error. A second option for a clause already filled replaces the
/// first, because PostgreSQL refuses a clause given twice (`42601`,
/// *conflicting or redundant options*): `MinValue(1).and(NoMinValue)` is
/// `NO MINVALUE`.
///
/// The clauses render in one fixed order whatever order they were given in;
/// the server reads them as a set.
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceOptions {
    clauses: [Option<SequenceOption>; CLAUSES],
}

impl From<SequenceOption> for SequenceOptions {
    fn from(first: SequenceOption) -> Self {
        let mut clauses = [None; CLAUSES];
        clauses[first.clause()] = Some(first);
        Self { clauses }
    }
}

impl SequenceOptions {
    /// Add `next`, replacing an option already given for its clause.
    #[must_use]
    pub fn and(mut self, next: SequenceOption) -> Self {
        self.clauses[next.clause()] = Some(next);
        self
    }

    /// Every option in `other` added to this set, each replacing an option
    /// already given for its clause.
    pub(crate) fn merge(&mut self, other: SequenceOptions) {
        for option in other.iter() {
            self.clauses[option.clause()] = Some(option);
        }
    }

    /// The options, in the order they render.
    pub fn iter(&self) -> impl Iterator<Item = SequenceOption> + '_ {
        self.clauses.iter().flatten().copied()
    }
}

/// Fold `options` into `slot`, so a second `options(..)` call adds to the
/// first rather than discarding it.
pub(crate) fn merge_into(slot: &mut Option<SequenceOptions>, options: SequenceOptions) {
    match slot {
        Some(existing) => existing.merge(options),
        None => *slot = Some(options),
    }
}

/// The integer type a sequence counts in, `AS smallint | integer | bigint`.
///
/// Those three are the whole of what PostgreSQL accepts — any other type is
/// refused (`22023`, *sequence type must be smallint, integer, or bigint*) — so
/// the choice is a closed set rather than a column type.
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceType {
    /// `smallint`
    SmallInteger,
    /// `integer`
    Integer,
    /// `bigint`, the default.
    BigInteger,
}

impl SequenceType {
    /// The type's SQL spelling.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::SmallInteger => "smallint",
            Self::Integer => "integer",
            Self::BigInteger => "bigint",
        }
    }
}
