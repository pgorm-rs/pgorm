//! For calling built-in SQL functions.

use crate::{expr::*, types::*};

/// Functions
#[derive(Debug, Clone, PartialEq)]
pub enum Function {
    Max,
    Min,
    Sum,
    Avg,
    Abs,
    Count,
    IfNull,
    CharLength,
    Cast,
    Custom(DynIden),
    Coalesce,
    Lower,
    Upper,
    BitAnd,
    BitOr,
    Random,
    Round,
    ToTsquery,
    ToTsvector,
    PhrasetoTsquery,
    PlaintoTsquery,
    WebsearchToTsquery,
    TsRank,
    TsRankCd,
    StartsWith,
    GenRandomUUID,
    Any,
    Some,
    All,
}

/// Function call.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub(crate) func: Function,
    pub(crate) args: Vec<SimpleExpr>,
    pub(crate) mods: Vec<FuncArgMod>,
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct FuncArgMod {
    pub distinct: bool,
}

impl FunctionCall {
    pub(crate) fn new(func: Function) -> Self {
        Self {
            func,
            args: Vec::new(),
            mods: Vec::new(),
        }
    }

    /// Append an argument to the function call
    pub fn arg<T>(self, arg: T) -> Self
    where
        T: Into<SimpleExpr>,
    {
        self.arg_with(arg, Default::default())
    }

    pub(crate) fn arg_with<T>(mut self, arg: T, mod_: FuncArgMod) -> Self
    where
        T: Into<SimpleExpr>,
    {
        self.args.push(arg.into());
        self.mods.push(mod_);
        self
    }

    /// Replace the arguments of the function call
    pub fn args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = SimpleExpr>,
    {
        self.args = args.into_iter().collect();
        self.mods = vec![Default::default(); self.args.len()];
        self
    }

    pub fn get_func(&self) -> &Function {
        &self.func
    }

    pub fn get_args(&self) -> &[SimpleExpr] {
        &self.args
    }

    pub fn get_mods(&self) -> &[FuncArgMod] {
        &self.mods
    }
}

/// Function call helper.
#[derive(Debug, Clone)]
pub struct Func;

impl Func {
    /// Call a custom function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// struct MyFunction;
    ///
    /// impl Iden for MyFunction {
    ///     fn unquoted(&self, s: &mut dyn Write) {
    ///         write!(s, "MY_FUNCTION").unwrap();
    ///     }
    /// }
    ///
    /// let query = Query::select()
    ///     .expr(Func::cust(MyFunction).arg("hello"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT MY_FUNCTION('hello')"#
    /// );
    /// ```
    pub fn cust<T>(func: T) -> FunctionCall
    where
        T: IntoIden,
    {
        FunctionCall::new(Function::Custom(func.into_iden()))
    }

