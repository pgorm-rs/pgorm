//! Base types used throughout pgorm-query.

use crate::{FunctionCall, ValueTuple, Values, expr::*, query::*};
use std::{fmt, mem, ops};

pub use std::sync::Arc as RcOrArc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote(pub(crate) u8, pub(crate) u8);

// [spec:pgorm:def:sql.types+1]
macro_rules! iden_trait {
    ($($bounds:ident),*) => {
        /// Identifier
        pub trait Iden where $(Self: $bounds),* {
            // [spec:pgorm:req:sql.render.ident-quoting] (wrap in quote pair; embedded right-quote doubled)
            fn prepare(&self, s: &mut dyn fmt::Write, q: Quote) {
                write!(s, "{}{}{}", q.left(), self.quoted(q), q.right()).unwrap();
            }

            fn quoted(&self, q: Quote) -> String {
                let byte = [q.1];
                let qq: &str = std::str::from_utf8(&byte).unwrap();
                self.to_string().replace(qq, qq.repeat(2).as_str())
            }

            fn to_string(&self) -> String {
                let mut s = String::new();
                self.unquoted(&mut s);
                s
            }

            fn unquoted(&self, s: &mut dyn fmt::Write);
        }

        /// Identifier
        pub trait IdenStatic: Iden + Copy + 'static {
            fn as_str(&self) -> &'static str;
        }
    };
}

iden_trait!(Send, Sync);

pub type DynIden = SeaRc<dyn Iden>;

#[derive(Debug)]
#[repr(transparent)]
pub struct SeaRc<I>(pub(crate) RcOrArc<I>)
where
    I: ?Sized;

impl ops::Deref for SeaRc<dyn Iden> {
    type Target = dyn Iden;

    fn deref(&self) -> &Self::Target {
        ops::Deref::deref(&self.0)
    }
}

impl Clone for SeaRc<dyn Iden> {
    fn clone(&self) -> SeaRc<dyn Iden> {
        SeaRc(RcOrArc::clone(&self.0))
    }
}

// [spec:pgorm:def:sql.types+1]
impl PartialEq for SeaRc<dyn Iden> {
    fn eq(&self, other: &Self) -> bool {
        let (self_vtable, other_vtable) = unsafe {
            let (_, self_vtable) = mem::transmute::<&dyn Iden, (usize, usize)>(&*self.0);
            let (_, other_vtable) = mem::transmute::<&dyn Iden, (usize, usize)>(&*other.0);
            (self_vtable, other_vtable)
        };
        self_vtable == other_vtable && self.to_string() == other.to_string()
    }
}

impl SeaRc<dyn Iden> {
    pub fn new<I>(i: I) -> SeaRc<dyn Iden>
    where
        I: Iden + 'static,
    {
        SeaRc(RcOrArc::new(i))
    }
}

pub trait IntoIden {
    fn into_iden(self) -> DynIden;
}

pub trait IdenList {
    type IntoIter: Iterator<Item = DynIden>;

    fn into_iter(self) -> Self::IntoIter;
}

impl fmt::Debug for dyn Iden {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.unquoted(formatter);
        Ok(())
    }
}

/// Column references
// [spec:pgorm:def:sql.types.column-ref]
// [spec:pgorm:def:sql.ast.keywords+1]
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnRef {
    Column(DynIden),
    TableColumn(DynIden, DynIden),
    SchemaTableColumn(DynIden, DynIden, DynIden),
    Asterisk,
    TableAsterisk(DynIden),
}

// [spec:pgorm:def:sql.types.column-ref]
pub trait IntoColumnRef {
    fn into_column_ref(self) -> ColumnRef;
}

