use inherent::inherent;

use crate::{QueryBuilder, SchemaStatementBuilder, types::*};

use super::common::*;

/// Create an index for an existing table
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create()
///     .name("idx-glyph-aspect")
///     .table(Glyph::Table)
///     .col(Glyph::Aspect)
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(QueryBuilder),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect")"#
/// );
/// ```
/// Create index if not exists
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create()
///     .if_not_exists()
///     .name("idx-glyph-aspect")
///     .table(Glyph::Table)
///     .col(Glyph::Aspect)
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(QueryBuilder),
///     r#"CREATE INDEX IF NOT EXISTS "idx-glyph-aspect" ON "glyph" ("aspect")"#
/// );
/// ```
/// Index with prefix
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create()
///     .name("idx-glyph-aspect")
///     .table(Glyph::Table)
///     .col((Glyph::Aspect, 128))
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(QueryBuilder),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect" (128))"#
/// );
/// ```
/// Index with order
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create()
///     .name("idx-glyph-aspect")
///     .table(Glyph::Table)
///     .col((Glyph::Aspect, IndexOrder::Desc))
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(QueryBuilder),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect" DESC)"#
/// );
/// ```
/// Index on multi-columns
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create()
///     .name("idx-glyph-aspect")
///     .table(Glyph::Table)
///     .col((Glyph::Image, IndexOrder::Asc))
///     .col((Glyph::Aspect, IndexOrder::Desc))
///     .unique()
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(QueryBuilder),
///     r#"CREATE UNIQUE INDEX "idx-glyph-aspect" ON "glyph" ("image" ASC, "aspect" DESC)"#
/// );
/// ```
/// Index with prefix and order
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::create()
///     .name("idx-glyph-aspect")
///     .table(Glyph::Table)
///     .col((Glyph::Aspect, 64, IndexOrder::Asc))
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(QueryBuilder),
///     r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect" (64) ASC)"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.index-create+1]
#[derive(Default, Debug, Clone)]
pub struct IndexCreateStatement {
    pub(crate) table: Option<TableRef>,
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
// [spec:pgorm:req:sql.ddl.index-create+1]
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
// [spec:pgorm:req:sql.ddl.index-create+1]
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
    /// Construct a new [`IndexCreateStatement`]
    pub fn new() -> Self {
        Self::default()
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

    /// Set target table
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableRef,
    {
        self.table = Some(table.into_table_ref());
        self
    }

    /// Add index column
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

    pub fn take(&mut self) -> Self {
        Self {
            table: self.table.take(),
            index: self.index.take(),
            kind: self.kind,
            nulls_not_distinct: self.nulls_not_distinct,
            index_type: self.index_type.take(),
            if_not_exists: self.if_not_exists,
        }
    }
}

#[inherent]
impl SchemaStatementBuilder for IndexCreateStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_index_create_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_index_create_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
