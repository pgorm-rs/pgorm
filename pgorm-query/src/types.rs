//! Base types used throughout pgorm-query.

use crate::{FunctionCall, ValueTuple, Values, expr::*, query::*, template::SqlTemplate};
use std::{any::Any, fmt, ops, sync::Arc};

/// A name in SQL: what an identifier position renders.
///
/// Spelled as the fork spells its other text contracts — `SqlText` is trusted
/// SQL, `SqlName` is a name — so the two positions a string can land in are
/// told apart by the trait it satisfies rather than by convention.
// [spec:pgorm:def:sql.types+9]
pub trait SqlName: Any + Send + Sync {
    /// Write the identifier as PostgreSQL spells one: wrapped in double
    /// quotes, with any embedded double quote doubled.
    // [spec:pgorm:req:sql.render.ident-quoting+5]
    fn prepare(&self, s: &mut dyn fmt::Write) {
        write!(s, "\"{}\"", self.quoted()).unwrap();
    }

    /// The identifier's text with embedded double quotes doubled, ready to sit
    /// between the quotes [`prepare`](Self::prepare) writes.
    // [spec:pgorm:req:sql.render.ident-quoting+5]
    fn quoted(&self) -> String {
        self.to_string().replace('"', "\"\"")
    }

    fn to_string(&self) -> String {
        let mut s = String::new();
        self.unquoted(&mut s);
        s
    }

    fn unquoted(&self, s: &mut dyn fmt::Write);
}

/// A name whose text is fixed by the program, and which therefore hands that
/// text out as a `&str` without rendering it.
///
/// This is the base identifier contract of anything that stands where a column
/// does: entities, columns and primary keys implement it through pgorm's
/// derives, the [`AliasName`] token implements it, and pgorm's key-column sets
/// are built from it. `Copy` and `'static` say the name is part of the shape of
/// the program; `Debug` is what makes a column printable in the generic code
/// that takes one.
// [spec:pgorm:def:sql.types+9]
pub trait StaticName: SqlName + Copy + fmt::Debug + 'static {
    /// The name as an unquoted string.
    fn as_str(&self) -> &str;
}

/// A name, type-erased and shared: one `Arc<dyn SqlName>` wide, cheap to
/// clone, and comparable.
///
/// Every identifier position in the AST holds one of these, so the type is
/// deliberately two words and no more — widening it widens every node.
#[derive(Debug)]
#[repr(transparent)]
pub struct Name(Arc<dyn SqlName>);

impl ops::Deref for Name {
    type Target = dyn SqlName;

    fn deref(&self) -> &Self::Target {
        ops::Deref::deref(&self.0)
    }
}

impl Clone for Name {
    fn clone(&self) -> Name {
        Name(Arc::clone(&self.0))
    }
}

/// Two identifiers are equal when their concrete types and their rendered text
/// both are.
///
/// The type is asked of the identifier by [`TypeId`](std::any::TypeId), not
/// read off the trait object's vtable address: Rust guarantees a vtable
/// neither unique per type nor stable across codegen units, so an address
/// comparison can answer that two runtime names both spelling `"id"` differ.
/// `SqlName` is bounded on [`Any`] so the erased value can still be asked, and
/// asking costs the identifier no width — a `Name` sits in nearly every
/// node of the AST.
// [spec:pgorm:def:sql.types+9]
impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        let (this, that): (&dyn Any, &dyn Any) = (&*self.0, &*other.0);
        this.type_id() == that.type_id() && self.to_string() == other.to_string()
    }
}

impl Name {
    /// Erase a name the program already spells as a type.
    pub fn new<I>(i: I) -> Name
    where
        I: SqlName + 'static,
    {
        Name(Arc::new(i))
    }

    /// Mint a name from text computed at run time.
    ///
    /// The one route from a `String` into identifier position — there is no
    /// `impl IntoName for &str` — so a grep for `Name::runtime` finds every
    /// place a value becomes a name, which is the set a reader auditing for
    /// injection has to look at. A name written in the program says so with
    /// its type: an [`alias`] token, a derived enum, an entity column.
    ///
    /// The text is rendered as a QUOTED identifier like every other name, so
    /// this is not an escape hatch into SQL — only into naming.
    // [spec:pgorm:def:sql.types+9]
    pub fn runtime<T>(text: T) -> Name
    where
        T: Into<String>,
    {
        Name(Arc::new(RuntimeName(text.into())))
    }
}

