use super::interval::IntervalSpec;
use crate::{expr::*, types::*};
use std::sync::Arc;

/// Specification of a table column
// [spec:pgorm:req:sql.ddl.column-def+5]
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub(crate) table: Option<TableName>,
    pub(crate) name: Name,
    pub(crate) types: Option<ColumnType>,
    pub(crate) spec: Vec<ColumnSpec>,
}

pub trait IntoColumnDef {
    fn into_column_def(self) -> ColumnDef;
}

/// All column types
///
/// | ColumnType            | PostgreSQL data type     |
/// |-----------------------|--------------------------|
/// | Char                  | char                     |
/// | String                | varchar                  |
/// | Text                  | text                     |
/// | SmallInteger          | smallint                 |
/// | Integer               | integer                  |
/// | BigInteger            | bigint                   |
/// | Float                 | real                     |
/// | Double                | double precision         |
/// | Decimal               | decimal                  |
/// | Timestamp             | timestamp                |
/// | TimestampWithTimeZone | timestamp with time zone |
/// | Time                  | time                     |
/// | Date                  | date                     |
/// | Interval              | interval                 |
/// | Bytea                 | bytea                    |
/// | Bit                   | bit                      |
/// | VarBit                | varbit                   |
/// | Boolean               | bool                     |
/// | Money                 | money                    |
/// | Json                  | json                     |
/// | JsonBinary            | jsonb                    |
/// | Uuid                  | uuid                     |
/// | Enum                  | ENUM_NAME                |
/// | Array                 | DATA_TYPE[]              |
/// | Vector                | vector                   |
/// | Cidr                  | cidr                     |
/// | Inet                  | inet                     |
/// | MacAddr               | macaddr                  |
/// | LTree                 | ltree                    |
// [spec:pgorm:def:sql.types.column-type+7]
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum ColumnType {
    Char(Option<u32>),
    String(StringLen),
    Text,
    Bytea,
    SmallInteger,
    Integer,
    BigInteger,
    Float,
    Double,
    Decimal(Option<(u32, u32)>),
    Timestamp,
    TimestampWithTimeZone,
    Time,
    Date,
    Interval(IntervalSpec),
    Bit(Option<u32>),
    VarBit(u32),
    Boolean,
    Money,
    Json,
    JsonBinary,
    Uuid,
    /// A type this crate has no variant for, named rather than spelled: the
    /// payload is a [`TypeName`], so it renders through the same
    /// quoted-or-safe-bare part policy every other type name does and a
    /// hostile catalogue name becomes a name PostgreSQL refuses, never SQL.
    Named(TypeName),
    Enum {
        name: Name,
        schema: Option<Name>,
        variants: Vec<Name>,
    },
    Array(Arc<ColumnType>),
    Vector(Option<u32>),
    Cidr,
    Inet,
    MacAddr,
    LTree,
}

/// Length for var-char; default to 255
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StringLen {
    /// String size
    N(u32),
    Max,
    #[default]
    None,
}

