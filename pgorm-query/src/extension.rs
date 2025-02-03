use core::fmt;

use crate::{DynIden, Iden, IntoIden, PgInterval, QueryBuilder, SqlWriter};

/// Creates a new "CREATE or DROP EXTENSION" statement for PostgreSQL
///
/// # Exampl
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Extension;

impl Extension {
    /// Creates a new [`ExtensionCreateStatement`]
    pub fn create() -> ExtensionCreateStatement {
        ExtensionCreateStatement::new()
    }

    /// Creates a new [`ExtensionDropStatement`]
    pub fn drop() -> ExtensionDropStatement {
        ExtensionDropStatement::new()
    }
}

/// Creates a new "CREATE EXTENSION" statement for PostgreSQL
///
/// # Synopsis
///
/// ```ignore
/// CREATE EXTENSION [ IF NOT EXISTS ] extension_name
///     [ WITH ] [ SCHEMA schema_name ]
///              [ VERSION version ]
///              [ CASCADE ]
/// ```
///
/// # Example
///
/// Creates the "ltree" extension if it doesn't exists.
///
/// ```
/// use pgorm_query::{extension::Extension, *};
///
/// assert_eq!(
///     Extension::create()
///         .name("ltree")
///         .schema("public")
///         .version("v0.1.0")
///         .cascade()
///         .if_not_exists()
///         .to_string(QueryBuilder),
///     r#"CREATE EXTENSION IF NOT EXISTS ltree WITH SCHEMA public VERSION v0.1.0 CASCADE"#
/// );
/// ```
///
/// # References
///
/// [Refer to the PostgreSQL Documentation][1]
///
/// [1]: https://www.postgresql.org/docs/current/sql-createextension.html
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtensionCreateStatement {
    pub(crate) name: String,
    pub(crate) schema: Option<String>,
    pub(crate) version: Option<String>,

    /// Conditional to execute query based on existance of the extension.
    pub(crate) if_not_exists: bool,

    /// Determines the presence of the `RESTRICT` statement
    pub(crate) cascade: bool,
}

impl ExtensionCreateStatement {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the name of the extension to be created.
    pub fn name<T: Into<String>>(&mut self, name: T) -> &mut Self {
        self.name = name.into();
        self
    }

    /// Uses "WITH SCHEMA" on Create Extension Statement.
    pub fn schema<T: Into<String>>(&mut self, schema: T) -> &mut Self {
        self.schema = Some(schema.into());
        self
    }

    /// Uses "VERSION" on Create Extension Statement.
    pub fn version<T: Into<String>>(&mut self, version: T) -> &mut Self {
        self.version = Some(version.into());
        self
    }

    /// Uses "CASCADE" on Create Extension Statement.
    pub fn cascade(&mut self) -> &mut Self {
        self.cascade = true;
        self
    }

    /// Uses "IF NOT EXISTS" on Create Extension Statement.
    pub fn if_not_exists(&mut self) -> &mut Self {
        self.if_not_exists = true;
        self
    }
}

/// Creates a new "DROP EXTENSION" statement for PostgreSQL
///
/// # Synopsis
///
/// ```ignore
/// DROP EXTENSION [ IF EXISTS ] name [, ...] [ CASCADE | RESTRICT ]
/// ```
///
/// # Example
///
/// Drops the "ltree" extension if it exists.
///
/// ```
/// use pgorm_query::{extension::Extension, *};
///
/// assert_eq!(
///     Extension::drop()
///         .name("ltree")
///         .cascade()
///         .if_exists()
///         .to_string(QueryBuilder),
///     r#"DROP EXTENSION IF EXISTS ltree CASCADE"#
/// );
/// ```
///
/// # References
///
/// [Refer to the PostgreSQL Documentation][1]
///
/// [1]: https://www.postgresql.org/docs/current/sql-createextension.html
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtensionDropStatement {
    pub(crate) name: String,
    pub(crate) schema: Option<String>,
    pub(crate) version: Option<String>,

    /// Conditional to execute query based on existance of the extension.
    pub(crate) if_exists: bool,

    /// Determines the presence of the `RESTRICT` statement.
    pub(crate) restrict: bool,

    /// Determines the presence of the `CASCADE` statement
    pub(crate) cascade: bool,
}