pub trait IntoName {
    fn into_name(self) -> Name;
}

impl fmt::Debug for dyn SqlName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.unquoted(formatter);
        Ok(())
    }
}

/// Column references
// [spec:pgorm:def:sql.types.column-ref]
// [spec:pgorm:def:sql.ast.keywords+5]
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnRef {
    Column(Name),
    TableColumn(Name, Name),
    SchemaTableColumn(Name, Name, Name),
    Asterisk,
    TableAsterisk(Name),
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
/// let name = (Name::runtime("public"), Glyph::Table).into_table_name();
/// assert_eq!(
///     Table::truncate(name.clone()).to_string(),
///     r#"TRUNCATE TABLE "public"."glyph""#
/// );
/// assert_eq!(
///     Query::select().column(Asterisk).from(name).to_string(),
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
///     Name::runtime("q"),
/// );
/// Table::truncate(sub);
/// ```
///
/// Nor does an aliased table: binding an alias makes it a [`NamedTable`], and
/// an alias means nothing to a statement that only names the catalogue entry:
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// Table::truncate(Glyph::Table.into_named_table().alias(Name::runtime("g")));
/// ```
// [spec:pgorm:def:sql.types.table-ref+4]
// [spec:pgorm:sem:sql.ddl.panics+4/test]    the DDL-position panics are gone because the shapes
// that reached them no longer typecheck
/// A type name in cast or column-type position: optionally
/// schema-qualified, optionally an array. Every part renders as a QUOTED
/// identifier — `"tenant_a"."status"[]` — so a name is a name, never SQL.
///
/// This is the *only* thing a cast carries as its type: one node shape, the
/// quoted-or-verbatim question answered inside the type rather than by
/// picking a different node.
// [spec:pgorm:def:sql.types.type-name+4]
// [spec:pgorm:req:sql.ast.cast-shape]
#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    pub schema: Option<Name>,
    pub name: Name,
    pub array: bool,
    /// Render `name` as the caller's own SQL rather than as an identifier.
    pub verbatim: bool,
}

impl TypeName {
    /// A bare, non-array type name.
    pub fn new<T>(name: T) -> Self
    where
        T: IntoName,
    {
        Self {
            schema: None,
            name: name.into_name(),
            array: false,
            verbatim: false,
        }
    }

    /// A type EXPRESSION — `BIT(8)`, `numeric(12, 2)` — rendered verbatim,
    /// nothing quoted or escaped.
    ///
    /// Reachable only through [`Expr::cast_as_raw`](crate::Expr::cast_as_raw),
    /// whose argument is a literal written in the calling source: the text is
    /// program text the author already controls, never data, so rendering it
    /// as SQL adds no reach that writing the SQL by hand would not have. The
    /// `&'static str` bound is what enforces that — a runtime `String` cannot
    /// reach this constructor, so no value-derived text can become SQL here.
    /// Any type that arrives as a *name* — from a schema, a derive attribute,
    /// or anything a value could reach — takes [`new`](Self::new) and is
    /// quoted.
    pub fn raw(type_expr: &'static str) -> Self {
        Self {
            schema: None,
            name: Name::runtime(type_expr),
            array: false,
            verbatim: true,
        }
    }

    /// Qualify with a schema.
    pub fn schema<S>(mut self, schema: S) -> Self
    where
        S: IntoName,
    {
        self.schema = Some(schema.into_name());
        self
    }

    /// Mark as the array of the named type.
    pub fn array(mut self) -> Self {
        self.array = true;
        self
    }

    /// The SQL spelling: parts dot-joined, a structural `[]` suffix for
    /// arrays. A part that is a safe lowercase identifier
    /// (`^[a-z_][a-z0-9_]*$`) renders bare — PostgreSQL folds unquoted names
    /// to lowercase, so the bare and quoted spellings are the same name
    /// there, and grammar-sugar type names (`integer`) only resolve bare.
    /// Every other part renders as a QUOTED identifier, case preserved — so
    /// a name is a name, and text that is not one (`int4) + 100 --`) becomes
    /// an identifier PostgreSQL refuses rather than SQL it executes. A
    /// [`raw`](Self::raw) type expression is the one exception and
    /// renders as written.
    pub fn to_sql_string(&self) -> String {
        if self.verbatim {
            return self.raw_text();
        }
        let mut out = String::new();
        if let Some(schema) = &self.schema {
            Self::prepare_part(schema, &mut out);
            out.push('.');
        }
        Self::prepare_part(&self.name, &mut out);
        if self.array {
            out.push_str("[]");
        }
        out
    }

