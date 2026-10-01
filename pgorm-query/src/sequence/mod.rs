//! `CREATE`, `ALTER`, `DROP` and rename of a `SEQUENCE`.

use crate::{IntoName, IntoTableName, Name, QueryBuilder, TableDropOpt, TableName};

mod alter;
mod options;

pub use alter::{PendingSequenceAlter, SequenceAlterStatement};
pub use options::{SequenceOption, SequenceOptions, SequenceType};

use options::merge_into;

/// Helper for constructing any sequence statement
///
/// A sequence is a relation, so it is named as a table is: a bare name or a
/// `(schema, name)` pair, each part a quoted identifier.
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone, Copy)]
pub struct Sequence;

impl Sequence {
    /// Construct a [`SequenceCreateStatement`] over the sequence it creates
    pub fn create<T>(name: T) -> SequenceCreateStatement
    where
        T: IntoTableName,
    {
        SequenceCreateStatement {
            name: name.into_table_name(),
            if_not_exists: false,
            clauses: SequenceClauses::default(),
        }
    }

    /// Name the sequence a [`SequenceAlterStatement`] will alter.
    ///
    /// Choosing a clause on the returned [`PendingSequenceAlter`] is what
    /// produces the statement: `ALTER SEQUENCE "s"` with nothing after it is a
    /// syntax error, so naming the sequence alone renders nothing.
    pub fn alter<T>(name: T) -> PendingSequenceAlter
    where
        T: IntoTableName,
    {
        PendingSequenceAlter::new(name.into_table_name())
    }

    /// Construct a [`SequenceDropStatement`] over the first sequence it drops
    pub fn drop<T>(name: T) -> SequenceDropStatement
    where
        T: IntoTableName,
    {
        SequenceDropStatement {
            first: name.into_table_name(),
            rest: Vec::new(),
            if_exists: false,
            behavior: None,
        }
    }

    /// Construct a [`SequenceRenameStatement`] from the old and new name
    pub fn rename<T, R>(from_name: T, to_name: R) -> SequenceRenameStatement
    where
        T: IntoTableName,
        R: IntoName,
    {
        SequenceRenameStatement {
            from_name: from_name.into_table_name(),
            to_name: to_name.into_name(),
        }
    }
}

/// What `OWNED BY` ties a sequence to.
#[derive(Debug, Clone)]
pub(crate) enum SequenceOwner {
    /// `OWNED BY "table"."column"`, the table schema-qualified or not.
    Column(TableName, Name),
    /// `OWNED BY NONE`.
    Nothing,
}

/// The clauses `CREATE SEQUENCE` and the option form of `ALTER SEQUENCE`
/// share, in the order they render. `restart` is `ALTER`'s alone.
#[derive(Debug, Clone, Default)]
pub(crate) struct SequenceClauses {
    pub(crate) as_type: Option<SequenceType>,
    pub(crate) options: Option<SequenceOptions>,
    /// `RESTART`, then `WITH n` when it names a value.
    pub(crate) restart: Option<Option<i64>>,
    pub(crate) owned_by: Option<SequenceOwner>,
}

impl SequenceClauses {
    fn add_options(&mut self, options: impl Into<SequenceOptions>) {
        merge_into(&mut self.options, options.into());
    }

    fn own<T, C>(&mut self, table: T, column: C)
    where
        T: IntoTableName,
        C: IntoName,
    {
        self.owned_by = Some(SequenceOwner::Column(
            table.into_table_name(),
            column.into_name(),
        ));
    }
}

/// Create a sequence
///
/// The name is taken by [`Sequence::create`], because `CREATE SEQUENCE ` is
/// rejected at end of input. Everything else is optional, and a sequence with
/// none of it counts in `bigint` from 1 upwards by 1.
///
/// ```compile_fail,E0061
/// use pgorm_query::*;
///
/// Sequence::create().if_not_exists();
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::*;
///
/// let seq = Sequence::create((Name::runtime("billing"), Name::runtime("invoice_no")))
///     .if_not_exists()
///     .as_type(SequenceType::Integer)
///     .options(SequenceOption::StartWith(1000).and(SequenceOption::Cache(20)))
///     .owned_by(
///         (Name::runtime("billing"), Name::runtime("invoice")),
///         Name::runtime("no"),
///     )
///     .to_owned();
///
/// assert_eq!(
///     seq.to_string(),
///     [
///         r#"CREATE SEQUENCE IF NOT EXISTS "billing"."invoice_no" AS integer"#,
///         r#"START WITH 1000 CACHE 20 OWNED BY "billing"."invoice"."no""#,
///     ]
///     .join(" ")
/// );
/// ```
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone)]
pub struct SequenceCreateStatement {
    pub(crate) name: TableName,
    pub(crate) if_not_exists: bool,
    pub(crate) clauses: SequenceClauses,
}