impl ExtensionDropStatement {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the name of the extension to be dropped.
    pub fn name<T: Into<String>>(&mut self, name: T) -> &mut Self {
        self.name = name.into();
        self
    }

    /// Uses "IF EXISTS" on Drop Extension Statement.
    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }

    /// Uses "CASCADE" on Drop Extension Statement.
    pub fn cascade(&mut self) -> &mut Self {
        self.cascade = true;
        self
    }

    /// Uses "RESTRICT" on Drop Extension Statement.
    pub fn restrict(&mut self) -> &mut Self {
        self.restrict = true;
        self
    }
}

macro_rules! impl_extension_statement_builder {
    ( $struct_name: ident, $func_name: ident ) => {
        impl $struct_name {
            pub fn build_ref(&self, extension_builder: &QueryBuilder) -> String {
                let mut sql = String::with_capacity(256);
                self.build_collect_ref(extension_builder, &mut sql)
            }

            pub fn build_collect(
                &self,
                extension_builder: QueryBuilder,
                sql: &mut dyn SqlWriter,
            ) -> String {
                self.build_collect_ref(&extension_builder, sql)
            }

            pub fn build_collect_ref(
                &self,
                extension_builder: &QueryBuilder,
                sql: &mut dyn SqlWriter,
            ) -> String {
                extension_builder.$func_name(self, sql);
                sql.to_string()
            }

            /// Build corresponding SQL statement and return SQL string
            pub fn to_string(&self, extension_builder: QueryBuilder) -> String {
                self.build_ref(&extension_builder)
            }
        }
    };
}

impl_extension_statement_builder!(ExtensionCreateStatement, prepare_extension_create_statement);
impl_extension_statement_builder!(ExtensionDropStatement, prepare_extension_drop_statement);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn creates_a_stmt_for_create_extension() {
        let create_extension_stmt = Extension::create()
            .name(PgLTree)
            .schema("public")
            .version("v0.1.0")
            .cascade()
            .if_not_exists()
            .to_owned();

        assert_eq!(create_extension_stmt.name, "ltree");
        assert_eq!(create_extension_stmt.schema, Some("public".to_string()));
        assert_eq!(create_extension_stmt.version, Some("v0.1.0".to_string()));
        assert!(create_extension_stmt.cascade);
        assert!(create_extension_stmt.if_not_exists);
    }

    #[test]
    fn creates_a_stmt_for_drop_extension() {
        let drop_extension_stmt = Extension::drop()
            .name(PgLTree)
            .cascade()
            .if_exists()
            .restrict()
            .to_owned();

        assert_eq!(drop_extension_stmt.name, "ltree");
        assert!(drop_extension_stmt.cascade);
        assert!(drop_extension_stmt.if_exists);
        assert!(drop_extension_stmt.restrict);
    }
}

impl fmt::Display for PgInterval {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let fields = match self {
            PgInterval::Year => "YEAR",
            PgInterval::Month => "MONTH",
            PgInterval::Day => "DAY",
            PgInterval::Hour => "HOUR",
            PgInterval::Minute => "MINUTE",
            PgInterval::Second => "SECOND",
            PgInterval::YearToMonth => "YEAR TO MONTH",
            PgInterval::DayToHour => "DAY TO HOUR",
            PgInterval::DayToMinute => "DAY TO MINUTE",
            PgInterval::DayToSecond => "DAY TO SECOND",
            PgInterval::HourToMinute => "HOUR TO MINUTE",
            PgInterval::HourToSecond => "HOUR TO SECOND",
            PgInterval::MinuteToSecond => "MINUTE TO SECOND",
        };
        write!(f, "{fields}")
    }
}