    /// Write one name part under the policy [`to_sql_string`](Self::to_sql_string)
    /// documents: a safe lowercase identifier bare, everything else quoted.
    ///
    /// Shared with the render sites that emit a single caller-supplied name
    /// which is not a `TypeName` — the index access method of
    /// [`IndexType::Named`](crate::IndexType::Named) — so one policy covers
    /// every name-shaped position rather than each site inventing its own.
    // [spec:pgorm:req:sql.render.ident-quoting+5]
    pub(crate) fn prepare_part(part: &Name, out: &mut String) {
        let text = part.to_string();
        let mut chars = text.chars();
        let safe = matches!(chars.next(), Some('a'..='z' | '_'))
            && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'));
        if safe {
            out.push_str(&text);
        } else {
            part.prepare(out);
        }
    }

    /// The unquoted dotted spelling — `tenant_a.status[]` — for consumers
    /// that compose their own quoting downstream (the pipeline adapter).
    pub fn raw_text(&self) -> String {
        let mut out = String::new();
        if let Some(schema) = &self.schema {
            out.push_str(&schema.to_string());
            out.push('.');
        }
        out.push_str(&self.name.to_string());
        if self.array {
            out.push_str("[]");
        }
        out
    }
}

impl<T> From<T> for TypeName
where
    T: IntoName,
{
    fn from(name: T) -> Self {
        Self::new(name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableName {
    /// Table identifier without any schema prefix
    Table(Name),
    /// Table identifier with schema prefix
    SchemaTable(Name, Name),
}

/// Conversion into the [`TableName`] a DDL statement targets.
// [spec:pgorm:def:sql.types.table-ref+4]
pub trait IntoTableName {
    /// Consume `self` and produce a [`TableName`]
    fn into_table_name(self) -> TableName;
}

/// A table name with an optional alias: the target of an `INSERT`, `UPDATE` or
/// `DELETE`, and the named form of a [`FromItem`].
///
/// PostgreSQL's write statements target a relation by name and may bind an
/// alias to it, and nothing else — so this is the type they take, and the
/// alias reads back in the statement:
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let target = Glyph::Table.into_named_table().alias(Name::runtime("g"));
/// assert_eq!(
///     Query::delete()
///         .from_table(target)
///         .and_where(Expr::col((Name::runtime("g"), Glyph::Id)).eq(1))
///         .to_string(),
///     r#"DELETE FROM "glyph" AS "g" WHERE "g"."id" = 1"#
/// );
/// ```
///
/// The value-producing [`FromItem`] forms name no relation, so none of them
/// reaches a write target. A subquery does not:
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// let sub = FromItem::SubQuery(
///     Query::select().column(Glyph::Id).from(Glyph::Table).take(),
///     Name::runtime("q"),
/// );
/// Query::insert().into_table(sub);
/// ```
///
/// nor a values list:
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// let values = FromItem::ValuesList(
///     vec![(1i32,).into_value_tuple()],
///     Name::runtime("v"),
/// );
/// Query::update().table(values);
/// ```
///
/// nor a function call:
///
/// ```compile_fail,E0277
/// use pgorm_query::{*, tests_cfg::*};
///
/// let func = FromItem::FunctionCall(
///     Func::named(Name::runtime("generate_series")).arg(1i32),
///     Name::runtime("f"),
/// );
/// Query::delete().from_table(func);
/// ```
// [spec:pgorm:def:sql.types.table-ref+4]
#[derive(Debug, Clone, PartialEq)]
pub struct NamedTable {
    /// The table this reference names
    pub name: TableName,
    /// The alias bound to the name, when one is
    pub alias: Option<Name>,
}

/// Conversion into the [`NamedTable`] a DML statement targets.
// [spec:pgorm:def:sql.types.table-ref+4]
pub trait IntoNamedTable {
    /// Consume `self` and produce a [`NamedTable`]
    fn into_named_table(self) -> NamedTable;
}

/// An entry in a `FROM` clause or a join.
///
/// A named table carries its alias beside it rather than in the variant, so
/// aliasing is orthogonal to how the name is qualified; the value-producing
/// forms carry the alias Postgres requires of them.
// [spec:pgorm:def:sql.types.table-ref+4]
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum FromItem {
    /// A table name with an optional alias
    Table(NamedTable),
    /// Subquery with alias
    SubQuery(SelectStatement, Name),
    /// Values list with alias
    ValuesList(Vec<ValueTuple>, Name),
    /// Function call with alias
    FunctionCall(FunctionCall, Name),
    /// A validated SQL fragment with alias: text this crate did not build,
    /// standing where a relation stands.
    ///
    /// The fragment is a [`SqlTemplate`], so it arrives already paired with
    /// the values its `$N` markers number — usually through
    /// [`SqlTemplate::from_sql`], which reads the `$` grammar of real SQL.
    /// Rendering re-emits each paired value as a parameter of the enclosing
    /// statement, so the fragment's markers renumber into that statement's
    /// space and a fragment may sit in a query that binds values of its own:
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let fragment = SqlTemplate::from_sql(
    ///     r#"SELECT "id" FROM "glyph" WHERE "aspect" > $1"#,
    ///     [1i32.into()],
    /// )?;
    ///
    /// let (sql, values) = Query::select()
    ///     .column(Asterisk)
    ///     .from(FromItem::Template(fragment, Name::runtime("g")))
    ///     .and_where(Expr::col((Name::runtime("g"), Glyph::Id)).lt(9))
    ///     .build();
    ///
    /// assert_eq!(
    ///     sql,
    ///     "SELECT * FROM (SELECT \"id\" FROM \"glyph\" WHERE \"aspect\" > $1\n\
    ///      ) AS \"g\" WHERE \"g\".\"id\" < $2"
    /// );
    /// assert_eq!(values.0, vec![1i32.into(), 9i32.into()]);
    /// # Ok::<(), pgorm_query::error::Error>(())
    /// ```
    Template(SqlTemplate, Name),
}

/// Conversion into a [`FromItem`].
// [spec:pgorm:def:sql.types.table-ref+4]
pub trait IntoFromItem {
    /// Consume `self` and produce a [`FromItem`]
    fn into_from_item(self) -> FromItem;
}

/// Unary operator
// [spec:pgorm:def:sql.types.opers+4]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOper {
    Not,
}

/// Binary operator
// [spec:pgorm:def:sql.types.opers+4]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOper {
    And,
    Or,
    Like,
    NotLike,
    Is,
    IsNot,
    /// `IS DISTINCT FROM`. Inequality that reads NULL as a value rather than
    /// as unknown: two NULLs are not distinct, a NULL and a non-NULL are, and
    /// the answer is never NULL.
    IsDistinctFrom,
    /// `IS NOT DISTINCT FROM`. The complement of
    /// [`BinOper::IsDistinctFrom`], and so null-safe equality.
    IsNotDistinctFrom,
    In,
    NotIn,
    Between,
    NotBetween,
    /// `BETWEEN SYMMETRIC`. `BETWEEN` with its bounds sorted first, so the
    /// range holds whichever order the two are given in.
    BetweenSymmetric,
    /// `NOT BETWEEN SYMMETRIC`. The complement of
    /// [`BinOper::BetweenSymmetric`].
    NotBetweenSymmetric,
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
    /// `#>`. Retrieves the JSON value at a `text[]` path.
    GetJsonPath,
    /// `#>>`. Retrieves the value at a `text[]` path as text.
    CastJsonPath,
    /// `?`. Whether a top-level key or string element exists.
    HasJsonKey,
    /// `?|`. Whether any key in a `text[]` list exists.
    HasAnyJsonKeys,
    /// `?&`. Whether every key in a `text[]` list exists.
    HasAllJsonKeys,
    /// `~` Regex operator.
    Regex,
    /// `~*`. Regex operator with case insensitive matching.
    RegexCaseInsensitive,
    /// `AT TIME ZONE`. Reinterprets a timestamp in the zone its right operand
    /// names, converting between `timestamp` and `timestamptz` in whichever
    /// direction the left operand's type calls for.
    AtTimeZone,
    EuclideanDistance,
    NegativeInnerProduct,
    CosineDistance,
    /// An operator this enum has no variant for, written verbatim.
    Raw(&'static str),
}

/// Join types that carry an `ON` constraint
// [spec:pgorm:req:sql.ast.select.join+1]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Join,
    InnerJoin,
    LeftJoin,
    RightJoin,
    FullOuterJoin,
}

/// A join and the constraint it carries.
///
/// `CROSS JOIN` is the one join PostgreSQL takes without an `ON` clause and
/// every other join requires one, so the two travel together: a constrained
/// cross join and an unconstrained inner join both fail to construct.
// [spec:pgorm:req:sql.ast.select.join+1]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JoinKind {
    Cross,
    Qualified(JoinType, JoinOn),
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
// [spec:pgorm:req:sql.render.joins+2]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JoinOn {
    Condition(Box<ConditionHolder>),
}

/// Ordering options
// [spec:pgorm:req:sql.ast.order+3]
#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Asc,
    Desc,
    Field(Values),
}

