use crate::types::*;

/// Specification of a foreign key
///
/// A foreign key maps columns of one table onto columns of another, so both
/// tables and at least one `(column, referenced column)` pair are taken by
/// [`TableForeignKey::new`]: PostgreSQL rejects `ALTER TABLE  ADD FOREIGN KEY`,
/// `REFERENCES  ()`, `FOREIGN KEY ()` and `REFERENCES "t" ()` alike, so none of
/// the four has a field to be left unset in
/// (`[dec:pgorm:invalid-states-unrepresentable]`).
///
/// The two column lists are one list of pairs rather than two lists, so the
/// referencing and referenced sides cannot disagree in length — a mismatch the
/// grammar accepts and parse analysis rejects, and so one no parser oracle can
/// catch. Further pairs are appended with [`TableForeignKey::col`].
// [spec:pgorm:req:sql.ddl.foreign-key+3]
#[derive(Debug, Clone)]
pub struct TableForeignKey {
    pub(crate) name: Option<DynIden>,
    pub(crate) table: TableName,
    pub(crate) ref_table: TableName,
    pub(crate) first: (DynIden, DynIden),
    pub(crate) rest: Vec<(DynIden, DynIden)>,
    pub(crate) on_delete: Option<ForeignKeyAction>,
    pub(crate) on_update: Option<ForeignKeyAction>,
}

/// Foreign key on update & on delete actions
#[derive(Debug, Clone, Copy)]
pub enum ForeignKeyAction {
    Restrict,
    Cascade,
    SetNull,
    NoAction,
    SetDefault,
}

impl TableForeignKey {
    /// Construct a foreign key from the two tables it relates and the first
    /// `(column, referenced column)` pair it maps
    pub fn new<T, C, R, S>(table: T, column: C, ref_table: R, ref_column: S) -> Self
    where
        T: IntoTableName,
        C: IntoIden,
        R: IntoTableName,
        S: IntoIden,
    {
        Self {
            name: None,
            table: table.into_table_name(),
            ref_table: ref_table.into_table_name(),
            first: (column.into_iden(), ref_column.into_iden()),
            rest: Vec::new(),
            on_delete: None,
            on_update: None,
        }
    }

    /// Set foreign key name
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoIden,
    {
        self.name = Some(name.into_iden());
        self
    }

    /// Map a further column onto a further referenced column, as a composite
    /// key requires
    pub fn col<C, S>(&mut self, column: C, ref_column: S) -> &mut Self
    where
        C: IntoIden,
        S: IntoIden,
    {
        self.rest.push((column.into_iden(), ref_column.into_iden()));
        self
    }

    /// Set on delete action
    pub fn on_delete(&mut self, action: ForeignKeyAction) -> &mut Self {
        self.on_delete = Some(action);
        self
    }

    /// Set on update action
    pub fn on_update(&mut self, action: ForeignKeyAction) -> &mut Self {
        self.on_update = Some(action);
        self
    }

    /// Retarget this key at `table`, as an embedding into a `CREATE TABLE` does
    pub(crate) fn retarget(&mut self, table: TableName) {
        self.table = table;
    }

    /// The mapped pairs in declaration order, of which there is at least one
    pub fn columns(&self) -> impl Iterator<Item = &(DynIden, DynIden)> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    pub fn get_table(&self) -> &TableName {
        &self.table
    }

    pub fn get_ref_table(&self) -> &TableName {
        &self.ref_table
    }

    pub fn get_columns(&self) -> Vec<String> {
        self.columns().map(|(col, _)| col.to_string()).collect()
    }

    pub fn get_ref_columns(&self) -> Vec<String> {
        self.columns().map(|(_, col)| col.to_string()).collect()
    }

    pub fn get_on_delete(&self) -> Option<ForeignKeyAction> {
        self.on_delete
    }

    pub fn get_on_update(&self) -> Option<ForeignKeyAction> {
        self.on_update
    }

    /// Clone this key out of a builder chain.
    ///
    /// The tables and the first pair are copied rather than moved: moving them
    /// out would leave the targetless, column-less key this type exists to rule
    /// out.
    pub fn take(&mut self) -> Self {
        Self {
            name: self.name.take(),
            table: self.table.clone(),
            ref_table: self.ref_table.clone(),
            first: self.first.clone(),
            rest: std::mem::take(&mut self.rest),
            on_delete: self.on_delete.take(),
            on_update: self.on_update.take(),
        }
    }
}
