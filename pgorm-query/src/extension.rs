use core::fmt;

use crate::{DynIden, Iden, IntoIden, PgInterval, QueryBuilder, SqlWriter};

/// Creates a new "CREATE or DROP EXTENSION" statement for PostgreSQL
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Extension;

impl Extension {
    /// Creates a new [`ExtensionCreateStatement`] over the extension it creates
    pub fn create<T>(name: T) -> ExtensionCreateStatement
    where
        T: IntoIden,
    {
        ExtensionCreateStatement::new(name)
    }

    /// Creates a new [`ExtensionDropStatement`] over the extension it drops
    pub fn drop<T>(name: T) -> ExtensionDropStatement
    where
        T: IntoIden,
    {
        ExtensionDropStatement::new(name)
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
///     Extension::create("ltree")
///         .schema("public")
///         .version("v0.1.0")
///         .cascade()
///         .if_not_exists()
///         .to_string(),
///     r#"CREATE EXTENSION IF NOT EXISTS "ltree" WITH SCHEMA "public" VERSION 'v0.1.0' CASCADE"#
/// );
/// ```
///
/// The extension name is taken by the constructor, because a statement that
/// never names one renders the zero-length delimited identifier PostgreSQL
/// rejects (`[dec:pgorm:invalid-states-unrepresentable]`):
///
/// ```compile_fail,E0061
/// use pgorm_query::{extension::Extension, *};
///
/// Extension::create().schema("public");
/// ```
///
/// # References
///
/// [Refer to the PostgreSQL Documentation][1]
///
/// [1]: https://www.postgresql.org/docs/current/sql-createextension.html
// [spec:pgorm:req:sql.ddl.extension+3]
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionCreateStatement {
    pub(crate) name: DynIden,
    pub(crate) schema: Option<String>,
    pub(crate) version: Option<String>,

    /// Conditional to execute query based on existance of the extension.
    pub(crate) if_not_exists: bool,

    /// Determines the presence of the `RESTRICT` statement
    pub(crate) cascade: bool,
}

impl ExtensionCreateStatement {
    /// Construct a new statement over the extension it creates
    pub fn new<T>(name: T) -> Self
    where
        T: IntoIden,
    {
        Self {
            name: name.into_iden(),
            schema: None,
            version: None,
            if_not_exists: false,
            cascade: false,
        }
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
///     Extension::drop("ltree").cascade().if_exists().to_string(),
///     r#"DROP EXTENSION IF EXISTS "ltree" CASCADE"#
/// );
/// ```
///
/// The extension name is taken by the constructor, for the reason it is on
/// [`ExtensionCreateStatement`]:
///
/// ```compile_fail,E0061
/// use pgorm_query::{extension::Extension, *};
///
/// Extension::drop().if_exists();
/// ```
///
/// # References
///
/// [Refer to the PostgreSQL Documentation][1]
///
/// [1]: https://www.postgresql.org/docs/current/sql-createextension.html
// [spec:pgorm:req:sql.ddl.extension+3]
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionDropStatement {
    pub(crate) name: DynIden,

    /// Conditional to execute query based on existance of the extension.
    pub(crate) if_exists: bool,

    /// The drop behaviour, at most one of `CASCADE` and `RESTRICT`.
    pub(crate) option: Option<ExtensionDropOpt>,
}

/// The drop behaviour of a `DROP EXTENSION` statement.
///
/// PostgreSQL takes one of `CASCADE` and `RESTRICT`, never both, so the two
/// spellings share one slot.
// [spec:pgorm:req:sql.ddl.extension+3]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionDropOpt {
    Cascade,
    Restrict,
}

impl ExtensionDropStatement {
    /// Construct a new statement over the extension it drops
    pub fn new<T>(name: T) -> Self
    where
        T: IntoIden,
    {
        Self {
            name: name.into_iden(),
            if_exists: false,
            option: None,
        }
    }

    /// Uses "IF EXISTS" on Drop Extension Statement.
    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }

    /// Uses "CASCADE" on Drop Extension Statement, replacing any "RESTRICT".
    pub fn cascade(&mut self) -> &mut Self {
        self.option = Some(ExtensionDropOpt::Cascade);
        self
    }

    /// Uses "RESTRICT" on Drop Extension Statement, replacing any "CASCADE".
    pub fn restrict(&mut self) -> &mut Self {
        self.option = Some(ExtensionDropOpt::Restrict);
        self
    }
}

macro_rules! impl_extension_statement_builder {
    ( $struct_name: ident, $func_name: ident ) => {
        impl $struct_name {
            /// Build the SQL statement into the given sink, returning the sink's text
            pub fn build_collect(&self, sql: &mut dyn SqlWriter) -> String {
                QueryBuilder.$func_name(self, sql);
                sql.to_string()
            }
        }

        // [spec:pgorm:req:sql.ddl+5] (the one rendering an extension statement has)
        impl fmt::Display for $struct_name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut sql = String::with_capacity(256);
                QueryBuilder.$func_name(self, &mut sql);
                f.write_str(&sql)
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
        let create_extension_stmt = Extension::create(PgLTree)
            .schema("public")
            .version("v0.1.0")
            .cascade()
            .if_not_exists()
            .to_owned();

        assert_eq!(create_extension_stmt.name.to_string(), "ltree");
        assert_eq!(create_extension_stmt.schema, Some("public".to_string()));
        assert_eq!(create_extension_stmt.version, Some("v0.1.0".to_string()));
        assert!(create_extension_stmt.cascade);
        assert!(create_extension_stmt.if_not_exists);
    }

    #[test]
    fn creates_a_stmt_for_drop_extension() {
        let drop_extension_stmt = Extension::drop(PgLTree)
            .cascade()
            .if_exists()
            .restrict()
            .to_owned();

        assert_eq!(drop_extension_stmt.name.to_string(), "ltree");
        assert!(drop_extension_stmt.if_exists);
        assert_eq!(drop_extension_stmt.option, Some(ExtensionDropOpt::Restrict));
    }
}

// [spec:pgorm:def:sql.types.column-type+3]
impl fmt::Display for PgInterval {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (fields, precision) = match self {
            PgInterval::Year => ("YEAR", None),
            PgInterval::Month => ("MONTH", None),
            PgInterval::Day => ("DAY", None),
            PgInterval::Hour => ("HOUR", None),
            PgInterval::Minute => ("MINUTE", None),
            PgInterval::Second(precision) => ("SECOND", *precision),
            PgInterval::YearToMonth => ("YEAR TO MONTH", None),
            PgInterval::DayToHour => ("DAY TO HOUR", None),
            PgInterval::DayToMinute => ("DAY TO MINUTE", None),
            PgInterval::DayToSecond(precision) => ("DAY TO SECOND", *precision),
            PgInterval::HourToMinute => ("HOUR TO MINUTE", None),
            PgInterval::HourToSecond(precision) => ("HOUR TO SECOND", *precision),
            PgInterval::MinuteToSecond(precision) => ("MINUTE TO SECOND", *precision),
        };
        write!(f, "{fields}")?;
        match precision {
            Some(precision) => write!(f, "({precision})"),
            None => Ok(()),
        }
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
            "SECOND" => Ok(PgInterval::Second(None)),
            "YEAR TO MONTH" => Ok(PgInterval::YearToMonth),
            "DAY TO HOUR" => Ok(PgInterval::DayToHour),
            "DAY TO MINUTE" => Ok(PgInterval::DayToMinute),
            "DAY TO SECOND" => Ok(PgInterval::DayToSecond(None)),
            "HOUR TO MINUTE" => Ok(PgInterval::HourToMinute),
            "HOUR TO SECOND" => Ok(PgInterval::HourToSecond(None)),
            "MINUTE TO SECOND" => Ok(PgInterval::MinuteToSecond(None)),
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
/// ```sql
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
/// ```text
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

/// Create a type
///
/// The type name is taken by [`Type::create`], because `CREATE TYPE ` is
/// rejected at the token after it (`[dec:pgorm:invalid-states-unrepresentable]`):
///
/// ```compile_fail,E0061
/// use pgorm_query::{extension::Type, tests_cfg::*, *};
///
/// Type::create().values([Font::Name]);
/// ```
///
/// The name alone is a shell type, which PostgreSQL accepts; `as_enum` and
/// `values` make it an enumeration, and the parenthesised value list is always
/// rendered once it is one, because `CREATE TYPE "t" AS ENUM ()` is an accepted
/// spelling of the empty enum while `CREATE TYPE "t" AS ENUM` is not a
/// statement at all.
// [spec:pgorm:req:sql.ddl.type-enum+2]
#[derive(Debug, Clone)]
pub struct TypeCreateStatement {
    pub(crate) name: TypeRef,
    pub(crate) as_type: Option<TypeAs>,
}

/// What a `CREATE TYPE` defines, when it defines more than a shell type.
// [spec:pgorm:req:sql.ddl.type-enum+2]
#[derive(Debug, Clone)]
pub enum TypeAs {
    // Composite,
    /// `AS ENUM (..)`, carrying its labels: the marker and the values are one
    /// fact, so no value list survives without the `AS ENUM` that renders it.
    Enum(Vec<DynIden>),
    /* Range,
     * Base,
     * Array, */
}

/// Drop one or more types
///
/// The first name is taken by [`Type::drop`] and every further one is appended,
/// so the name list is non-empty: PostgreSQL rejects `DROP TYPE ` at end of
/// input (`[dec:pgorm:invalid-states-unrepresentable]`).
///
/// ```compile_fail,E0061
/// use pgorm_query::{extension::Type, *};
///
/// Type::drop().if_exists();
/// ```
// [spec:pgorm:req:sql.ddl.type-alter-drop+3]
#[derive(Debug, Clone)]
pub struct TypeDropStatement {
    pub(crate) first: TypeRef,
    pub(crate) rest: Vec<TypeRef>,
    pub(crate) option: Option<TypeDropOpt>,
    pub(crate) if_exists: bool,
}

/// A type awaiting its first alter option.
///
/// PostgreSQL has no spelling for an `ALTER TYPE` that does nothing, so this is
/// what [`Type::alter`] returns: naming the type is not yet a statement. Each
/// option method consumes it and yields a [`TypeAlterStatement`].
///
/// ```compile_fail,E0599
/// use pgorm_query::{extension::Type, tests_cfg::*, *};
///
/// Type::alter(Font::Table).to_string();
/// ```
// [spec:pgorm:req:sql.ddl.type-alter-drop+3]
#[derive(Debug, Clone)]
pub struct PendingTypeAlter {
    name: TypeRef,
}

/// Alter a type
///
/// A statement of this type always names a type and always carries exactly one
/// option: it is reachable only by choosing an option on a
/// [`PendingTypeAlter`], so the `ALTER TYPE "font"` PostgreSQL rejects has no
/// constructor.
// [spec:pgorm:req:sql.ddl.type-alter-drop+3]
#[derive(Debug, Clone)]
pub struct TypeAlterStatement {
    pub(crate) name: TypeRef,
    pub(crate) option: TypeAlterOpt,
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
    /// Construct type [`TypeCreateStatement`] over the type it creates
    pub fn create<T>(name: T) -> TypeCreateStatement
    where
        T: IntoTypeRef,
    {
        TypeCreateStatement::new(name)
    }

    /// Construct type [`TypeDropStatement`] over the first type it drops
    pub fn drop<T>(name: T) -> TypeDropStatement
    where
        T: IntoTypeRef,
    {
        TypeDropStatement::new(name)
    }

    /// Name the type a [`TypeAlterStatement`] will alter
    pub fn alter<T>(name: T) -> PendingTypeAlter
    where
        T: IntoTypeRef,
    {
        PendingTypeAlter::new(name)
    }
}

impl TypeCreateStatement {
    /// Construct a new statement over the type it creates
    pub fn new<T>(name: T) -> Self
    where
        T: IntoTypeRef,
    {
        Self {
            name: name.into_type_ref(),
            as_type: None,
        }
    }

    /// Define the type as an enumeration, whose values are appended by
    /// [`TypeCreateStatement::values`]
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Type::create(Font::Table).as_enum().to_string(),
    ///     r#"CREATE TYPE "font" AS ENUM ()"#
    /// );
    /// ```
    pub fn as_enum(&mut self) -> &mut Self {
        if self.as_type.is_none() {
            self.as_type = Some(TypeAs::Enum(Vec::new()));
        }
        self
    }

    /// Append enum values, defining the type as an enumeration if it is not one
    /// already
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
    ///     Type::create(FontFamily::Type)
    ///         .values([FontFamily::Serif, FontFamily::Sans, FontFamily::Monospace])
    ///         .to_string(),
    ///     r#"CREATE TYPE "font_family" AS ENUM ('serif', 'sans', 'monospace')"#
    /// );
    /// ```
    pub fn values<T, I>(&mut self, values: I) -> &mut Self
    where
        T: IntoIden,
        I: IntoIterator<Item = T>,
    {
        self.as_enum();
        if let Some(TypeAs::Enum(existing)) = self.as_type.as_mut() {
            existing.extend(values.into_iter().map(IntoIden::into_iden));
        }
        self
    }
}

impl TypeDropStatement {
    /// Construct a new statement over the first type it drops
    pub fn new<T>(name: T) -> Self
    where
        T: IntoTypeRef,
    {
        Self {
            first: name.into_type_ref(),
            rest: Vec::new(),
            option: None,
            if_exists: false,
        }
    }

    /// Drop a further type
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
    ///     Type::drop(FontFamily).if_exists().restrict().to_string(),
    ///     r#"DROP TYPE IF EXISTS "font_family" RESTRICT"#
    /// );
    /// ```
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoTypeRef,
    {
        self.rest.push(name.into_type_ref());
        self
    }

    /// Drop further types
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
    ///     Type::drop(SeaRc::new(KycStatus::Type) as DynIden)
    ///         .if_exists()
    ///         .names([SeaRc::new(FontFamily::Type) as DynIden])
    ///         .cascade()
    ///         .to_string(),
    ///     r#"DROP TYPE IF EXISTS "kyc_status", "font_family" CASCADE"#
    /// );
    /// ```
    pub fn names<T, I>(&mut self, names: I) -> &mut Self
    where
        T: IntoTypeRef,
        I: IntoIterator<Item = T>,
    {
        self.rest
            .extend(names.into_iter().map(IntoTypeRef::into_type_ref));
        self
    }

    /// The types dropped, in declaration order, of which there is at least one
    pub fn names_iter(&self) -> impl Iterator<Item = &TypeRef> {
        std::iter::once(&self.first).chain(self.rest.iter())
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

impl PendingTypeAlter {
    fn new<T>(name: T) -> Self
    where
        T: IntoTypeRef,
    {
        Self {
            name: name.into_type_ref(),
        }
    }

    fn with(self, option: TypeAlterOpt) -> TypeAlterStatement {
        TypeAlterStatement {
            name: self.name,
            option,
        }
    }

    /// Add an enum value
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
    ///     Type::alter(FontFamily::Type)
    ///         .add_value(Alias::new("cursive"))
    ///         .to_string(),
    ///     r#"ALTER TYPE "font_family" ADD VALUE 'cursive'"#
    /// );
    /// ```
    pub fn add_value<T>(self, value: T) -> TypeAlterStatement
    where
        T: IntoIden,
    {
        self.with(TypeAlterOpt::Add(value.into_iden(), None))
    }

    /// Rename the type
    pub fn rename_to<T>(self, name: T) -> TypeAlterStatement
    where
        T: IntoIden,
    {
        self.with(TypeAlterOpt::Rename(name.into_iden()))
    }

    /// Rename an enum value
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Type::alter(Font::Table)
    ///         .rename_value(Alias::new("variant"), Alias::new("language"))
    ///         .to_string(),
    ///     r#"ALTER TYPE "font" RENAME VALUE 'variant' TO 'language'"#
    /// )
    /// ```
    pub fn rename_value<T, V>(self, existing: T, new_name: V) -> TypeAlterStatement
    where
        T: IntoIden,
        V: IntoIden,
    {
        self.with(TypeAlterOpt::RenameValue(
            existing.into_iden(),
            new_name.into_iden(),
        ))
    }
}

impl TypeAlterStatement {
    /// Add a enum value before an existing value
    ///
    /// ```
    /// use pgorm_query::{*, extension::Type, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Type::alter(Font::Table)
    ///         .add_value(Alias::new("weight"))
    ///         .before(Font::Variant)
    ///         .to_string(),
    ///     r#"ALTER TYPE "font" ADD VALUE 'weight' BEFORE 'variant'"#
    /// )
    /// ```
    #[must_use]
    pub fn before<T>(mut self, value: T) -> Self
    where
        T: IntoIden,
    {
        self.option = self.option.before(value);
        self
    }

    #[must_use]
    pub fn after<T>(mut self, value: T) -> Self
    where
        T: IntoIden,
    {
        self.option = self.option.after(value);
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
            /// Build the SQL statement into the given sink, returning the sink's text
            pub fn build_collect(&self, sql: &mut dyn SqlWriter) -> String {
                QueryBuilder.$func_name(self, sql);
                sql.to_string()
            }
        }

        // [spec:pgorm:req:sql.ddl+5] (the one rendering a type statement has)
        impl fmt::Display for $struct_name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut sql = String::with_capacity(256);
                QueryBuilder.$func_name(self, &mut sql);
                f.write_str(&sql)
            }
        }
    };
}

impl_type_statement_builder!(TypeCreateStatement, prepare_type_create_statement);
impl_type_statement_builder!(TypeAlterStatement, prepare_type_alter_statement);
impl_type_statement_builder!(TypeDropStatement, prepare_type_drop_statement);