#[path = "types_alias.rs"]
mod types_alias;
pub use types_alias::*;

/// A name computed at run time, reachable only through
/// [`Name::runtime`].
///
/// Private on purpose: `Name::runtime` is the one door a `String` walks
/// through to reach identifier position, so every entry of
/// possibly-untrusted text into a name is one grep away.
#[derive(Debug, Clone)]
struct RuntimeName(String);

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
///     query.to_string(),
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
///     query.to_string(),
///     r#"SELECT "character".* FROM "character""#
/// );
/// ```
#[derive(Default, Debug, Clone, Copy)]
pub struct Asterisk;

/// SQL Keywords
// [spec:pgorm:def:sql.ast.keywords+5]
#[derive(Debug, Clone, PartialEq)]
pub enum Keyword {
    Null,
    CurrentDate,
    CurrentTime,
    CurrentTimestamp,
}

/// Like Expression
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl<T: 'static> IntoName for T
where
    T: SqlName,
{
    fn into_name(self) -> Name {
        Name::new(self)
    }
}

impl IntoName for Name {
    fn into_name(self) -> Name {
        self
    }
}

impl IntoColumnRef for ColumnRef {
    fn into_column_ref(self) -> ColumnRef {
        self
    }
}

impl<T: 'static> IntoColumnRef for T
where
    T: IntoName,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::Column(self.into_name())
    }
}