/// The name of a table, as a DDL statement targets it.
///
/// Every form denotes a table that exists in the catalogue, so this is what
/// `CREATE`/`ALTER`/`DROP`/`RENAME`/`TRUNCATE`, index and foreign-key targets
/// and comment targets accept. The query-position forms that name no table —
/// a subquery, a values list, a function call — live on [`FromItem`] and
/// cannot reach a DDL position.
///
/// A name reads in either position:
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let name = (Alias::new("public"), Glyph::Table).into_table_name();
/// assert_eq!(
///     Table::truncate().table(name.clone()).to_string(QueryBuilder),
///     r#"TRUNCATE TABLE "public"."glyph""#
/// );
/// assert_eq!(
///     Query::select().column(Asterisk).from(name).to_string(QueryBuilder),
///     r#"SELECT * FROM "public"."glyph""#
/// );
/// ```
///
/// A subquery is a [`FromItem`] and not a name, so it does not typecheck as a
/// DDL target:
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// let sub = FromItem::SubQuery(
///     Query::select().column(Glyph::Id).from(Glyph::Table).take(),
///     Alias::new("q").into_iden(),
/// );
/// Table::truncate().table(sub);
/// ```
///
/// Nor does an aliased table, whose alias only means anything in a query:
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// Table::truncate().table(Glyph::Table.into_from_item().alias(Alias::new("g")));
/// ```
// [spec:pgorm:def:sql.types.table-ref+1]
// [spec:pgorm:sem:sql.ddl.panics+1/test]    the DDL-position panics are gone because the shapes
// that reached them no longer typecheck
#[derive(Debug, Clone, PartialEq)]
pub enum TableName {
    /// Table identifier without any schema prefix
    Table(DynIden),
    /// Table identifier with schema prefix
    SchemaTable(DynIden, DynIden),
}

/// Conversion into the [`TableName`] a DDL statement targets.
// [spec:pgorm:def:sql.types.table-ref+1]
pub trait IntoTableName {
    /// Consume `self` and produce a [`TableName`]
    fn into_table_name(self) -> TableName;
}

/// An entry in a `FROM` clause, a join, or the target of a DML statement.
///
/// A named table carries its alias beside it rather than in the variant, so
/// aliasing is orthogonal to how the name is qualified; the value-producing
/// forms carry the alias Postgres requires of them.
// [spec:pgorm:def:sql.types.table-ref+1]
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum FromItem {
    /// A named table with an optional alias
    Table(TableName, Option<DynIden>),
    /// Subquery with alias
    SubQuery(SelectStatement, DynIden),
    /// Values list with alias
    ValuesList(Vec<ValueTuple>, DynIden),
    /// Function call with alias
    FunctionCall(FunctionCall, DynIden),
}

/// Conversion into a [`FromItem`].
// [spec:pgorm:def:sql.types.table-ref+1]
pub trait IntoFromItem {
    /// Consume `self` and produce a [`FromItem`]
    fn into_from_item(self) -> FromItem;
}

/// Unary operator
// [spec:pgorm:def:sql.types.opers]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOper {
    Not,
}

/// Binary operator
// [spec:pgorm:def:sql.types.opers]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOper {
    And,
    Or,
    Like,
    NotLike,
    Is,
    IsNot,
    In,
    NotIn,
    Between,
    NotBetween,
    Equal,
    NotEqual,
    SmallerThan,
    GreaterThan,
    SmallerThanOrEqual,
    GreaterThanOrEqual,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LShift,
    RShift,
    As,
    Escape,
    ILike,
    NotILike,
    Matches,
    Contains,
    Contained,
    Concatenate,
    Overlap,
    Similarity,
    WordSimilarity,
    StrictWordSimilarity,
    SimilarityDistance,
    WordSimilarityDistance,
    StrictWordSimilarityDistance,
    /// `->`. Retrieves JSON field as JSON value.
    GetJsonField,
    /// `->>`. Retrieves JSON field and casts it to an appropriate SQL type.
    CastJsonField,
    /// `~` Regex operator.
    Regex,
    /// `~*`. Regex operator with case insensitive matching.
    RegexCaseInsensitive,
    EuclideanDistance,
    NegativeInnerProduct,
    CosineDistance,
    Custom(&'static str),
}

/// Join types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Join,
    CrossJoin,
    InnerJoin,
    LeftJoin,
    RightJoin,
    FullOuterJoin,
}

