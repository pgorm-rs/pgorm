use crate::Deferrability;

use super::IndexCreateStatement;

/// An index written as a `CREATE TABLE` constraint — `UNIQUE (…)` or
/// `PRIMARY KEY (…)` — together with the deferrability only that position
/// takes.
///
/// PostgreSQL defers *constraints*, never indexes: `CREATE UNIQUE INDEX` has
/// no `DEFERRABLE` clause in its grammar. So deferrability is not a field of
/// [`IndexCreateStatement`], which renders on its own as that statement, but
/// of this wrapper, which has no rendering of its own at all and reaches SQL
/// only embedded, through
/// [`TableCreateStatement::index`](crate::TableCreateStatement::index) or
/// [`primary_key`](crate::TableCreateStatement::primary_key). Both accept a
/// plain [`IndexCreateStatement`] too, which converts into a constraint that
/// says nothing about deferral.
///
/// ```compile_fail,E0599
/// use pgorm_query::{*, tests_cfg::*};
///
/// // A deferrable index has no standalone rendering to call.
/// Index::create(Glyph::Table, Glyph::Aspect)
///     .unique()
///     .to_owned()
///     .deferrability(Deferrability::DeferrableInitiallyDeferred)
///     .to_string();
/// ```
// [spec:pgorm:req:sql.ddl.deferrability]
#[derive(Debug, Clone)]
pub struct IndexConstraint {
    pub(crate) index: IndexCreateStatement,
    pub(crate) deferrability: Option<Deferrability>,
}

impl IndexConstraint {
    /// The index this constraint is written from.
    pub fn get_index(&self) -> &IndexCreateStatement {
        &self.index
    }

    /// When this constraint's check runs, if the caller said.
    pub fn get_deferrability(&self) -> Option<Deferrability> {
        self.deferrability
    }
}

impl From<IndexCreateStatement> for IndexConstraint {
    fn from(index: IndexCreateStatement) -> Self {
        Self {
            index,
            deferrability: None,
        }
    }
}

impl IndexCreateStatement {
    /// This index as a table constraint whose check runs when `deferrability`
    /// says, for [`TableCreateStatement::index`](crate::TableCreateStatement::index)
    /// or [`primary_key`](crate::TableCreateStatement::primary_key) to embed.
    ///
    /// The statement is consumed, as the embedders consume what they embed, so a
    /// builder chain is ended with `.to_owned()` first. What comes back has no
    /// standalone rendering: only a constraint can be deferred, and a unique or
    /// primary-key constraint is spelled only inside `CREATE TABLE`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::create(Glyph::Table)
    ///     .col(ColumnDef::new(Glyph::Id).integer().not_null())
    ///     .col(ColumnDef::new(Glyph::Aspect).integer().not_null())
    ///     .primary_key(
    ///         Index::create(Glyph::Table, Glyph::Id)
    ///             .to_owned()
    ///             .deferrability(Deferrability::DeferrableInitiallyDeferred),
    ///     )
    ///     .index(
    ///         Index::create(Glyph::Table, Glyph::Aspect)
    ///             .name(Name::runtime("glyph_aspect"))
    ///             .unique()
    ///             .to_owned()
    ///             .deferrability(Deferrability::DeferrableInitiallyImmediate),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"CREATE TABLE "glyph" ("#,
    ///         r#""id" integer NOT NULL,"#,
    ///         r#""aspect" integer NOT NULL,"#,
    ///         r#"PRIMARY KEY ("id") DEFERRABLE INITIALLY DEFERRED,"#,
    ///         r#"CONSTRAINT "glyph_aspect" UNIQUE ("aspect") DEFERRABLE INITIALLY IMMEDIATE"#,
    ///         r#")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    // [spec:pgorm:req:sql.ddl.deferrability]
    pub fn deferrability(self, deferrability: Deferrability) -> IndexConstraint {
        IndexConstraint {
            index: self,
            deferrability: Some(deferrability),
        }
    }
}