impl SequenceCreateStatement {
    /// `IF NOT EXISTS`: leave an existing relation of this name alone, and
    /// its definition with it.
    pub fn if_not_exists(&mut self) -> &mut Self {
        self.if_not_exists = true;
        self
    }

    /// `AS <type>`: the integer type the sequence counts in, which also sets
    /// its default bounds.
    pub fn as_type(&mut self, ty: SequenceType) -> &mut Self {
        self.clauses.as_type = Some(ty);
        self
    }

    /// Add options to the definition. A later call adds to an earlier one,
    /// each option replacing one already given for its clause.
    pub fn options<O>(&mut self, options: O) -> &mut Self
    where
        O: Into<SequenceOptions>,
    {
        self.clauses.add_options(options);
        self
    }

    /// `OWNED BY "table"."column"`: drop the sequence with the column.
    ///
    /// The owner is a column of a table, so both are taken: PostgreSQL refuses
    /// `OWNED BY "column"` alone (`42601`, *invalid OWNED BY option*), and the
    /// table in the same schema as the sequence (`55000` otherwise).
    pub fn owned_by<T, C>(&mut self, table: T, column: C) -> &mut Self
    where
        T: IntoTableName,
        C: IntoName,
    {
        self.clauses.own(table, column);
        self
    }

    /// `OWNED BY NONE`: the sequence belongs to no column, which is also the
    /// default.
    pub fn owned_by_none(&mut self) -> &mut Self {
        self.clauses.owned_by = Some(SequenceOwner::Nothing);
        self
    }
}

/// Drop one or more sequences
///
/// The first name is taken by [`Sequence::drop`] and every further one is
/// appended, so the list is non-empty: PostgreSQL rejects `DROP SEQUENCE ` at
/// end of input.
///
/// ```
/// use pgorm_query::*;
///
/// assert_eq!(
///     Sequence::drop(Name::runtime("a"))
///         .name(Name::runtime("b"))
///         .if_exists()
///         .cascade()
///         .to_string(),
///     r#"DROP SEQUENCE IF EXISTS "a", "b" CASCADE"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone)]
pub struct SequenceDropStatement {
    pub(crate) first: TableName,
    pub(crate) rest: Vec<TableName>,
    pub(crate) if_exists: bool,
    pub(crate) behavior: Option<TableDropOpt>,
}

impl SequenceDropStatement {
    /// Drop a further sequence
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoTableName,
    {
        self.rest.push(name.into_table_name());
        self
    }

    /// `IF EXISTS`
    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }

    /// `CASCADE`: drop what depends on the sequence too — a column default
    /// that calls `nextval` on it. The last of this and
    /// [`restrict`](Self::restrict) wins.
    pub fn cascade(&mut self) -> &mut Self {
        self.behavior = Some(TableDropOpt::Cascade);
        self
    }

    /// `RESTRICT`: refuse to drop a sequence something depends on, the
    /// default.
    pub fn restrict(&mut self) -> &mut Self {
        self.behavior = Some(TableDropOpt::Restrict);
        self
    }

    /// The sequences dropped, in declaration order, of which there is at least
    /// one
    pub fn names(&self) -> impl Iterator<Item = &TableName> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }
}

/// Rename a sequence: `ALTER SEQUENCE "from" RENAME TO "to"`.
///
/// Both names are taken by [`Sequence::rename`]. The new name is a bare
/// identifier, because `RENAME TO` leaves a sequence in the schema it is
/// already in.
///
/// ```
/// use pgorm_query::*;
///
/// assert_eq!(
///     Sequence::rename(
///         (Name::runtime("billing"), Name::runtime("invoice_no")),
///         Name::runtime("invoice_seq"),
///     )
///     .to_string(),
///     r#"ALTER SEQUENCE "billing"."invoice_no" RENAME TO "invoice_seq""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.sequence]
#[derive(Debug, Clone)]
pub struct SequenceRenameStatement {
    pub(crate) from_name: TableName,
    pub(crate) to_name: Name,
}

macro_rules! impl_sequence_display {
    ( $( $struct_name:ident => $func_name:ident ),* $(,)? ) => {
        $(
            /// Renders the statement with every value inlined as an SQL
            /// literal. This is its only rendering: a sequence statement
            /// carries integers and names and nothing to bind.
            // [spec:pgorm:req:sql.ddl+8]
            impl std::fmt::Display for $struct_name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let mut sql = String::with_capacity(128);
                    QueryBuilder.$func_name(self, &mut sql);
                    f.write_str(&sql)
                }
            }
        )*
    };
}

impl_sequence_display!(
    SequenceCreateStatement => prepare_sequence_create_statement,
    SequenceAlterStatement => prepare_sequence_alter_statement,
    SequenceDropStatement => prepare_sequence_drop_statement,
    SequenceRenameStatement => prepare_sequence_rename_statement,
);
