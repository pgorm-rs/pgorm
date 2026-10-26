//! A table read through `TABLESAMPLE`: a random sample of its rows, drawn by
//! one of PostgreSQL's two built-in methods.

use crate::{FromItem, IntoFromItem, IntoNamedTable, NamedTable, Value};

/// How a [`TableSample`] draws its rows: PostgreSQL's two built-in sampling
/// methods.
// [spec:pgorm:req:sql.ast.select.table-sample]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleMethod {
    /// `BERNOULLI`: each row on its own, with the given probability, so the
    /// sample is spread across the table and every page is read.
    Bernoulli,
    /// `SYSTEM`: whole pages, each with the given probability, which reads
    /// less of the table and clusters the sample by page.
    System,
}

/// A table in `FROM` read through `TABLESAMPLE`: `"t" AS "a" TABLESAMPLE
/// BERNOULLI ($1) REPEATABLE ($2)`.
///
/// The percentage is the probability, from 0 to 100, that a row (or a page,
/// for [`SampleMethod::System`]) is drawn. [`repeatable`](Self::repeatable)
/// seeds the draw, so the same seed over unchanged data draws the same
/// sample. Both are bound as parameters, which PostgreSQL types `real` and
/// `double precision`; a percentage outside 0 to 100 and a NULL are its
/// refusals (`2202H`, and `2202G` for a NULL seed).
///
/// Only a table can be sampled, so a sample is built from a table name,
/// optionally schema-qualified and aliased; a subquery, a values list or a
/// function has no value to build one from.
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let (sql, values) = Query::select()
///     .column(Glyph::Image)
///     .from(
///         TableSample::new(
///             Glyph::Table.into_named_table().alias(Name::runtime("g")),
///             SampleMethod::Bernoulli,
///             25,
///         )
///         .repeatable(7),
///     )
///     .build();
///
/// assert_eq!(
///     sql,
///     r#"SELECT "image" FROM "glyph" AS "g" TABLESAMPLE BERNOULLI ($1) REPEATABLE ($2)"#
/// );
/// assert_eq!(values.0, vec![25.0_f64.into(), 7.0_f64.into()]);
/// ```
// [spec:pgorm:req:sql.ast.select.table-sample]
#[derive(Debug, Clone, PartialEq)]
pub struct TableSample {
    pub(crate) table: NamedTable,
    pub(crate) method: SampleMethod,
    pub(crate) percentage: Value,
    pub(crate) seed: Option<Value>,
}

impl TableSample {
    /// Sample `table` by `method`, drawing each row (or page) with
    /// probability `percentage` out of 100.
    pub fn new<T, P>(table: T, method: SampleMethod, percentage: P) -> Self
    where
        T: IntoNamedTable,
        P: Into<f64>,
    {
        Self {
            table: table.into_named_table(),
            method,
            percentage: Value::Double(Some(percentage.into())),
            seed: None,
        }
    }

    /// Seed the draw, `REPEATABLE (seed)`, replacing any seed already given.
    pub fn repeatable<S>(mut self, seed: S) -> Self
    where
        S: Into<f64>,
    {
        self.seed = Some(Value::Double(Some(seed.into())));
        self
    }
}

impl From<TableSample> for FromItem {
    fn from(sample: TableSample) -> Self {
        Self::TableSample(Box::new(sample))
    }
}

impl IntoFromItem for TableSample {
    fn into_from_item(self) -> FromItem {
        self.into()
    }
}