impl TryFrom<String> for PgInterval {
    type Error = String;

    fn try_from(field: String) -> Result<Self, Self::Error> {
        PgInterval::try_from(field.as_str())
    }
}

impl TryFrom<&String> for PgInterval {
    type Error = String;

    fn try_from(field: &String) -> Result<Self, Self::Error> {
        PgInterval::try_from(field.as_str())
    }
}

impl TryFrom<&str> for PgInterval {
    type Error = String;

    fn try_from(field: &str) -> Result<Self, Self::Error> {
        match field.trim_start().trim_end().to_uppercase().as_ref() {
            "YEAR" => Ok(PgInterval::Year),
            "MONTH" => Ok(PgInterval::Month),
            "DAY" => Ok(PgInterval::Day),
            "HOUR" => Ok(PgInterval::Hour),
            "MINUTE" => Ok(PgInterval::Minute),
            "SECOND" => Ok(PgInterval::Second),
            "YEAR TO MONTH" => Ok(PgInterval::YearToMonth),
            "DAY TO HOUR" => Ok(PgInterval::DayToHour),
            "DAY TO MINUTE" => Ok(PgInterval::DayToMinute),
            "DAY TO SECOND" => Ok(PgInterval::DayToSecond),
            "HOUR TO MINUTE" => Ok(PgInterval::HourToMinute),
            "HOUR TO SECOND" => Ok(PgInterval::HourToSecond),
            "MINUTE TO SECOND" => Ok(PgInterval::MinuteToSecond),
            field => Err(format!(
                "Cannot turn \"{field}\" into a Postgres interval field",
            )),
        }
    }
}

/// PostgreSQL `ltree` extension type.
///
/// `ltree` stores a raber path which in this struct is represented as the
/// tuple's first value.
///
/// # PostcreSQL Reference
///
/// The following set of SQL statements can be used to create a table with
/// a `ltree` column. Here the `ltree` column is called `path`.
///
/// The `path` column is then populated to generate the tree.
///
/// ```ignore
/// CREATE TABLE test (path ltree);
/// INSERT INTO test VALUES ('Top');
/// INSERT INTO test VALUES ('Top.Science');
/// INSERT INTO test VALUES ('Top.Science.Astronomy');
/// INSERT INTO test VALUES ('Top.Science.Astronomy.Astrophysics');
/// INSERT INTO test VALUES ('Top.Science.Astronomy.Cosmology');
/// INSERT INTO test VALUES ('Top.Hobbies');
/// INSERT INTO test VALUES ('Top.Hobbies.Amateurs_Astronomy');
/// INSERT INTO test VALUES ('Top.Collections');
/// INSERT INTO test VALUES ('Top.Collections.Pictures');
/// INSERT INTO test VALUES ('Top.Collections.Pictures.Astronomy');
/// INSERT INTO test VALUES ('Top.Collections.Pictures.Astronomy.Stars');
/// INSERT INTO test VALUES ('Top.Collections.Pictures.Astronomy.Galaxies');
/// INSERT INTO test VALUES ('Top.Collections.Pictures.Astronomy.Astronauts');
/// CREATE INDEX path_gist_idx ON test USING GIST (path);
/// CREATE INDEX path_idx ON test USING BTREE (path);
/// ```
///
/// The set of queries above will generate the following tree:
///
/// ```ignore
///                        Top
///                     /   |  \
///              Science Hobbies Collections
///                /       |              \
///       Astronomy   Amateurs_Astronomy Pictures
///            /  \                            |
/// Astrophysics  Cosmology                Astronomy
///                                       /    |    \
///                                Galaxies  Stars  Astronauts
/// ```
/// [Source][1]
///
/// [1]: https://www.postgresql.org/docs/current/ltree.html
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PgLTree;

impl Iden for PgLTree {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "ltree").unwrap();
    }
}

impl From<PgLTree> for String {
    fn from(l: PgLTree) -> Self {
        l.to_string()
    }
}

