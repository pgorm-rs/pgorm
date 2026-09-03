use crate::{expr::*, types::*};

/// Specification of a table column
// [spec:pgorm:req:sql.ddl.column-def+3]
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub(crate) table: Option<TableName>,
    pub(crate) name: DynIden,
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
// [spec:pgorm:def:sql.types.column-type+3]
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
    Custom(DynIden),
    Enum {
        name: DynIden,
        variants: Vec<DynIden>,
    },
    Array(RcOrArc<ColumnType>),
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

// [spec:pgorm:def:sql.types.column-type+3]
impl PartialEq for ColumnType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Char(l0), Self::Char(r0)) => l0 == r0,
            (Self::String(l0), Self::String(r0)) => l0 == r0,
            (Self::Decimal(l0), Self::Decimal(r0)) => l0 == r0,
            (Self::Interval(l0), Self::Interval(r0)) => l0 == r0,
            (Self::Bit(l0), Self::Bit(r0)) => l0 == r0,
            (Self::VarBit(l0), Self::VarBit(r0)) => l0 == r0,
            (Self::Custom(l0), Self::Custom(r0)) => l0.to_string() == r0.to_string(),
            (
                Self::Enum {
                    name: l_name,
                    variants: l_variants,
                },
                Self::Enum {
                    name: r_name,
                    variants: r_variants,
                },
            ) => {
                l_name.to_string() == r_name.to_string()
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
    pub fn custom<T>(ty: T) -> ColumnType
    where
        T: Into<String>,
    {
        ColumnType::Custom(Alias::new(ty).into_iden())
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
    // [spec:pgorm:req:sql.ddl.column-def+3]
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
    Generated { expr: SimpleExpr, stored: bool },
    Extra(String),
    Comment(String),
}

/// The `[fields] [(p)]` tail of a PostgreSQL `interval` type.
///
/// PostgreSQL takes a precision only where the trailing field is `SECOND`, so
/// the precision sits on the second-bearing field spellings and on the
/// unqualified form, and `interval HOUR(3)` has no spelling here.
// [spec:pgorm:def:sql.types.column-type+3]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum IntervalSpec {
    /// `interval`, or `interval(p)` — every field, with a fractional-seconds
    /// precision when one is given.
    Any(Option<IntervalPrecision>),
    /// `interval <fields>` — one of the field-qualified forms.
    Fields(PgInterval),
}

/// Fractional-seconds precision of an interval type.
///
/// PostgreSQL accepts 0 through 6; a wider precision has no spelling.
// [spec:pgorm:def:sql.types.column-type+3]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IntervalPrecision {
    P0,
    P1,
    P2,
    P3,
    P4,
    P5,
    P6,
}

impl IntervalPrecision {
    /// The precision `digits` names, or `None` where PostgreSQL has none.
    pub const fn new(digits: u8) -> Option<Self> {
        match digits {
            0 => Some(Self::P0),
            1 => Some(Self::P1),
            2 => Some(Self::P2),
            3 => Some(Self::P3),
            4 => Some(Self::P4),
            5 => Some(Self::P5),
            6 => Some(Self::P6),
            _ => None,
        }
    }

    /// The digit count this precision spells.
    pub const fn digits(self) -> u8 {
        match self {
            Self::P0 => 0,
            Self::P1 => 1,
            Self::P2 => 2,
            Self::P3 => 3,
            Self::P4 => 4,
            Self::P5 => 5,
            Self::P6 => 6,
        }
    }
}

impl std::fmt::Display for IntervalPrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.digits())
    }
}

/// All interval field qualifiers; the second-bearing ones carry the precision
/// PostgreSQL allows only there.
// [spec:pgorm:def:sql.types.column-type+3]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PgInterval {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second(Option<IntervalPrecision>),
    YearToMonth,
    DayToHour,
    DayToMinute,
    DayToSecond(Option<IntervalPrecision>),
    HourToMinute,
    HourToSecond(Option<IntervalPrecision>),
    MinuteToSecond(Option<IntervalPrecision>),
}

impl ColumnDef {
    /// Construct a table column
    pub fn new<T>(name: T) -> Self
    where
        T: IntoIden,
    {
        Self {
            table: None,
            name: name.into_iden(),
            types: None,
            spec: Vec::new(),
        }
    }

