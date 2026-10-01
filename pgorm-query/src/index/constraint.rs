use crate::{Deferrability, IntoName, Name};

/// A `UNIQUE` or `PRIMARY KEY` constraint written inside `CREATE TABLE`, for
/// [`TableCreateStatement::index`](crate::TableCreateStatement::index) to
/// embed.
///
/// A table constraint is not an index statement moved inside a table, and it
/// is not built from one. Its grammar takes a list of plain column names,
/// `INCLUDE`, `NULLS NOT DISTINCT` on a unique key, and deferrability; it has
/// no place for the non-unique kind, an expression entry, an ordering, an
/// operator class, a predicate or an access method, all of which
/// [`IndexCreateStatement`](crate::IndexCreateStatement) carries for the
/// standalone `CREATE INDEX`. So the two are separate builders, and none of
/// those can reach the embedded position:
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// // An index statement does not embed.
/// Table::create(Glyph::Table).index(Index::create(Glyph::Table, Glyph::Aspect).unique().to_owned());
/// ```
///
/// ```compile_fail,E0599
/// use pgorm_query::{*, tests_cfg::*};
///
/// // A constraint has no predicate.
/// IndexConstraint::unique(Glyph::Aspect).and_where(Expr::col(Glyph::Image).is_not_null());
/// ```
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// // Nor an expression or an ordered entry: a key column is a name.
/// IndexConstraint::unique((Glyph::Aspect, IndexOrder::Desc));
/// ```
///
/// Deferrability is the one clause that runs the other way: PostgreSQL
/// defers constraints, never indexes, so it is spelled here and nowhere on
/// `IndexCreateStatement`. The constraint has no rendering of its own — no
/// `Display` and no build path — because PostgreSQL spells a unique or
/// primary-key constraint only inside `CREATE TABLE`:
///
/// ```compile_fail,E0599
/// use pgorm_query::{*, tests_cfg::*};
///
/// IndexConstraint::unique(Glyph::Aspect)
///     .deferrability(Deferrability::DeferrableInitiallyDeferred)
///     .to_string();
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::create(Glyph::Table)
///     .col(ColumnDef::new(Glyph::Id).integer().not_null())
///     .col(ColumnDef::new(Glyph::Aspect).integer().not_null())
///     .col(ColumnDef::new(Glyph::Image).text())
///     .index(
///         IndexConstraint::primary_key(Glyph::Id)
///             .deferrability(Deferrability::DeferrableInitiallyDeferred),
///     )
///     .index(
///         IndexConstraint::unique_nulls_not_distinct(Glyph::Aspect)
///             .col(Glyph::Image)
///             .name(Name::runtime("glyph_aspect"))
///             .include([Glyph::Id]),
///     )
///     .to_owned();
///
/// assert_eq!(
///     table.to_string(),
///     [
///         r#"CREATE TABLE "glyph" ("#,
///         r#""id" integer NOT NULL,"#,
///         r#""aspect" integer NOT NULL,"#,
///         r#""image" text,"#,
///         r#"PRIMARY KEY ("id") DEFERRABLE INITIALLY DEFERRED,"#,
///         r#"CONSTRAINT "glyph_aspect" UNIQUE NULLS NOT DISTINCT ("aspect", "image") INCLUDE ("id")"#,
///         r#")"#,
///     ]
///     .join(" ")
/// );
/// ```
// [spec:pgorm:req:sql.ddl.create-table+10]
#[derive(Debug, Clone)]
pub struct IndexConstraint {
    pub(crate) name: Option<Name>,
    pub(crate) key: ConstraintKey,
    pub(crate) columns: Vec<Name>,
    pub(crate) include: Vec<Name>,
    pub(crate) deferrability: Option<Deferrability>,
}