/// Helper for constructing any type statement
#[derive(Debug)]
pub struct Type;

#[derive(Clone, Debug)]
pub enum TypeRef {
    Type(DynIden),
    SchemaType(DynIden, DynIden),
    DatabaseSchemaType(DynIden, DynIden, DynIden),
}

pub trait IntoTypeRef {
    fn into_type_ref(self) -> TypeRef;
}

impl IntoTypeRef for TypeRef {
    fn into_type_ref(self) -> TypeRef {
        self
    }
}

impl<I> IntoTypeRef for I
where
    I: IntoIden,
{
    fn into_type_ref(self) -> TypeRef {
        TypeRef::Type(self.into_iden())
    }
}

impl<A, B> IntoTypeRef for (A, B)
where
    A: IntoIden,
    B: IntoIden,
{
    fn into_type_ref(self) -> TypeRef {
        TypeRef::SchemaType(self.0.into_iden(), self.1.into_iden())
    }
}

impl<A, B, C> IntoTypeRef for (A, B, C)
where
    A: IntoIden,
    B: IntoIden,
    C: IntoIden,
{
    fn into_type_ref(self) -> TypeRef {
        TypeRef::DatabaseSchemaType(self.0.into_iden(), self.1.into_iden(), self.2.into_iden())
    }
}

#[derive(Debug, Clone, Default)]
pub struct TypeCreateStatement {
    pub(crate) name: Option<TypeRef>,
    pub(crate) as_type: Option<TypeAs>,
    pub(crate) values: Vec<DynIden>,
}

#[derive(Debug, Clone)]
pub enum TypeAs {
    // Composite,
    Enum,
    /* Range,
     * Base,
     * Array, */
}

#[derive(Debug, Clone, Default)]
pub struct TypeDropStatement {
    pub(crate) names: Vec<TypeRef>,
    pub(crate) option: Option<TypeDropOpt>,
    pub(crate) if_exists: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TypeAlterStatement {
    pub(crate) name: Option<TypeRef>,
    pub(crate) option: Option<TypeAlterOpt>,
}

#[derive(Debug, Clone)]
pub enum TypeDropOpt {
    Cascade,
    Restrict,
}

#[derive(Debug, Clone)]
pub enum TypeAlterOpt {
    Add(DynIden, Option<TypeAlterAddOpt>),
    Rename(DynIden),
    RenameValue(DynIden, DynIden),
}

#[derive(Debug, Clone)]
pub enum TypeAlterAddOpt {
    Before(DynIden),
    After(DynIden),
}

impl Type {
    /// Construct type [`TypeCreateStatement`]
    pub fn create() -> TypeCreateStatement {
        TypeCreateStatement::new()
    }

    /// Construct type [`TypeDropStatement`]
    pub fn drop() -> TypeDropStatement {
        TypeDropStatement::new()
    }

    /// Construct type [`TypeAlterStatement`]
    pub fn alter() -> TypeAlterStatement {
        TypeAlterStatement::new()
    }
}

impl TypeCreateStatement {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create enum as custom type
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type};
    ///
    /// enum FontFamily {
    ///     Type,
    ///     Serif,
    ///     Sans,
    ///     Monospace,
    /// }
    ///
    /// impl Iden for FontFamily {
    ///     fn unquoted(&self, s: &mut dyn Write) {
    ///         write!(
    ///             s,
    ///             "{}",
    ///             match self {
    ///                 Self::Type => "font_family",
    ///                 Self::Serif => "serif",
    ///                 Self::Sans => "sans",
    ///                 Self::Monospace => "monospace",
    ///             }
    ///         )
    ///         .unwrap();
    ///     }
    /// }
    ///
    /// assert_eq!(
    ///     Type::create()
    ///         .as_enum(FontFamily::Type)
    ///         .values([FontFamily::Serif, FontFamily::Sans, FontFamily::Monospace])
    ///         .to_string(QueryBuilder),
    ///     r#"CREATE TYPE "font_family" AS ENUM ('serif', 'sans', 'monospace')"#
    /// );
    /// ```
    pub fn as_enum<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoTypeRef,
    {
        self.name = Some(name.into_type_ref());
        self.as_type = Some(TypeAs::Enum);
        self
    }