    /// Construct a table column with column type
    pub fn new_with_type<T>(name: T, types: ColumnType) -> Self
    where
        T: IntoIden,
    {
        Self {
            table: None,
            name: name.into_iden(),
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
    /// let table = Table::create()
    ///     .table(Char::Table)
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
    ///     table.to_string(QueryBuilder),
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

    /// Set column auto increment
    pub fn auto_increment(&mut self) -> &mut Self {
        self.spec.push(ColumnSpec::AutoIncrement);
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

    /// Set column type as char with custom length
    pub fn char_len(&mut self, length: u32) -> &mut Self {
        self.types = Some(ColumnType::Char(Some(length)));
        self
    }

    /// Set column type as char
    pub fn char(&mut self) -> &mut Self {
        self.types = Some(ColumnType::Char(None));
        self
    }

    /// Set column type as string with custom length
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

    /// Set column type as decimal with custom precision and scale
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
    ///     Table::create()
    ///         .table(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Alias::new("I1"))
    ///                 .interval(IntervalSpec::Any(None))
    ///                 .not_null()
    ///         )
    ///         .col(
    ///             ColumnDef::new(Alias::new("I2"))
    ///                 .interval(IntervalSpec::Fields(PgInterval::YearToMonth))
    ///                 .not_null()
    ///         )
    ///         .col(
    ///             ColumnDef::new(Alias::new("I3"))
    ///                 .interval(IntervalSpec::Any(Some(IntervalPrecision::P4)))
    ///                 .not_null()
    ///         )
    ///         .col(
    ///             ColumnDef::new(Alias::new("I4"))
    ///                 .interval(IntervalSpec::Fields(PgInterval::HourToSecond(Some(
    ///                     IntervalPrecision::P3
    ///                 ))))
    ///                 .not_null()
    ///         )
    ///         .to_string(QueryBuilder),
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
    // [spec:pgorm:def:sql.types.column-type+3]
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

    /// Use a custom type on this column.
    pub fn custom<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoIden,
    {
        self.types = Some(ColumnType::Custom(name.into_iden()));
        self
    }

    /// Set column type as enum.
    pub fn enumeration<N, S, V>(&mut self, name: N, variants: V) -> &mut Self
    where
        N: IntoIden,
        S: IntoIden,
        V: IntoIterator<Item = S>,
    {
        self.types = Some(ColumnType::Enum {
            name: name.into_iden(),
            variants: variants.into_iter().map(IntoIden::into_iden).collect(),
        });
        self
    }

    /// Set column type as an array with a specified element type.
    /// This is only supported on Postgres.
    pub fn array(&mut self, elem_type: ColumnType) -> &mut Self {
        self.types = Some(ColumnType::Array(RcOrArc::new(elem_type)));
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
    ///     Table::create()
    ///         .table(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Glyph::Id)
    ///                 .integer()
    ///                 .not_null()
    ///                 .auto_increment()
    ///                 .primary_key()
    ///         )
    ///         .col(ColumnDef::new(Glyph::Tokens).ltree())
    ///         .to_string(QueryBuilder),
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
    ///     Table::create()
    ///         .table(Glyph::Table)
    ///         .col(
    ///             ColumnDef::new(Glyph::Id)
    ///                 .integer()
    ///                 .not_null()
    ///                 .check(Expr::col(Glyph::Id).gt(10))
    ///         )
    ///         .to_string(QueryBuilder),
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

    /// Sets the column as generated with SimpleExpr
    pub fn generated<T>(&mut self, expr: T, stored: bool) -> &mut Self
    where
        T: Into<SimpleExpr>,
    {
        self.spec.push(ColumnSpec::Generated {
            expr: expr.into(),
            stored,
        });
        self
    }

    /// Some extra options in custom string
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    /// let table = Table::create()
    ///     .table(Char::Table)
    ///     .col(
    ///         ColumnDef::new(Char::Id)
    ///             .uuid()
    ///             .extra("DEFAULT gen_random_uuid()")
    ///             .primary_key()
    ///             .not_null(),
    ///     )
    ///     .col(
    ///         ColumnDef::new(Char::CreatedAt)
    ///             .timestamp_with_time_zone()
    ///             .extra("DEFAULT NOW()")
    ///             .not_null(),
    ///     )
    ///     .to_owned();
    /// assert_eq!(
    ///     table.to_string(QueryBuilder),
    ///     [
    ///         r#"CREATE TABLE "character" ("#,
    ///         r#""id" uuid DEFAULT gen_random_uuid() PRIMARY KEY NOT NULL,"#,
    ///         r#""created_at" timestamp with time zone DEFAULT NOW() NOT NULL"#,
    ///         r#")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn extra<T>(&mut self, string: T) -> &mut Self
    where
        T: Into<String>,
    {
        self.spec.push(ColumnSpec::Extra(string.into()));
        self
    }

    /// Record a comment for this column.
    ///
    /// The spec is skipped when the column renders: on Postgres a column
    /// comment is a `COMMENT ON` statement of its own.
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