// [spec:pgorm:def:sql.types.column-type+7]
impl PartialEq for ColumnType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Char(l0), Self::Char(r0)) => l0 == r0,
            (Self::String(l0), Self::String(r0)) => l0 == r0,
            (Self::Decimal(l0), Self::Decimal(r0)) => l0 == r0,
            (Self::Interval(l0), Self::Interval(r0)) => l0 == r0,
            (Self::Bit(l0), Self::Bit(r0)) => l0 == r0,
            (Self::VarBit(l0), Self::VarBit(r0)) => l0 == r0,
            (Self::Named(l0), Self::Named(r0)) => l0.raw_text() == r0.raw_text(),
            (
                Self::Enum {
                    name: l_name,
                    schema: l_schema,
                    variants: l_variants,
                },
                Self::Enum {
                    name: r_name,
                    schema: r_schema,
                    variants: r_variants,
                },
            ) => {
                l_name.to_string() == r_name.to_string()
                    && l_schema.as_ref().map(|s| s.to_string())
                        == r_schema.as_ref().map(|s| s.to_string())
                    && l_variants
                        .iter()
                        .map(|v| v.to_string())
                        .eq(r_variants.iter().map(|v| v.to_string()))
            }
            (Self::Array(l0), Self::Array(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl ColumnType {
    /// A type named rather than spelled — `citext`, `tenant_a.status`.
    ///
    /// The name is data, not program text: it renders through
    /// [`TypeName`]'s part policy, so a runtime `String` is safe here.
    /// A type *expression* — `numeric(12, 2)` — is grammar rather than a
    /// name and belongs in
    /// [`Expr::cast_as_raw`](crate::Expr::cast_as_raw).
    // [spec:pgorm:req:sql.render.ident-quoting+5]
    pub fn named<T>(ty: T) -> ColumnType
    where
        T: Into<String>,
    {
        ColumnType::Named(TypeName::new(Name::runtime(ty)))
    }

    pub fn string(length: Option<u32>) -> ColumnType {
        match length {
            Some(s) => ColumnType::String(StringLen::N(s)),
            None => ColumnType::String(StringLen::None),
        }
    }

    /// The `serial` spelling this type is replaced by when the column carries
    /// [`ColumnSpec::AutoIncrement`], or `None` when Postgres has no serial
    /// form for it.
    // [spec:pgorm:req:sql.ddl.column-def+5]
    pub fn serial_spelling(&self) -> Option<&'static str> {
        match self {
            ColumnType::SmallInteger => Some("smallserial"),
            ColumnType::Integer => Some("serial"),
            ColumnType::BigInteger => Some("bigserial"),
            _ => None,
        }
    }
}

/// All column specification keywords
#[derive(Debug, Clone)]
pub enum ColumnSpec {
    Null,
    NotNull,
    Default(SimpleExpr),
    AutoIncrement,
    UniqueKey,
    PrimaryKey,
    Check(SimpleExpr),
    Generated {
        expr: SimpleExpr,
    },
    /// `GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY` — PostgreSQL's
    /// standard-SQL replacement for the serial family.
    // [spec:pgorm:req:sql.ddl.column-def+5]
    Identity(IdentityGeneration),
    /// Verbatim SQL appended after the column's own clauses.
    RawSuffix(&'static str),
    /// Metadata, not a rendered clause: PostgreSQL has no column-comment
    /// clause of `CREATE TABLE`, so the renderer skips this spec and the text
    /// is carried for
    /// [`TableCreateStatement::comments`](crate::TableCreateStatement::comments)
    /// — and for consumers reading it back off `get_column_spec` — to turn
    /// into the `COMMENT ON COLUMN` statement it really is.
    // [spec:pgorm:req:sql.ddl.comment+4]
    Comment(String),
}

/// Which of PostgreSQL's two identity forms supplies a column's values.
///
/// The distinction is only about what happens when a statement *does* name the
/// column: under [`Always`](IdentityGeneration::Always) the server rejects the
/// supplied value, under [`ByDefault`](IdentityGeneration::ByDefault) it takes
/// it and the sequence fills in only where a statement stays silent. Both draw
/// from the same implicit sequence otherwise.
///
/// There is no third state and no flag: an identity column is generated one way
/// or the other, never both and never neither, so the choice is a closed pair
/// rather than a `bool` that reads backwards at the call site
/// (`[dec:pgorm:invalid-states-unrepresentable]`).
// [spec:pgorm:req:sql.ddl.column-def+5]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityGeneration {
    /// `GENERATED ALWAYS AS IDENTITY`.
    Always,
    /// `GENERATED BY DEFAULT AS IDENTITY`.
    ByDefault,
}

impl IdentityGeneration {
    /// The keyword PostgreSQL spells this form with — `ALWAYS` or
    /// `BY DEFAULT`, the words `information_schema.columns.identity_generation`
    /// reports it under.
    // [spec:pgorm:req:sql.ddl.column-def+5]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Always => "ALWAYS",
            Self::ByDefault => "BY DEFAULT",
        }
    }
}

impl ColumnDef {
    /// Construct a table column
    pub fn new<T>(name: T) -> Self
    where
        T: IntoName,
    {
        Self {
            table: None,
            name: name.into_name(),
            types: None,
            spec: Vec::new(),
        }
    }

