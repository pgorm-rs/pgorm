//! A column's own unique and primary keys with a deferrability. `ColumnDef`'s
//! file is over its function cap, so these two setters sit beside it here.

use crate::Deferrability;

use super::{ColumnDef, ColumnSpec};

impl ColumnDef {
    /// Set a column unique constraint whose check runs when `deferrability`
    /// says: `UNIQUE DEFERRABLE INITIALLY DEFERRED` and its two siblings.
    ///
    /// The clause rides inside the spec, so it is written directly after its
    /// `UNIQUE` and cannot trail another clause, where PostgreSQL refuses it
    /// as misplaced. A deferrable key cannot arbitrate an `ON CONFLICT`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::create(Glyph::Table)
    ///     .col(
    ///         ColumnDef::new(Glyph::Aspect)
    ///             .integer()
    ///             .not_null()
    ///             .unique_key_deferrability(Deferrability::DeferrableInitiallyDeferred),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     r#"CREATE TABLE "glyph" ( "aspect" integer NOT NULL UNIQUE DEFERRABLE INITIALLY DEFERRED )"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ddl.deferrability]
    pub fn unique_key_deferrability(&mut self, deferrability: Deferrability) -> &mut Self {
        self.spec.push(ColumnSpec::UniqueKey(Some(deferrability)));
        self
    }

    /// Set a column as primary key whose check runs when `deferrability`
    /// says: `PRIMARY KEY DEFERRABLE INITIALLY DEFERRED` and its two siblings.
    // [spec:pgorm:req:sql.ddl.deferrability]
    pub fn primary_key_deferrability(&mut self, deferrability: Deferrability) -> &mut Self {
        self.spec.push(ColumnSpec::PrimaryKey(Some(deferrability)));
        self
    }
}