/// Nulls order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullOrdering {
    First,
    Last,
}

/// Order expression
#[derive(Debug, Clone, PartialEq)]
pub struct OrderExpr {
    pub(crate) expr: SimpleExpr,
    pub(crate) order: Order,
    pub(crate) nulls: Option<NullOrdering>,
}

/// Join on types
#[derive(Debug, Clone, PartialEq)]
pub enum JoinOn {
    Condition(Box<ConditionHolder>),
    Columns(Vec<SimpleExpr>),
}

/// Ordering options
// [spec:pgorm:req:sql.ast.order]
#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Asc,
    Desc,
    Field(Values),
}

/// Helper for create name alias
#[derive(Debug, Clone)]
pub struct Alias(String);

/// Null Alias
#[derive(Default, Debug, Copy, Clone)]
pub struct NullAlias;

/// Asterisk ("*")
///
/// Express the asterisk without table prefix.
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::select()
///     .column(Asterisk)
///     .from(Char::Table)
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(QueryBuilder),
///     r#"SELECT * FROM "character""#
/// );
/// ```
///
/// Express the asterisk with table prefix.
///
/// Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::select()
///     .column((Char::Table, Asterisk))
///     .from(Char::Table)
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(QueryBuilder),
///     r#"SELECT "character".* FROM "character""#
/// );
/// ```
#[derive(Default, Debug, Clone, Copy)]
pub struct Asterisk;

/// SQL Keywords
// [spec:pgorm:def:sql.ast.keywords+1]
#[derive(Debug, Clone, PartialEq)]
pub enum Keyword {
    Null,
    CurrentDate,
    CurrentTime,
    CurrentTimestamp,
    Custom(DynIden),
}

/// Like Expression
#[derive(Debug, Clone)]
pub struct LikeExpr {
    pub(crate) pattern: String,
    pub(crate) escape: Option<char>,
}

pub trait IntoLikeExpr {
    fn into_like_expr(self) -> LikeExpr;
}

/// SubQuery operators
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SubQueryOper {
    Exists,
    Any,
    Some,
    All,
}

// Impl begins

impl Quote {
    pub fn new(c: u8) -> Self {
        Self(c, c)
    }

    pub fn left(&self) -> char {
        char::from(self.0)
    }

    pub fn right(&self) -> char {
        char::from(self.1)
    }
}

impl From<char> for Quote {
    fn from(c: char) -> Self {
        (c as u8).into()
    }
}

impl From<(char, char)> for Quote {
    fn from((l, r): (char, char)) -> Self {
        (l as u8, r as u8).into()
    }
}

impl From<u8> for Quote {
    fn from(u8: u8) -> Self {
        Quote::new(u8)
    }
}

impl From<(u8, u8)> for Quote {
    fn from((l, r): (u8, u8)) -> Self {
        Quote(l, r)
    }
}

impl<T: 'static> IntoIden for T
where
    T: Iden,
{
    fn into_iden(self) -> DynIden {
        SeaRc::new(self)
    }
}

impl IntoIden for DynIden {
    fn into_iden(self) -> DynIden {
        self
    }
}

// [spec:pgorm:def:sql.types+1]
impl IntoIden for &str {
    fn into_iden(self) -> DynIden {
        SeaRc::new(Alias::new(self))
    }
}

// [spec:pgorm:def:sql.types+1]
impl IntoIden for String {
    fn into_iden(self) -> DynIden {
        SeaRc::new(Alias::new(self))
    }
}

impl<I> IdenList for I
where
    I: IntoIden,
{
    type IntoIter = std::iter::Once<DynIden>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self.into_iden())
    }
}

impl<A, B> IdenList for (A, B)
where
    A: IntoIden,
    B: IntoIden,
{
    type IntoIter = std::array::IntoIter<DynIden, 2>;

    fn into_iter(self) -> Self::IntoIter {
        [self.0.into_iden(), self.1.into_iden()].into_iter()
    }
}