    /// Construct a table column with column type
    pub fn new_with_type<T>(name: T, types: ColumnType) -> Self
    where
        T: IntoName,
    {
        Self {
            table: None,
            name: name.into_name(),
            types: Some(types),
            spec: Vec::new(),
        }
    }

    /// Set column not null
    pub fn not_null(&mut self) -> &mut Self {
        self.spec.push(ColumnSpec::NotNull);
        self
    }

    /// Set column null
    pub fn null(&mut self) -> &mut Self {
        self.spec.push(ColumnSpec::Null);
        self
    }

    /// Set default expression of a column
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::create(Char::Table)
    ///     .col(ColumnDef::new(Char::FontId).integer().default(12i32))
    ///     .col(
    ///         ColumnDef::new(Char::CreatedAt)
    ///             .timestamp()
    ///             .default(Expr::current_timestamp())
    ///             .not_null(),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"CREATE TABLE "character" ("#,
    ///         r#""font_id" integer DEFAULT 12,"#,
    ///         r#""created_at" timestamp DEFAULT CURRENT_TIMESTAMP NOT NULL"#,
    ///         r#")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn default<T>(&mut self, value: T) -> &mut Self
    where
        T: Into<SimpleExpr>,
    {
        self.spec.push(ColumnSpec::Default(value.into()));
        self
    }

    /// Draw the column's values from the serial family — `serial`,
    /// `smallserial`, `bigserial` — in place of its declared type.
    ///
    /// This is the legacy spelling. PostgreSQL has recommended
    /// [`identity`](Self::identity) since 10: an identity column is standard
    /// SQL, owns its sequence outright rather than merely defaulting to one,
    /// and cannot be overwritten by accident. `auto_increment` stays because it
    /// is what the entity derive's `auto_increment` attribute means today;
    /// prefer `identity` in new schemas.
    // [spec:pgorm:req:sql.ddl.column-def+5]
    pub fn auto_increment(&mut self) -> &mut Self {
        self.spec.push(ColumnSpec::AutoIncrement);
        self
    }

    /// Generate the column's values `ALWAYS` — `GENERATED ALWAYS AS IDENTITY`.
    ///
    /// The server refuses an explicit value for the column, which is what makes
    /// this the safe default: a row cannot silently claim a key the sequence
    /// does not know it handed out. `OVERRIDING SYSTEM VALUE` is the deliberate
    /// override, and this crate has no spelling for it
    /// (`[spec:pgorm:req:sql.ddl.column-def+5]`).
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// assert_eq!(
    ///     Table::create(Glyph::Table)
    ///         .col(ColumnDef::new(Glyph::Id).integer().identity().primary_key())
    ///         .to_string(),
    ///     r#"CREATE TABLE "glyph" ( "id" integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY )"#,
    /// );
    /// ```
    ///
    /// The sequence's own options — `START WITH`, `INCREMENT BY`, `CACHE` —
    /// have no spelling here. They are a sequence definition wearing a column
    /// clause, and the column vocabulary is the wrong place to grow one;
    /// [`raw_suffix`](Self::raw_suffix) takes them verbatim until there is a
    /// typed sequence builder to take them properly.
    // [spec:pgorm:req:sql.ddl.column-def+5]
    pub fn identity(&mut self) -> &mut Self {
        self.spec
            .push(ColumnSpec::Identity(IdentityGeneration::Always));
        self
    }

    /// Generate the column's values `BY DEFAULT` —
    /// `GENERATED BY DEFAULT AS IDENTITY`.
    ///
    /// The sequence supplies a value only where the statement gives none, so an
    /// explicit value is accepted — the behaviour a `serial` column has, with
    /// an owned sequence instead of a `DEFAULT nextval(..)`. Reach for it where
    /// rows arrive carrying keys already, such as a restore or a migration;
    /// otherwise prefer [`identity`](Self::identity).
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// assert_eq!(
    ///     Table::create(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Glyph::Id)
    ///                 .big_integer()
    ///                 .identity_by_default()
    ///                 .not_null()
    ///         )
    ///         .to_string(),
    ///     r#"CREATE TABLE "glyph" ( "id" bigint GENERATED BY DEFAULT AS IDENTITY NOT NULL )"#,
    /// );
    /// ```
    // [spec:pgorm:req:sql.ddl.column-def+5]
    pub fn identity_by_default(&mut self) -> &mut Self {
        self.spec
            .push(ColumnSpec::Identity(IdentityGeneration::ByDefault));
        self
    }