    /// Call `MAX` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::max(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT MAX("id") FROM "character""#
    /// );
    /// ```
    pub fn max<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Max).arg(expr)
    }

    /// Call `MIN` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::min(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT MIN("id") FROM "character""#
    /// );
    /// ```
    pub fn min<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Min).arg(expr)
    }

    /// Call `SUM` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::sum(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT SUM("id") FROM "character""#
    /// );
    /// ```
    pub fn sum<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Sum).arg(expr)
    }

    /// Call `AVG` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::avg(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT AVG("id") FROM "character""#
    /// );
    /// ```
    pub fn avg<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Avg).arg(expr)
    }

    /// Call `ABS` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::abs(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT ABS("id") FROM "character""#
    /// );
    /// ```
    pub fn abs<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Abs).arg(expr)
    }

    /// Call `COUNT` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::count(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT COUNT("id") FROM "character""#
    /// );
    /// ```
    pub fn count<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Count).arg(expr)
    }

    /// Call `COUNT` function with the `DISTINCT` modifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::count_distinct(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT COUNT(DISTINCT "id") FROM "character""#
    /// );
    /// ```
    pub fn count_distinct<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Count).arg_with(expr, FuncArgMod { distinct: true })
    }

    /// Call `CHAR_LENGTH` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::char_length(Expr::col(Character::Character)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT CHAR_LENGTH("character") FROM "character""#
    /// );
    /// ```
    pub fn char_length<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::CharLength).arg(expr)
    }

    /// Call `IF NULL` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::if_null(
    ///             Expr::col(Character::Character),
    ///             Expr::val("default")
    ///         ))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT COALESCE("character", 'default') FROM "character""#
    /// );
    /// ```
    pub fn if_null<A, B>(a: A, b: B) -> FunctionCall
    where
        A: Into<SimpleExpr>,
        B: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::IfNull).args([a.into(), b.into()])
    }

    /// Call `CAST` function with a custom type.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::cast_as(
    ///             Expr::col(Character::Id),
    ///             Alias::new("TEXT")
    ///         ))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT CAST("id" AS TEXT) FROM "character""#
    /// );
    /// ```
    pub fn cast_as<V, I>(expr: V, iden: I) -> FunctionCall
    where
        V: Into<SimpleExpr>,
        I: IntoIden,
    {
        let expr: SimpleExpr = expr.into();
        FunctionCall::new(Function::Cast).arg(expr.binary(
            BinOper::As,
            Expr::cust(iden.into_iden().to_string().as_str()),
        ))
    }

    /// Call `COALESCE` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::coalesce([
    ///         Expr::col(Char::SizeW).into(),
    ///         Expr::col(Char::SizeH).into(),
    ///         Expr::val(12).into(),
    ///     ]))
    ///     .from(Char::Table)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT COALESCE("size_w", "size_h", 12) FROM "character""#
    /// );
    /// ```
    pub fn coalesce<I>(args: I) -> FunctionCall
    where
        I: IntoIterator<Item = SimpleExpr>,
    {
        FunctionCall::new(Function::Coalesce).args(args)
    }

    /// Call `LOWER` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::lower(Expr::col(Character::Character)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT LOWER("character") FROM "character""#
    /// );
    /// ```
    pub fn lower<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Lower).arg(expr)
    }

    /// Call `UPPER` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::upper(Expr::col(Character::Character)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT UPPER("character") FROM "character""#
    /// );
    /// ```
    pub fn upper<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Upper).arg(expr)
    }

    /// Call `BIT_AND` function, this is not supported on SQLite.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::bit_and(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT BIT_AND("id") FROM "character""#
    /// );
    /// ```
    pub fn bit_and<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::BitAnd).arg(expr)
    }

    /// Call `BIT_OR` function, this is not supported on SQLite.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::bit_or(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT BIT_OR("id") FROM "character""#
    /// );
    /// ```
    pub fn bit_or<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::BitOr).arg(expr)
    }

    /// Call `ROUND` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::round(Expr::col(Character::Id)))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT ROUND("id") FROM "character""#
    /// );
    /// ```
    pub fn round<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Round).arg(expr)
    }

    /// Call `ROUND` function with the precision.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::round_with_precision(
    ///             Expr::col(Character::Id),
    ///             2
    ///         ))
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT ROUND("id", 2) FROM "character""#
    /// );
    /// ```
    pub fn round_with_precision<T, U>(expr: T, precision: U) -> FunctionCall
    where
        T: Into<SimpleExpr>,
        U: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Round).args([expr.into(), precision.into()])
    }

    /// Call `RANDOM` function.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .expr(Func::random())
    ///         .from(Character::Table)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT RANDOM() FROM "character""#
    /// );
    /// ```
    pub fn random() -> FunctionCall {
        FunctionCall::new(Function::Random)
    }

    /// Call `TO_TSQUERY` function. Postgres only.
    ///
    /// The parameter `regconfig` represents the OID of the text search configuration.
    /// If the value is `None` the argument is omitted from the query, and hence the database default used.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::to_tsquery("a & b", None))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT TO_TSQUERY('a & b')"#
    /// );
    /// ```
    pub fn to_tsquery<T>(expr: T, regconfig: Option<u32>) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        match regconfig {
            Some(config) => {
                let config = SimpleExpr::Value(config.into());
                FunctionCall::new(Function::ToTsquery).args([config, expr.into()])
            }
            None => FunctionCall::new(Function::ToTsquery).arg(expr),
        }
    }

    /// Call `TO_TSVECTOR` function. Postgres only.
    ///
    /// The parameter `regconfig` represents the OID of the text search configuration.
    /// If the value is `None` the argument is omitted from the query, and hence the database default used.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::to_tsvector("a b", None))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT TO_TSVECTOR('a b')"#
    /// );
    /// ```
    pub fn to_tsvector<T>(expr: T, regconfig: Option<u32>) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        match regconfig {
            Some(config) => {
                let config = SimpleExpr::Value(config.into());
                FunctionCall::new(Function::ToTsvector).args([config, expr.into()])
            }
            None => FunctionCall::new(Function::ToTsvector).arg(expr),
        }
    }

    /// Call `PHRASE_TO_TSQUERY` function. Postgres only.
    ///
    /// The parameter `regconfig` represents the OID of the text search configuration.
    /// If the value is `None` the argument is omitted from the query, and hence the database default used.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::phraseto_tsquery("a b", None))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT PHRASETO_TSQUERY('a b')"#
    /// );
    /// ```
    pub fn phraseto_tsquery<T>(expr: T, regconfig: Option<u32>) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        match regconfig {
            Some(config) => {
                let config = SimpleExpr::Value(config.into());
                FunctionCall::new(Function::PhrasetoTsquery).args([config, expr.into()])
            }
            None => FunctionCall::new(Function::PhrasetoTsquery).arg(expr),
        }
    }

    /// Call `PLAIN_TO_TSQUERY` function. Postgres only.
    ///
    /// The parameter `regconfig` represents the OID of the text search configuration.
    /// If the value is `None` the argument is omitted from the query, and hence the database default used.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::plainto_tsquery("a b", None))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT PLAINTO_TSQUERY('a b')"#
    /// );
    /// ```
    pub fn plainto_tsquery<T>(expr: T, regconfig: Option<u32>) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        match regconfig {
            Some(config) => {
                let config = SimpleExpr::Value(config.into());
                FunctionCall::new(Function::PlaintoTsquery).args([config, expr.into()])
            }
            None => FunctionCall::new(Function::PlaintoTsquery).arg(expr),
        }
    }

    /// Call `WEBSEARCH_TO_TSQUERY` function. Postgres only.
    ///
    /// The parameter `regconfig` represents the OID of the text search configuration.
    /// If the value is `None` the argument is omitted from the query, and hence the database default used.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::websearch_to_tsquery("a b", None))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT WEBSEARCH_TO_TSQUERY('a b')"#
    /// );
    /// ```
    pub fn websearch_to_tsquery<T>(expr: T, regconfig: Option<u32>) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        match regconfig {
            Some(config) => {
                let config = SimpleExpr::Value(config.into());
                FunctionCall::new(Function::WebsearchToTsquery).args([config, expr.into()])
            }
            None => FunctionCall::new(Function::WebsearchToTsquery).arg(expr),
        }
    }

    /// Call `TS_RANK` function. Postgres only.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::ts_rank("a b", "a&b"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT TS_RANK('a b', 'a&b')"#
    /// );
    /// ```
    pub fn ts_rank<T>(vector: T, query: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::TsRank).args([vector.into(), query.into()])
    }

    /// Call `TS_RANK_CD` function. Postgres only.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::ts_rank_cd("a b", "a&b"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT TS_RANK_CD('a b', 'a&b')"#
    /// );
    /// ```
    pub fn ts_rank_cd<T>(vector: T, query: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::TsRankCd).args([vector.into(), query.into()])
    }

    /// Call `ANY` function. Postgres only.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select().expr(Func::any(vec![0, 1])).to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT ANY(ARRAY [0,1])"#
    /// );
    /// ```
    pub fn any<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Any).arg(expr)
    }

    /// Call `SOME` function. Postgres only.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select().expr(Func::some(vec![0, 1])).to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT SOME(ARRAY [0,1])"#
    /// );
    /// ```
    pub fn some<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Some).arg(expr)
    }

    /// Call `ALL` function. Postgres only.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select().expr(Func::all(vec![0, 1])).to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT ALL(ARRAY [0,1])"#
    /// );
    /// ```
    pub fn all<T>(expr: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::All).arg(expr)
    }

    /// Call `STARTS_WITH` function. Postgres only.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .expr(Func::starts_with("123", "1"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT STARTS_WITH('123', '1')"#
    /// );
    /// ```
    pub fn starts_with<T, P>(text: T, prefix: P) -> FunctionCall
    where
        T: Into<SimpleExpr>,
        P: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::StartsWith).args([text.into(), prefix.into()])
    }

    /// Call `GEN_RANDOM_UUID` function. Postgres only.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select().expr(Func::gen_random_uuid()).to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     r#"SELECT GEN_RANDOM_UUID()"#
    /// );
    /// ```
    pub fn gen_random_uuid() -> FunctionCall {
        FunctionCall::new(Function::GenRandomUUID)
    }
}