impl IntoColumnRef for Asterisk {
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::Asterisk
    }
}

impl<S: 'static, T: 'static> IntoColumnRef for (S, T)
where
    S: IntoName,
    T: IntoName,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::TableColumn(self.0.into_name(), self.1.into_name())
    }
}

impl<T: 'static> IntoColumnRef for (T, Asterisk)
where
    T: IntoName,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::TableAsterisk(self.0.into_name())
    }
}

impl<S: 'static, T: 'static, U: 'static> IntoColumnRef for (S, T, U)
where
    S: IntoName,
    T: IntoName,
    U: IntoName,
{
    fn into_column_ref(self) -> ColumnRef {
        ColumnRef::SchemaTableColumn(self.0.into_name(), self.1.into_name(), self.2.into_name())
    }
}

impl IntoTableName for TableName {
    fn into_table_name(self) -> TableName {
        self
    }
}

impl<T: 'static> IntoTableName for T
where
    T: IntoName,
{
    fn into_table_name(self) -> TableName {
        TableName::Table(self.into_name())
    }
}

impl<S: 'static, T: 'static> IntoTableName for (S, T)
where
    S: IntoName,
    T: IntoName,
{
    fn into_table_name(self) -> TableName {
        TableName::SchemaTable(self.0.into_name(), self.1.into_name())
    }
}

// [spec:pgorm:def:sql.types.table-ref+4]
impl TableName {
    /// The table identifier, without its schema
    pub fn table(&self) -> &Name {
        match self {
            Self::Table(table) | Self::SchemaTable(_, table) => table,
        }
    }

    /// The schema identifier, when the name carries one
    pub fn schema(&self) -> Option<&Name> {
        match self {
            Self::Table(_) => None,
            Self::SchemaTable(schema, _) => Some(schema),
        }
    }
}

impl IntoNamedTable for NamedTable {
    fn into_named_table(self) -> NamedTable {
        self
    }
}

impl<T: 'static> IntoNamedTable for T
where
    T: IntoTableName,
{
    fn into_named_table(self) -> NamedTable {
        NamedTable {
            name: self.into_table_name(),
            alias: None,
        }
    }
}

// [spec:pgorm:def:sql.types.table-ref+4]
impl NamedTable {
    /// Bind an alias to the name, replacing any alias already bound
    pub fn alias<A>(self, alias: A) -> Self
    where
        A: IntoName,
    {
        Self {
            name: self.name,
            alias: Some(alias.into_name()),
        }
    }

    /// The identifier a column of this table is qualified by: the alias when
    /// one is bound, otherwise the table identifier.
    pub fn qualifier(&self) -> &Name {
        self.alias.as_ref().unwrap_or_else(|| self.name.table())
    }
}

