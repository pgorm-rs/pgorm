use crate::{expr::SimpleExpr, types::*};

/// Specification of a table index
#[derive(Default, Debug, Clone)]
pub struct TableIndex {
    pub(crate) name: Option<Name>,
    pub(crate) columns: Vec<IndexColumn>,
}

/// What one entry of an index indexes: a column of the table, or an expression
/// computed over its row.
///
/// The two render differently and PostgreSQL requires the difference — a column
/// stands bare, an expression is parenthesised — so which one an entry holds is
/// a state of the type rather than something the renderer infers from the shape
/// of an expression (`[dec:pgorm:invalid-states-unrepresentable]`).
// [spec:pgorm:req:sql.ddl.index-create+9]
#[derive(Debug, Clone)]
pub enum IndexColumnTarget {
    Name(Name),
    Expr(SimpleExpr),
}

/// One entry of an index: what it indexes, optionally under an operator class,
/// optionally in a stated order.
// [spec:pgorm:req:sql.ddl.index-create+9]
#[derive(Debug, Clone)]
pub struct IndexColumn {
    pub(crate) target: IndexColumnTarget,
    pub(crate) operator_class: Option<Name>,
    pub(crate) order: Option<IndexOrder>,
}

#[derive(Debug, Clone)]
pub enum IndexOrder {
    Asc,
    Desc,
}

impl IndexColumn {
    /// Index the named column.
    // [spec:pgorm:req:sql.ddl.index-create+9]
    pub fn name<N>(name: N) -> Self
    where
        N: IntoName,
    {
        Self {
            target: IndexColumnTarget::Name(name.into_name()),
            operator_class: None,
            order: None,
        }
    }

    /// Index an expression computed over the row — `(lower("name"))`,
    /// `((data ->> 'id'))` — rather than a column of it.
    ///
    /// This is the legitimate occupant of the syntactic position MySQL's
    /// prefix length used to hold. A query reaches such an index only by
    /// writing the same expression it was built from, so the expression here
    /// and the one in the `WHERE` clause should be the same value.
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let lowered = IndexColumn::expr(Func::lower(Expr::col(Glyph::Image)));
    ///
    /// assert_eq!(
    ///     Index::create(Glyph::Table, lowered)
    ///         .name(Name::runtime("idx-glyph-image-lower"))
    ///         .to_string(),
    ///     r#"CREATE INDEX "idx-glyph-image-lower" ON "glyph" ((LOWER("image")))"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ddl.index-create+9]
    pub fn expr<E>(expr: E) -> Self
    where
        E: Into<SimpleExpr>,
    {
        Self {
            target: IndexColumnTarget::Expr(expr.into()),
            operator_class: None,
            order: None,
        }
    }

    /// Build this entry under a named operator class — `text_pattern_ops`,
    /// `gin_trgm_ops` — replacing any already set.
    ///
    /// The class is an identifier, so it renders quoted like every other name
    /// and never as SQL.
    // [spec:pgorm:req:sql.ddl.index-create+9]
    pub fn operator_class<N>(mut self, class: N) -> Self
    where
        N: IntoName,
    {
        self.operator_class = Some(class.into_name());
        self
    }

    /// State the order this entry is stored in, replacing any already set.
    ///
    /// The `(col, IndexOrder)` tuple is the shorthand for a named column; this
    /// is how an expression entry says the same thing.
    // [spec:pgorm:req:sql.ddl.index-create+9]
    pub fn order(mut self, order: IndexOrder) -> Self {
        self.order = Some(order);
        self
    }
}

pub trait IntoIndexColumn {
    fn into_index_column(self) -> IndexColumn;
}

impl IntoIndexColumn for IndexColumn {
    fn into_index_column(self) -> IndexColumn {
        self
    }
}

impl<I> IntoIndexColumn for I
where
    I: IntoName,
{
    fn into_index_column(self) -> IndexColumn {
        IndexColumn::name(self)
    }
}

impl<I> IntoIndexColumn for (I, IndexOrder)
where
    I: IntoName,
{
    fn into_index_column(self) -> IndexColumn {
        IndexColumn::name(self.0).order(self.1)
    }
}

impl TableIndex {
    /// Construct a new table index
    pub fn new() -> Self {
        Self::default()
    }

    /// Set index name
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoName,
    {
        self.name = Some(name.into_name());
        self
    }

    /// Set index column
    pub fn col(&mut self, col: IndexColumn) -> &mut Self {
        self.columns.push(col);
        self
    }

    /// The names of the entries that index a column.
    ///
    /// An expression entry has no name to report and contributes nothing. Its
    /// one caller reads a primary key's columns back, and a primary key indexes
    /// columns only — PostgreSQL has no expression primary key — so there is no
    /// name being dropped there.
    // [spec:pgorm:req:sql.ddl.index-create+9]
    pub fn get_column_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter_map(|col| match &col.target {
                IndexColumnTarget::Name(name) => Some(name.to_string()),
                IndexColumnTarget::Expr(_) => None,
            })
            .collect()
    }

    pub fn take(&mut self) -> Self {
        Self {
            name: self.name.take(),
            columns: std::mem::take(&mut self.columns),
        }
    }
}