/// Which key a table constraint declares. `NULLS NOT DISTINCT` is a kind of
/// unique key rather than a flag beside the kind, because the grammar has no
/// place for it on a primary key.
// [spec:pgorm:req:sql.ddl.create-table+10]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstraintKey {
    Unique,
    UniqueNullsNotDistinct,
    Primary,
}

impl IndexConstraint {
    fn new(key: ConstraintKey, column: Name) -> Self {
        Self {
            name: None,
            key,
            columns: vec![column],
            include: Vec::new(),
            deferrability: None,
        }
    }

    /// `UNIQUE ("column")`, over the first of its columns; [`col`](Self::col)
    /// adds the rest.
    pub fn unique<C>(column: C) -> Self
    where
        C: IntoName,
    {
        Self::new(ConstraintKey::Unique, column.into_name())
    }

    /// `UNIQUE NULLS NOT DISTINCT ("column")`: a unique key under which nulls
    /// are equal, so at most one row may hold a null where a plain unique key
    /// admits any number.
    ///
    /// It is a constructor rather than a setter because the grammar has no
    /// place for the clause on a primary key, so there is nothing for it to
    /// be set on.
    pub fn unique_nulls_not_distinct<C>(column: C) -> Self
    where
        C: IntoName,
    {
        Self::new(ConstraintKey::UniqueNullsNotDistinct, column.into_name())
    }

    /// `PRIMARY KEY ("column")`, over the first of its columns;
    /// [`col`](Self::col) adds the rest.
    pub fn primary_key<C>(column: C) -> Self
    where
        C: IntoName,
    {
        Self::new(ConstraintKey::Primary, column.into_name())
    }

    /// Add a further key column.
    #[must_use]
    pub fn col<C>(mut self, column: C) -> Self
    where
        C: IntoName,
    {
        self.columns.push(column.into_name());
        self
    }

    /// Name the constraint: `CONSTRAINT "name" ...`. Unnamed, PostgreSQL
    /// derives one.
    #[must_use]
    pub fn name<N>(mut self, name: N) -> Self
    where
        N: IntoName,
    {
        self.name = Some(name.into_name());
        self
    }

    /// Carry further columns in the constraint's index without making them
    /// part of the key — `INCLUDE (…)`. Repeated calls append.
    #[must_use]
    pub fn include<N, I>(mut self, columns: I) -> Self
    where
        N: IntoName,
        I: IntoIterator<Item = N>,
    {
        self.include
            .extend(columns.into_iter().map(IntoName::into_name));
        self
    }

    /// Run the constraint's check when `deferrability` says
    /// (`[spec:pgorm:req:sql.ddl.deferrability+3]`), replacing any already set.
    // [spec:pgorm:req:sql.ddl.deferrability+3]
    #[must_use]
    pub fn deferrability(mut self, deferrability: Deferrability) -> Self {
        self.deferrability = Some(deferrability);
        self
    }

    /// The constraint's name, if it was given one.
    pub fn get_name(&self) -> Option<&Name> {
        self.name.as_ref()
    }

    /// Whether this is the table's primary key.
    pub fn is_primary_key(&self) -> bool {
        self.key == ConstraintKey::Primary
    }

    /// Whether this is a unique key, nulls distinct or not.
    pub fn is_unique_key(&self) -> bool {
        matches!(
            self.key,
            ConstraintKey::Unique | ConstraintKey::UniqueNullsNotDistinct
        )
    }

    /// Whether this is a unique key under which nulls are equal.
    pub fn is_nulls_not_distinct(&self) -> bool {
        self.key == ConstraintKey::UniqueNullsNotDistinct
    }

    /// The key columns, in order. Never empty.
    pub fn get_columns(&self) -> &[Name] {
        &self.columns
    }

    /// The `INCLUDE` columns, in order.
    pub fn get_include(&self) -> &[Name] {
        &self.include
    }

    /// When this constraint's check runs, if the caller said.
    pub fn get_deferrability(&self) -> Option<Deferrability> {
        self.deferrability
    }
}