impl From<TableName> for NamedTable {
    fn from(name: TableName) -> Self {
        Self { name, alias: None }
    }
}

impl IntoFromItem for FromItem {
    fn into_from_item(self) -> FromItem {
        self
    }
}

impl<T: 'static> IntoFromItem for T
where
    T: IntoNamedTable,
{
    fn into_from_item(self) -> FromItem {
        FromItem::Table(self.into_named_table())
    }
}

impl From<NamedTable> for FromItem {
    fn from(table: NamedTable) -> Self {
        Self::Table(table)
    }
}

impl From<TableName> for FromItem {
    fn from(name: TableName) -> Self {
        Self::Table(name.into())
    }
}

// [spec:pgorm:def:sql.types.table-ref+4]
impl FromItem {
    /// Add or replace the current alias
    pub fn alias<A>(self, alias: A) -> Self
    where
        A: IntoName,
    {
        match self {
            Self::Table(table) => Self::Table(table.alias(alias)),
            Self::SubQuery(statement, _) => Self::SubQuery(statement, alias.into_name()),
            Self::ValuesList(values, _) => Self::ValuesList(values, alias.into_name()),
            Self::FunctionCall(func, _) => Self::FunctionCall(func, alias.into_name()),
            Self::Template(template, _) => Self::Template(template, alias.into_name()),
        }
    }

    /// The name of the table this item reads, when it names one
    pub fn table_name(&self) -> Option<&TableName> {
        match self {
            Self::Table(table) => Some(&table.name),
            Self::SubQuery(_, _)
            | Self::ValuesList(_, _)
            | Self::FunctionCall(_, _)
            | Self::Template(_, _) => None,
        }
    }

    /// The identifier a column of this item is qualified by: the alias when
    /// one is bound, otherwise the table identifier.
    pub fn qualifier(&self) -> &Name {
        match self {
            Self::Table(table) => table.qualifier(),
            Self::SubQuery(_, alias)
            | Self::ValuesList(_, alias)
            | Self::FunctionCall(_, alias)
            | Self::Template(_, alias) => alias,
        }
    }
}

// [spec:pgorm:def:sql.types+9]
impl SqlName for RuntimeName {
    fn unquoted(&self, s: &mut dyn fmt::Write) {
        write!(s, "{}", self.0).unwrap();
    }
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
            .column(Name::runtime("hello-World_"))
            .to_owned();

        assert_eq!(query.to_string(), r#"SELECT "hello-World_""#);
    }

    // [spec:pgorm:def:sql.types+9/test]
    #[test]
    fn test_quoted_identifier_1() {
        let query = Query::select().column(Name::runtime("hel\"lo")).to_owned();

        assert_eq!(query.to_string(), r#"SELECT "hel""lo""#);
    }

    #[test]
    fn test_quoted_identifier_2() {
        let query = Query::select()
            .column(Name::runtime("hel\"\"lo"))
            .to_owned();

        assert_eq!(query.to_string(), r#"SELECT "hel""""lo""#);
    }

    // [spec:pgorm:def:sql.types+9/test]
    #[test]
    fn test_cmp_identifier() {
        type CharLocal = Character;

        assert_eq!(
            ColumnRef::Column(Character::Id.into_name()),
            ColumnRef::Column(Character::Id.into_name())
        );
        assert_eq!(
            ColumnRef::Column(Character::Id.into_name()),
            ColumnRef::Column(Char::Id.into_name())
        );
        assert_eq!(
            ColumnRef::Column(Character::Id.into_name()),
            ColumnRef::Column(CharLocal::Id.into_name())
        );
        assert_eq!(
            ColumnRef::Column(Character::Id.into_name()),
            ColumnRef::Column(CharReexport::Id.into_name())
        );
        assert_eq!(
            ColumnRef::Column(Name::runtime("id")),
            ColumnRef::Column(Name::runtime("id"))
        );
        assert_ne!(
            ColumnRef::Column(Name::runtime("id")),
            ColumnRef::Column(Name::runtime("id_"))
        );
        assert_ne!(
            ColumnRef::Column(Character::Id.into_name()),
            ColumnRef::Column(Name::runtime("id"))
        );
        assert_ne!(
            ColumnRef::Column(Character::Id.into_name()),
            ColumnRef::Column(Character::Table.into_name())
        );
        assert_ne!(
            ColumnRef::Column(Character::Id.into_name()),
            ColumnRef::Column(Font::Id.into_name())
        );
    }
}