impl<A, B, C> IdenList for (A, B, C)
where
    A: IntoIden,
    B: IntoIden,
    C: IntoIden,
{
    type IntoIter = std::array::IntoIter<DynIden, 3>;

    fn into_iter(self) -> Self::IntoIter {
        [self.0.into_iden(), self.1.into_iden(), self.2.into_iden()].into_iter()
    }
}

impl IntoColumnRef for ColumnRef {
    fn into_column_ref(self) -> ColumnRef {
        self
    }
}

impl<T: 'static> IntoColumnRef for T
where
    T: IntoIden,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::Column(self.into_iden())
    }
}

impl IntoColumnRef for Asterisk {
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::Asterisk
    }
}

impl<S: 'static, T: 'static> IntoColumnRef for (S, T)
where
    S: IntoIden,
    T: IntoIden,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::TableColumn(self.0.into_iden(), self.1.into_iden())
    }
}

impl<T: 'static> IntoColumnRef for (T, Asterisk)
where
    T: IntoIden,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::TableAsterisk(self.0.into_iden())
    }
}

impl<S: 'static, T: 'static, U: 'static> IntoColumnRef for (S, T, U)
where
    S: IntoIden,
    T: IntoIden,
    U: IntoIden,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::SchemaTableColumn(self.0.into_iden(), self.1.into_iden(), self.2.into_iden())
    }
}

impl IntoTableName for TableName {
    fn into_table_name(self) -> TableName {
        self
    }
}

impl<T: 'static> IntoTableName for T
where
    T: IntoIden,
{
    fn into_table_name(self) -> TableName {
        TableName::Table(self.into_iden())
    }
}

impl<S: 'static, T: 'static> IntoTableName for (S, T)
where
    S: IntoIden,
    T: IntoIden,
{
    fn into_table_name(self) -> TableName {
        TableName::SchemaTable(self.0.into_iden(), self.1.into_iden())
    }
}

// [spec:pgorm:def:sql.types.table-ref+1]
impl TableName {
    /// The table identifier, without its schema
    pub fn table(&self) -> &DynIden {
        match self {
            Self::Table(table) | Self::SchemaTable(_, table) => table,
        }
    }

    /// The schema identifier, when the name carries one
    pub fn schema(&self) -> Option<&DynIden> {
        match self {
            Self::Table(_) => None,
            Self::SchemaTable(schema, _) => Some(schema),
        }
    }
}

impl IntoFromItem for FromItem {
    fn into_from_item(self) -> FromItem {
        self
    }
}

impl IntoFromItem for TableName {
    fn into_from_item(self) -> FromItem {
        FromItem::Table(self, None)
    }
}

impl<T: 'static> IntoFromItem for T
where
    T: IntoIden,
{
    fn into_from_item(self) -> FromItem {
        FromItem::Table(TableName::Table(self.into_iden()), None)
    }
}

impl<S: 'static, T: 'static> IntoFromItem for (S, T)
where
    S: IntoIden,
    T: IntoIden,
{
    fn into_from_item(self) -> FromItem {
        FromItem::Table(
            TableName::SchemaTable(self.0.into_iden(), self.1.into_iden()),
            None,
        )
    }
}

impl From<TableName> for FromItem {
    fn from(name: TableName) -> Self {
        Self::Table(name, None)
    }
}

// [spec:pgorm:def:sql.types.table-ref+1]
impl FromItem {
    /// Add or replace the current alias
    pub fn alias<A>(self, alias: A) -> Self
    where
        A: IntoIden,
    {
        match self {
            Self::Table(table, _) => Self::Table(table, Some(alias.into_iden())),
            Self::SubQuery(statement, _) => Self::SubQuery(statement, alias.into_iden()),
            Self::ValuesList(values, _) => Self::ValuesList(values, alias.into_iden()),
            Self::FunctionCall(func, _) => Self::FunctionCall(func, alias.into_iden()),
        }
    }

    /// The name of the table this item reads, when it names one
    pub fn table_name(&self) -> Option<&TableName> {
        match self {
            Self::Table(name, _) => Some(name),
            Self::SubQuery(_, _) | Self::ValuesList(_, _) | Self::FunctionCall(_, _) => None,
        }
    }

