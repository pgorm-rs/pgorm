use crate::types::*;

/// Specification of a table index
#[derive(Default, Debug, Clone)]
pub struct TableIndex {
    pub(crate) name: Option<Name>,
    pub(crate) columns: Vec<IndexColumn>,
}

#[derive(Debug, Clone)]
pub struct IndexColumn {
    pub(crate) name: Name,
    pub(crate) order: Option<IndexOrder>,
}

#[derive(Debug, Clone)]
pub enum IndexOrder {
    Asc,
    Desc,
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
        IndexColumn {
            name: self.into_name(),
            order: None,
        }
    }
}

impl<I> IntoIndexColumn for (I, IndexOrder)
where
    I: IntoName,
{
    fn into_index_column(self) -> IndexColumn {
        IndexColumn {
            name: self.0.into_name(),
            order: Some(self.1),
        }
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

    pub fn get_column_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|col| col.name.to_string())
            .collect()
    }

    pub fn take(&mut self) -> Self {
        Self {
            name: self.name.take(),
            columns: std::mem::take(&mut self.columns),
        }
    }
}