    pub fn values<T, I>(&mut self, values: I) -> &mut Self
    where
        T: IntoIden,
        I: IntoIterator<Item = T>,
    {
        for v in values.into_iter() {
            self.values.push(v.into_iden());
        }
        self
    }
}

impl TypeDropStatement {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop a type
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type};
    ///
    /// struct FontFamily;
    ///
    /// impl Iden for FontFamily {
    ///     fn unquoted(&self, s: &mut dyn Write) {
    ///         write!(s, "{}", "font_family").unwrap();
    ///     }
    /// }
    ///
    /// assert_eq!(
    ///     Type::drop()
    ///         .if_exists()
    ///         .name(FontFamily)
    ///         .restrict()
    ///         .to_string(QueryBuilder),
    ///     r#"DROP TYPE IF EXISTS "font_family" RESTRICT"#
    /// );
    /// ```
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoTypeRef,
    {
        self.names.push(name.into_type_ref());
        self
    }

    /// Drop multiple types
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type};
    ///
    /// #[derive(Iden)]
    /// enum KycStatus {
    ///     #[iden = "kyc_status"]
    ///     Type,
    ///     Pending,
    ///     Approved,
    /// }
    ///
    /// #[derive(Iden)]
    /// enum FontFamily {
    ///     #[iden = "font_family"]
    ///     Type,
    ///     Aerial,
    ///     Forte,
    /// }
    ///
    /// assert_eq!(
    ///     Type::drop()
    ///         .if_exists()
    ///         .names([
    ///             SeaRc::new(KycStatus::Type) as DynIden,
    ///             SeaRc::new(FontFamily::Type) as DynIden,
    ///         ])
    ///         .cascade()
    ///         .to_string(QueryBuilder),
    ///     r#"DROP TYPE IF EXISTS "kyc_status", "font_family" CASCADE"#
    /// );
    /// ```
    pub fn names<T, I>(&mut self, names: I) -> &mut Self
    where
        T: IntoTypeRef,
        I: IntoIterator<Item = T>,
    {
        for n in names.into_iter() {
            self.names.push(n.into_type_ref());
        }
        self
    }

    /// Set `IF EXISTS`
    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }

    /// Set `CASCADE`
    pub fn cascade(&mut self) -> &mut Self {
        self.option = Some(TypeDropOpt::Cascade);
        self
    }

    /// Set `RESTRICT`
    pub fn restrict(&mut self) -> &mut Self {
        self.option = Some(TypeDropOpt::Restrict);
        self
    }
}

impl TypeAlterStatement {
    pub fn new() -> Self {
        Self::default()
    }

