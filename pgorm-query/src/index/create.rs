use crate::{QueryBuilder, types::*};

use super::common::*;

/// Create an index for an existing table
///
/// An index indexes at least one column of exactly one table, in both the
/// standalone and the `CREATE TABLE`-embedded position, so both are taken by the
/// constructor and `col` appends the remaining columns: neither the empty column
/// list nor the missing `ON` target PostgreSQL rejects has anywhere to come from.
/// The index name stays optional, because PostgreSQL derives one when it is
/// absent.
///
/// ```compile_fail,E0061
/// use pgorm_query::{*, tests_cfg::*};
///
/// Index::create(Glyph::Aspect).name("idx-glyph-aspect");
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create(Glyph::Table, Glyph::Aspect)
///     .name("idx-glyph-aspect")
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect")"#
/// );
/// ```
/// Create index if not exists
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create(Glyph::Table, Glyph::Aspect)
///     .if_not_exists()
///     .name("idx-glyph-aspect")
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(),
///     r#"CREATE INDEX IF NOT EXISTS "idx-glyph-aspect" ON "glyph" ("aspect")"#
/// );
/// ```
/// Index with prefix
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create(Glyph::Table, (Glyph::Aspect, 128))
///     .name("idx-glyph-aspect")
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect" (128))"#
/// );
/// ```
/// Index with order
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create(Glyph::Table, (Glyph::Aspect, IndexOrder::Desc))
///     .name("idx-glyph-aspect")
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect" DESC)"#
/// );
/// ```
/// Index on multi-columns
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create(Glyph::Table, (Glyph::Image, IndexOrder::Asc))
///     .name("idx-glyph-aspect")
///     .col((Glyph::Aspect, IndexOrder::Desc))
///     .unique()
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(),
///     r#"CREATE UNIQUE INDEX "idx-glyph-aspect" ON "glyph" ("image" ASC, "aspect" DESC)"#
/// );
/// ```
/// Index with prefix and order
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create(Glyph::Table, (Glyph::Aspect, 64, IndexOrder::Asc))
///     .name("idx-glyph-aspect")
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect" (64) ASC)"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.index-create+4]
#[derive(Debug, Clone)]
pub struct IndexCreateStatement {
    pub(crate) table: TableName,
    pub(crate) index: TableIndex,
    pub(crate) kind: IndexKind,
    pub(crate) nulls_not_distinct: bool,
    pub(crate) index_type: Option<IndexType>,
    pub(crate) if_not_exists: bool,
}

/// What an index constrains: nothing, uniqueness, or the table's primary key.
///
/// The three states are mutually exclusive, so no index is both a primary key
/// and a unique key. PostgreSQL spells `PRIMARY KEY` only as an inline table
/// constraint, so [`IndexKind::PrimaryKey`] is meaningful only on the embedded
/// path — it is what [`TableCreateStatement::primary_key`] sets. A standalone
/// `CREATE INDEX` sees the kind through [`IndexKind::standalone`], which has no
/// primary-key image.
///
/// [`TableCreateStatement::primary_key`]: crate::TableCreateStatement::primary_key
// [spec:pgorm:req:sql.ddl.index-create+4]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    #[default]
    Plain,
    Unique,
    PrimaryKey,
}

/// The index kinds a standalone `CREATE ... INDEX` can spell.
///
/// Obtained only through [`IndexKind::standalone`], so the standalone renderer
/// cannot be handed a primary key.
// [spec:pgorm:req:sql.ddl.index-create+4]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandaloneIndexKind {
    Plain,
    Unique,
}

impl IndexKind {
    /// This kind as a standalone `CREATE INDEX` prefix, or `None` for
    /// [`IndexKind::PrimaryKey`], which has no standalone spelling.
    pub fn standalone(self) -> Option<StandaloneIndexKind> {
        match self {
            Self::Plain => Some(StandaloneIndexKind::Plain),
            Self::Unique => Some(StandaloneIndexKind::Unique),
            Self::PrimaryKey => None,
        }
    }
}

/// Specification of a table index
#[derive(Debug, Clone)]
pub enum IndexType {
    BTree,
    FullText,
    Hash,
    Custom(DynIden),
}

impl IndexCreateStatement {
    /// Construct a new [`IndexCreateStatement`] over its table and first column
    pub fn new<T, C>(table: T, col: C) -> Self
    where
        T: IntoTableName,
        C: IntoIndexColumn,
    {
        let mut index = TableIndex::new();
        index.col(col.into_index_column());
        Self {
            table: table.into_table_name(),
            index,
            kind: IndexKind::default(),
            nulls_not_distinct: false,
            index_type: None,
            if_not_exists: false,
        }
    }

    /// Create index if index not exists
    pub fn if_not_exists(&mut self) -> &mut Self {
        self.if_not_exists = true;
        self
    }

    /// Set index name
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoIden,
    {
        self.index.name(name);
        self
    }

    /// Add a further index column, after the one the constructor took
    pub fn col<C>(&mut self, col: C) -> &mut Self
    where
        C: IntoIndexColumn,
    {
        self.index.col(col.into_index_column());
        self
    }

    /// Set index kind to [`IndexKind::PrimaryKey`], replacing any kind already
    /// set.
    ///
    /// A primary key is only spelled inside `CREATE TABLE`; rendered standalone
    /// the statement is a plain `CREATE INDEX`.
    pub fn primary(&mut self) -> &mut Self {
        self.kind = IndexKind::PrimaryKey;
        self
    }

    /// Set index kind to [`IndexKind::Unique`], replacing any kind already set.
    pub fn unique(&mut self) -> &mut Self {
        self.kind = IndexKind::Unique;
        self
    }

    /// Set nulls to not be treated as distinct values.
    ///
    /// PostgreSQL defines this only for unique indexes and unique constraints,
    /// so it is rendered only when the kind is [`IndexKind::Unique`].
    pub fn nulls_not_distinct(&mut self) -> &mut Self {
        self.nulls_not_distinct = true;
        self
    }

    /// Set index as full text.
    /// On MySQL, this is `FULLTEXT`.
    /// On PgSQL, this is `GIN`.
    pub fn full_text(&mut self) -> &mut Self {
        self.index_type(IndexType::FullText)
    }

    /// Set index type. Not available on Sqlite.
    pub fn index_type(&mut self, index_type: IndexType) -> &mut Self {
        self.index_type = Some(index_type);
        self
    }

    pub fn kind(&self) -> IndexKind {
        self.kind
    }

    pub fn is_primary_key(&self) -> bool {
        self.kind == IndexKind::PrimaryKey
    }

    pub fn is_unique_key(&self) -> bool {
        self.kind == IndexKind::Unique
    }

    pub fn is_nulls_not_distinct(&self) -> bool {
        self.nulls_not_distinct
    }

    pub fn get_index_spec(&self) -> &TableIndex {
        &self.index
    }

    pub fn get_table_name(&self) -> &TableName {
        &self.table
    }

    /// Clone this statement out of a builder chain.
    ///
    /// This copies rather than moves: moving the table or the columns out would
    /// leave the targetless, column-less index this type exists to rule out.
    pub fn take(&mut self) -> Self {
        self.clone()
    }
}

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for IndexCreateStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_index_create_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
