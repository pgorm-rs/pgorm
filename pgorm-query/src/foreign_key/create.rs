use crate::{ForeignKeyAction, QueryBuilder, TableForeignKey, types::*};

/// Create a foreign key constraint for an existing table
///
/// The two tables and the first `(column, referenced column)` pair are taken by
/// [`ForeignKey::create`], because PostgreSQL rejects every render that leaves
/// one of them out; further pairs are appended with
/// [`ForeignKeyCreateStatement::col`]. A half-named key does not construct:
///
/// ```compile_fail,E0061
/// use pgorm_query::{*, tests_cfg::*};
///
/// ForeignKey::create().name("FK_character_font");
/// ```
///
/// and neither does one that names the referencing side alone:
///
/// ```compile_fail,E0061
/// use pgorm_query::{*, tests_cfg::*};
///
/// ForeignKey::create(Char::Table, Char::FontId);
/// ```
///
/// [`ForeignKey::create`]: crate::ForeignKey::create
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let foreign_key = ForeignKey::create(Char::Table, Char::FontId, Font::Table, Font::Id)
///     .name("FK_character_font")
///     .to_owned();
///
/// assert_eq!(
///     foreign_key.to_string(),
///     [
///         r#"ALTER TABLE "character" ADD CONSTRAINT "FK_character_font""#,
///         r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
///     ]
///     .join(" ")
/// );
/// ```
///
/// Composite key
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let foreign_key = ForeignKey::create(Char::Table, Char::FontId, Glyph::Table, Char::FontId)
///     .name("FK_character_glyph")
///     .col(Char::Id, Glyph::Id)
///     .to_owned();
///
/// assert_eq!(
///     foreign_key.to_string(),
///     [
///         r#"ALTER TABLE "character" ADD CONSTRAINT "FK_character_glyph""#,
///         r#"FOREIGN KEY ("font_id", "id") REFERENCES "glyph" ("font_id", "id")"#,
///     ]
///     .join(" ")
/// );
/// ```
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let foreign_key = ForeignKey::create(
///     Character::Table,
///     Character::Id,
///     Character::Table,
///     Character::Id,
/// )
/// .name("FK_character_id")
/// .to_owned();
///
/// assert_eq!(
///     foreign_key.to_string(),
///     r#"ALTER TABLE "character" ADD CONSTRAINT "FK_character_id" FOREIGN KEY ("id") REFERENCES "character" ("id")"#
/// );
/// ```
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let foreign_key = ForeignKey::create(
///     Character::Table,
///     Character::Id,
///     Character::Table,
///     Character::Id,
/// )
/// .name("FK_character_id")
/// .on_delete(ForeignKeyAction::Cascade)
/// .on_update(ForeignKeyAction::Cascade)
/// .to_owned();
///
/// assert_eq!(
///     foreign_key.to_string(),
///     r#"ALTER TABLE "character" ADD CONSTRAINT "FK_character_id" FOREIGN KEY ("id") REFERENCES "character" ("id") ON DELETE CASCADE ON UPDATE CASCADE"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.foreign-key+3]
#[derive(Debug, Clone)]
pub struct ForeignKeyCreateStatement {
    pub(crate) foreign_key: TableForeignKey,
}

impl ForeignKeyCreateStatement {
    /// Construct a new [`ForeignKeyCreateStatement`] over the two tables it
    /// relates and the first `(column, referenced column)` pair it maps
    pub fn new<T, C, R, S>(table: T, column: C, ref_table: R, ref_column: S) -> Self
    where
        T: IntoTableName,
        C: IntoIden,
        R: IntoTableName,
        S: IntoIden,
    {
        Self {
            foreign_key: TableForeignKey::new(table, column, ref_table, ref_column),
        }
    }

    /// Set foreign key name
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoIden,
    {
        self.foreign_key.name(name);
        self
    }

    /// Map a further column onto a further referenced column, as a composite
    /// key requires
    pub fn col<C, S>(&mut self, column: C, ref_column: S) -> &mut Self
    where
        C: IntoIden,
        S: IntoIden,
    {
        self.foreign_key.col(column, ref_column);
        self
    }

    /// Set on delete action
    pub fn on_delete(&mut self, action: ForeignKeyAction) -> &mut Self {
        self.foreign_key.on_delete(action);
        self
    }

    /// Set on update action
    pub fn on_update(&mut self, action: ForeignKeyAction) -> &mut Self {
        self.foreign_key.on_update(action);
        self
    }

    pub fn get_foreign_key(&self) -> &TableForeignKey {
        &self.foreign_key
    }

    pub fn take(&mut self) -> Self {
        Self {
            foreign_key: self.foreign_key.take(),
        }
    }
}

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for ForeignKeyCreateStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_foreign_key_create_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