    /// Set column unique constraint
    pub fn unique_key(&mut self) -> &mut Self {
        self.spec.push(ColumnSpec::UniqueKey);
        self
    }

    /// Set column as primary key
    pub fn primary_key(&mut self) -> &mut Self {
        self.spec.push(ColumnSpec::PrimaryKey);
        self
    }

    /// Set column type as char with an explicit length
    pub fn char_len(&mut self, length: u32) -> &mut Self {
        self.types = Some(ColumnType::Char(Some(length)));
        self
    }

    /// Set column type as char
    pub fn char(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Char(None));
        self
    }

    /// Set column type as string with an explicit length
    pub fn string_len(&mut self, length: u32) -> &mut Self {
        self.types = Some(ColumnType::String(StringLen::N(length)));
        self
    }

    /// Set column type as string
    pub fn string(&mut self) -> &mut Self {
        self.types = Some(ColumnType::String(Default::default()));
        self
    }

    /// Set column type as text
    pub fn text(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Text);
        self
    }

    /// Set column type as small_integer
    pub fn small_integer(&mut self) -> &mut Self {
        self.types = Some(ColumnType::SmallInteger);
        self
    }

    /// Set column type as integer
    pub fn integer(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Integer);
        self
    }

    /// Set column type as big_integer
    pub fn big_integer(&mut self) -> &mut Self {
        self.types = Some(ColumnType::BigInteger);
        self
    }

    /// Set column type as float
    pub fn float(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Float);
        self
    }

    /// Set column type as double
    pub fn double(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Double);
        self
    }

    /// Set column type as decimal with an explicit precision and scale
    pub fn decimal_len(&mut self, precision: u32, scale: u32) -> &mut Self {
        self.types = Some(ColumnType::Decimal(Some((precision, scale))));
        self
    }

    /// Set column type as decimal
    pub fn decimal(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Decimal(None));
        self
    }

    /// Set column type as interval type. Postgres only
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    /// assert_eq!(
    ///     Table::create(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Name::runtime("I1"))
    ///                 .interval(IntervalSpec::Any(None))
    ///                 .not_null()
    ///         )
    ///         .col(
    ///             ColumnDef::new(Name::runtime("I2"))
    ///                 .interval(IntervalSpec::Fields(PgInterval::YearToMonth))
    ///                 .not_null()
    ///         )
    ///         .col(
    ///             ColumnDef::new(Name::runtime("I3"))
    ///                 .interval(IntervalSpec::Any(Some(IntervalPrecision::P4)))
    ///                 .not_null()
    ///         )
    ///         .col(
    ///             ColumnDef::new(Name::runtime("I4"))
    ///                 .interval(IntervalSpec::Fields(PgInterval::HourToSecond(Some(
    ///                     IntervalPrecision::P3
    ///                 ))))
    ///                 .not_null()
    ///         )
    ///         .to_string(),
    ///     [
    ///         r#"CREATE TABLE "glyph" ("#,
    ///         r#""I1" interval NOT NULL,"#,
    ///         r#""I2" interval YEAR TO MONTH NOT NULL,"#,
    ///         r#""I3" interval(4) NOT NULL,"#,
    ///         r#""I4" interval HOUR TO SECOND(3) NOT NULL"#,
    ///         r#")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    // [spec:pgorm:def:sql.types.column-type+7]
    pub fn interval(&mut self, spec: IntervalSpec) -> &mut Self {
        self.types = Some(ColumnType::Interval(spec));
        self
    }

    pub fn vector(&mut self, size: Option<u32>) -> &mut Self {
        self.types = Some(ColumnType::Vector(size));
        self
    }

    /// Set column type as timestamp
    pub fn timestamp(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Timestamp);
        self
    }

    /// Set column type as timestamp with time zone. Postgres only
    pub fn timestamp_with_time_zone(&mut self) -> &mut Self {
        self.types = Some(ColumnType::TimestampWithTimeZone);
        self
    }

    /// Set column type as time
    pub fn time(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Time);
        self
    }

    /// Set column type as date
    pub fn date(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Date);
        self
    }

    /// Set column type as bit with variable length
    pub fn bit(&mut self, length: Option<u32>) -> &mut Self {
        self.types = Some(ColumnType::Bit(length));
        self
    }

    /// Set column type as varbit with variable length
    pub fn varbit(&mut self, length: u32) -> &mut Self {
        self.types = Some(ColumnType::VarBit(length));
        self
    }

    /// Set column type as bytea
    pub fn bytea(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Bytea);
        self
    }

    /// Set column type as boolean
    pub fn boolean(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Boolean);
        self
    }

    /// Set column type as money
    pub fn money(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Money);
        self
    }

    /// Set column type as json.
    pub fn json(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Json);
        self
    }

    /// Set column type as json binary.
    pub fn json_binary(&mut self) -> &mut Self {
        self.types = Some(ColumnType::JsonBinary);
        self
    }

    /// Set column type as uuid
    pub fn uuid(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Uuid);
        self
    }

    /// Use a type this vocabulary has no spelling for, named rather than
    /// spelled.
    ///
    /// Takes anything a [`TypeName`] is built from — a bare name, or a
    /// `TypeName` carrying a schema qualifier or an array suffix — and every
    /// part renders quoted-or-safe-bare, never as SQL.
    // [spec:pgorm:req:sql.render.ident-quoting+5]
    pub fn named<T>(&mut self, name: T) -> &mut Self
    where
        T: Into<TypeName>,
    {
        self.types = Some(ColumnType::Named(name.into()));
        self
    }

    /// Set column type as enum.
    pub fn enumeration<N, S, V>(&mut self, name: N, variants: V) -> &mut Self
    where
        N: IntoName,
        S: IntoName,
        V: IntoIterator<Item = S>,
    {
        self.types = Some(ColumnType::Enum {
            name: name.into_name(),
            schema: None,
            variants: variants.into_iter().map(IntoName::into_name).collect(),
        });
        self
    }

    /// Set column type as an array with a specified element type.
    /// This is only supported on Postgres.
    pub fn array(&mut self, elem_type: ColumnType) -> &mut Self {
        self.types = Some(ColumnType::Array(Arc::new(elem_type)));
        self
    }

    /// Set columnt type as cidr.
    /// This is only supported on Postgres.
    pub fn cidr(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Cidr);
        self
    }

    /// Set columnt type as inet.
    /// This is only supported on Postgres.
    pub fn inet(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Inet);
        self
    }

    /// Set columnt type as macaddr.
    /// This is only supported on Postgres.
    pub fn mac_address(&mut self) -> &mut Self {
        self.types = Some(ColumnType::MacAddr);
        self
    }

    /// Set column type as `ltree`
    /// This is only supported on Postgres.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    /// assert_eq!(
    ///     Table::create(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Glyph::Id)
    ///                 .integer()
    ///                 .not_null()
    ///                 .auto_increment()
    ///                 .primary_key()
    ///         )
    ///         .col(ColumnDef::new(Glyph::Tokens).ltree())
    ///         .to_string(),
    ///     [
    ///         r#"CREATE TABLE "glyph" ("#,
    ///         r#""id" serial NOT NULL PRIMARY KEY,"#,
    ///         r#""tokens" ltree"#,
    ///         r#")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn ltree(&mut self) -> &mut Self {
        self.types = Some(ColumnType::LTree);
        self
    }

    /// Set constraints as SimpleExpr
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    /// assert_eq!(
    ///     Table::create(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Glyph::Id)
    ///                 .integer()
    ///                 .not_null()
    ///                 .check(Expr::col(Glyph::Id).gt(10))
    ///         )
    ///         .to_string(),
    ///     r#"CREATE TABLE "glyph" ( "id" integer NOT NULL CHECK ("id" > 10) )"#,
    /// );
    /// ```
    pub fn check<T>(&mut self, value: T) -> &mut Self
    where
        T: Into<SimpleExpr>,
    {
        self.spec.push(ColumnSpec::Check(value.into()));
        self
    }

    /// Sets the column as generated from an expression, and stored.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// assert_eq!(
    ///     Table::create(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Glyph::Aspect)
    ///                 .integer()
    ///                 .generated(Expr::col(Glyph::Id).mul(2))
    ///         )
    ///         .to_string(),
    ///     r#"CREATE TABLE "glyph" ( "aspect" integer GENERATED ALWAYS AS ("id" * 2) STORED )"#,
    /// );
    /// ```
    ///
    /// There is no virtual spelling to ask for. PostgreSQL has accepted
    /// `VIRTUAL` only since 18 and rejects it as a syntax error on every
    /// earlier release, and the builder cannot know which one it is writing
    /// for; a column that cannot be stored is spelled as a view or a trigger
    /// instead.
    ///
    /// ```compile_fail,E0061
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// ColumnDef::new(Glyph::Aspect).integer().generated(Expr::val(1), false);
    /// ```
    // [spec:pgorm:req:sql.ddl.column-def+5]
    pub fn generated<T>(&mut self, expr: T) -> &mut Self
    where
        T: Into<SimpleExpr>,
    {
        self.spec.push(ColumnSpec::Generated { expr: expr.into() });
        self
    }

    /// Append verbatim SQL after this column's own clauses — the escape hatch
    /// for column specs this vocabulary has no spelling for.
    ///
    /// The `&'static str` bound is the contract, the same one
    /// [`Expr::raw`](crate::Expr::raw) carries: only program text can be
    /// appended, never a runtime string a value could have reached.
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    /// let table = Table::create(Char::Table)
    ///     .col(
    ///         ColumnDef::new(Char::Id)
    ///             .uuid()
    ///             .raw_suffix("DEFAULT gen_random_uuid()")
    ///             .primary_key()
    ///             .not_null(),
    ///     )
    ///     .col(
    ///         ColumnDef::new(Char::CreatedAt)
    ///             .timestamp_with_time_zone()
    ///             .raw_suffix("DEFAULT NOW()")
    ///             .not_null(),
    ///     )
    ///     .to_owned();
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"CREATE TABLE "character" ("#,
    ///         r#""id" uuid DEFAULT gen_random_uuid() PRIMARY KEY NOT NULL,"#,
    ///         r#""created_at" timestamp with time zone DEFAULT NOW() NOT NULL"#,
    ///         r#")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn raw_suffix(&mut self, sql: &'static str) -> &mut Self {
        self.spec.push(ColumnSpec::RawSuffix(sql));
        self
    }

    /// Record a comment for this column.
    ///
    /// The spec is skipped when the column renders: on Postgres a column
    /// comment is a `COMMENT ON` statement of its own. Render it off the
    /// create statement this column goes into, with
    /// [`TableCreateStatement::comments`](crate::TableCreateStatement::comments),
    /// or build it directly with [`Comment::on_column`](crate::Comment::on_column).
    // [spec:pgorm:req:sql.ddl.comment+4]
    pub fn comment<T>(&mut self, string: T) -> &mut Self
    where
        T: Into<String>,
    {
        self.spec.push(ColumnSpec::Comment(string.into()));
        self
    }

    pub fn get_column_name(&self) -> String {
        self.name.to_string()
    }

    pub fn get_column_type(&self) -> Option<&ColumnType> {
        self.types.as_ref()
    }

    pub fn get_column_spec(&self) -> &Vec<ColumnSpec> {
        self.spec.as_ref()
    }

    pub fn take(&mut self) -> Self {
        Self {
            table: self.table.take(),
            name: self.name.clone(),
            types: self.types.take(),
            spec: std::mem::take(&mut self.spec),
        }
    }
}

impl IntoColumnDef for &mut ColumnDef {
    fn into_column_def(self) -> ColumnDef {
        self.take()
    }
}

impl IntoColumnDef for ColumnDef {
    fn into_column_def(self) -> ColumnDef {
        self
    }
}