    /// The identifier a column of this item is qualified by: the alias when
    /// one is bound, otherwise the table identifier.
    pub fn qualifier(&self) -> &DynIden {
        match self {
            Self::Table(name, None) => name.table(),
            Self::Table(_, Some(alias))
            | Self::SubQuery(_, alias)
            | Self::ValuesList(_, alias)
            | Self::FunctionCall(_, alias) => alias,
        }
    }
}

impl Alias {
    pub fn new<T>(n: T) -> Self
    where
        T: Into<String>,
    {
        Self(n.into())
    }
}

impl Iden for Alias {
    fn unquoted(&self, s: &mut dyn fmt::Write) {
        write!(s, "{}", self.0).unwrap();
    }
}

impl NullAlias {
    pub fn new() -> Self {
        Self
    }
}

impl Iden for NullAlias {
    fn unquoted(&self, _s: &mut dyn fmt::Write) {}
}

impl LikeExpr {
    pub fn new<T>(pattern: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            pattern: pattern.into(),
            escape: None,
        }
    }

    pub fn escape(self, c: char) -> Self {
        Self {
            pattern: self.pattern,
            escape: Some(c),
        }
    }
}

impl IntoLikeExpr for LikeExpr {
    fn into_like_expr(self) -> LikeExpr {
        self
    }
}

impl<T> IntoLikeExpr for T
where
    T: Into<String>,
{
    fn into_like_expr(self) -> LikeExpr {
        LikeExpr::new(self)
    }
}

#[cfg(test)]
mod tests {
    pub use crate::{tests_cfg::*, *};
    pub use Character as CharReexport;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_identifier() {
        let query = Query::select()
            .column(Alias::new("hello-World_"))
            .to_owned();

        assert_eq!(query.to_string(QueryBuilder), r#"SELECT "hello-World_""#);
    }

    // [spec:pgorm:def:sql.types+1/test]
    #[test]
    fn test_quoted_identifier_1() {
        let query = Query::select().column(Alias::new("hel\"lo")).to_owned();

        assert_eq!(query.to_string(QueryBuilder), r#"SELECT "hel""lo""#);
    }

    #[test]
    fn test_quoted_identifier_2() {
        let query = Query::select().column(Alias::new("hel\"\"lo")).to_owned();

        assert_eq!(query.to_string(QueryBuilder), r#"SELECT "hel""""lo""#);
    }

    // [spec:pgorm:def:sql.types+1/test]
    #[test]
    fn test_cmp_identifier() {
        type CharLocal = Character;

        assert_eq!(
            ColumnRef::Column(Character::Id.into_iden()),
            ColumnRef::Column(Character::Id.into_iden())
        );
        assert_eq!(
            ColumnRef::Column(Character::Id.into_iden()),
            ColumnRef::Column(Char::Id.into_iden())
        );
        assert_eq!(
            ColumnRef::Column(Character::Id.into_iden()),
            ColumnRef::Column(CharLocal::Id.into_iden())
        );
        assert_eq!(
            ColumnRef::Column(Character::Id.into_iden()),
            ColumnRef::Column(CharReexport::Id.into_iden())
        );
        assert_eq!(
            ColumnRef::Column(Alias::new("id").into_iden()),
            ColumnRef::Column(Alias::new("id").into_iden())
        );
        assert_ne!(
            ColumnRef::Column(Alias::new("id").into_iden()),
            ColumnRef::Column(Alias::new("id_").into_iden())
        );
        assert_ne!(
            ColumnRef::Column(Character::Id.into_iden()),
            ColumnRef::Column(Alias::new("id").into_iden())
        );
        assert_ne!(
            ColumnRef::Column(Character::Id.into_iden()),
            ColumnRef::Column(Character::Table.into_iden())
        );
        assert_ne!(
            ColumnRef::Column(Character::Id.into_iden()),
            ColumnRef::Column(Font::Id.into_iden())
        );
    }
}