    /// Change the definition of a type
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type};
    ///
    /// enum FontFamily {
    ///     Type,
    ///     Serif,
    ///     Sans,
    ///     Monospace,
    /// }
    ///
    /// impl Iden for FontFamily {
    ///     fn unquoted(&self, s: &mut dyn Write) {
    ///         write!(
    ///             s,
    ///             "{}",
    ///             match self {
    ///                 Self::Type => "font_family",
    ///                 Self::Serif => "serif",
    ///                 Self::Sans => "sans",
    ///                 Self::Monospace => "monospace",
    ///             }
    ///         )
    ///         .unwrap();
    ///     }
    /// }
    ///
    /// assert_eq!(
    ///     Type::alter()
    ///         .name(FontFamily::Type)
    ///         .add_value(Alias::new("cursive"))
    ///         .to_string(QueryBuilder),
    ///     r#"ALTER TYPE "font_family" ADD VALUE 'cursive'"#
    /// );
    /// ```
    pub fn name<T>(mut self, name: T) -> Self
    where
        T: IntoTypeRef,
    {
        self.name = Some(name.into_type_ref());
        self
    }

    pub fn add_value<T>(self, value: T) -> Self
    where
        T: IntoIden,
    {
        self.alter_option(TypeAlterOpt::Add(value.into_iden(), None))
    }

    /// Add a enum value before an existing value
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Type::alter()
    ///         .name(Font::Table)
    ///         .add_value(Alias::new("weight"))
    ///         .before(Font::Variant)
    ///         .to_string(QueryBuilder),
    ///     r#"ALTER TYPE "font" ADD VALUE 'weight' BEFORE 'variant'"#
    /// )
    /// ```
    pub fn before<T>(mut self, value: T) -> Self
    where
        T: IntoIden,
    {
        if let Some(option) = self.option {
            self.option = Some(option.before(value));
        }
        self
    }

    pub fn after<T>(mut self, value: T) -> Self
    where
        T: IntoIden,
    {
        if let Some(option) = self.option {
            self.option = Some(option.after(value));
        }
        self
    }

    pub fn rename_to<T>(self, name: T) -> Self
    where
        T: IntoIden,
    {
        self.alter_option(TypeAlterOpt::Rename(name.into_iden()))
    }

    /// Rename a enum value
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Type::alter()
    ///         .name(Font::Table)
    ///         .rename_value(Alias::new("variant"), Alias::new("language"))
    ///         .to_string(QueryBuilder),
    ///     r#"ALTER TYPE "font" RENAME VALUE 'variant' TO 'language'"#
    /// )
    /// ```
    pub fn rename_value<T, V>(self, existing: T, new_name: V) -> Self
    where
        T: IntoIden,
        V: IntoIden,
    {
        self.alter_option(TypeAlterOpt::RenameValue(
            existing.into_iden(),
            new_name.into_iden(),
        ))
    }

    fn alter_option(mut self, option: TypeAlterOpt) -> Self {
        self.option = Some(option);
        self
    }
}

impl TypeAlterOpt {
    /// Changes only `ADD VALUE x` options into `ADD VALUE x BEFORE` options, does nothing otherwise
    pub fn before<T>(self, value: T) -> Self
    where
        T: IntoIden,
    {
        match self {
            TypeAlterOpt::Add(iden, _) => {
                Self::Add(iden, Some(TypeAlterAddOpt::Before(value.into_iden())))
            }
            _ => self,
        }
    }

    /// Changes only `ADD VALUE x` options into `ADD VALUE x AFTER` options, does nothing otherwise
    pub fn after<T>(self, value: T) -> Self
    where
        T: IntoIden,
    {
        match self {
            TypeAlterOpt::Add(iden, _) => {
                Self::Add(iden, Some(TypeAlterAddOpt::After(value.into_iden())))
            }
            _ => self,
        }
    }
}

macro_rules! impl_type_statement_builder {
    ( $struct_name: ident, $func_name: ident ) => {
        impl $struct_name {
            pub fn build_ref(&self, type_builder: &QueryBuilder) -> String {
                let mut sql = String::with_capacity(256);
                self.build_collect_ref(type_builder, &mut sql)
            }

            pub fn build_collect(
                &self,
                type_builder: QueryBuilder,
                sql: &mut dyn SqlWriter,
            ) -> String {
                self.build_collect_ref(&type_builder, sql)
            }

            pub fn build_collect_ref(
                &self,
                type_builder: &QueryBuilder,
                sql: &mut dyn SqlWriter,
            ) -> String {
                type_builder.$func_name(self, sql);
                sql.to_string()
            }

            /// Build corresponding SQL statement and return SQL string
            pub fn to_string(&self, type_builder: QueryBuilder) -> String {
                self.build_ref(&type_builder)
            }
        }
    };
}

impl_type_statement_builder!(TypeCreateStatement, prepare_type_create_statement);
impl_type_statement_builder!(TypeAlterStatement, prepare_type_alter_statement);
impl_type_statement_builder!(TypeDropStatement, prepare_type_drop_statement);
