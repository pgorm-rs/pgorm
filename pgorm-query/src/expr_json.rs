//! JSON operators on [`Expr`]: field and path extraction, and key existence.
//!
//! Containment (`@>`, `<@`) and concatenation (`||`) are not here. PostgreSQL
//! spells them with one operator each across every type that has them — arrays,
//! ranges, `tsquery`, `jsonb` — so [`Expr::contains`], [`Expr::contained`] and
//! [`Expr::concat`] already reach `jsonb` operands and a JSON-specific
//! duplicate would only be a second name for the same node.

use super::*;

/// Gather a JSON path or key list into the single `text[]` parameter the
/// operators demand.
fn text_array<S, I>(values: I) -> Value
where
    S: Into<String>,
    I: IntoIterator<Item = S>,
{
    Value::array(values.into_iter().map(Into::into))
}

impl Expr {
    /// Express a postgres retrieves JSON field as JSON value (`->`).
    ///
    /// The right operand is any expression, because `->` selects an object key
    /// by text *or* an array element by integer, and the two are the same
    /// operator. Drilling more than one level means re-entering the builder —
    /// `Expr::expr(a.get_json_field("x")).get_json_field("y")` — so
    /// [`Expr::get_json_path`] is the paved road for a multi-step path.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Font::Variant)
    ///     .from(Font::Table)
    ///     .and_where(Expr::col(Font::Variant).get_json_field("a"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "variant" FROM "font" WHERE "variant" -> 'a'"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.json]
    pub fn get_json_field<T>(self, right: T) -> SimpleExpr
    where
        T: Into<SimpleExpr>,
    {
        self.bin_op(BinOper::GetJsonField, right)
    }

    /// Express a postgres retrieves JSON field and casts it to an appropriate SQL type (`->>`).
    ///
    /// The result is `text`, not JSON, which is what makes this the end of a
    /// chain: no JSON operator applies to what it returns.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Font::Variant)
    ///     .from(Font::Table)
    ///     .and_where(Expr::col(Font::Variant).cast_json_field("a"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "variant" FROM "font" WHERE "variant" ->> 'a'"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.json]
    pub fn cast_json_field<T>(self, right: T) -> SimpleExpr
    where
        T: Into<SimpleExpr>,
    {
        self.bin_op(BinOper::CastJsonField, right)
    }

    /// Express a postgres retrieves the JSON value at a path (`#>`).
    ///
    /// One operator walks the whole path, where `->` walks one step, so a
    /// three-deep lookup is one node instead of three nested ones. The path is
    /// a `text[]`, gathered into a single parameter the way
    /// [`Expr::eq_any`] gathers a list: depth varies without varying the
    /// statement text.
    ///
    /// Path steps are text even when they index into a JSON array, because the
    /// array is `text[]` and can hold nothing else; PostgreSQL reads a step
    /// that parses as an integer as a subscript. An empty path selects the
    /// document itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Char::Id)
    ///     .from(Char::Table)
    ///     .and_where(Expr::expr(Expr::col(Char::UserData).get_json_path(["a", "b"])).is_not_null())
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE ("user_data" #> ARRAY ['a','b']) IS NOT NULL"#
    /// );
    ///
    /// let (sql, values) = query.build();
    /// assert_eq!(
    ///     sql,
    ///     r#"SELECT "id" FROM "character" WHERE ("user_data" #> $1) IS NOT NULL"#
    /// );
    /// assert_eq!(values.0.len(), 1);
    /// ```
    // [spec:pgorm:req:sql.ast.expr.json]
    pub fn get_json_path<S, I>(self, path: I) -> SimpleExpr
    where
        S: Into<String>,
        I: IntoIterator<Item = S>,
    {
        self.bin_op(BinOper::GetJsonPath, text_array(path))
    }

    /// Express a postgres retrieves the value at a JSON path as text (`#>>`).
    ///
    /// [`Expr::get_json_path`] with the `->>` ending: the same path walk, the
    /// same single `text[]` parameter, but the result is `text` rather than
    /// JSON, so it compares against Rust strings without a cast.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Char::Id)
    ///     .from(Char::Table)
    ///     .and_where(Expr::expr(Expr::col(Char::UserData).cast_json_path(["a", "b"])).eq("x"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE ("user_data" #>> ARRAY ['a','b']) = 'x'"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.json]
    pub fn cast_json_path<S, I>(self, path: I) -> SimpleExpr
    where
        S: Into<String>,
        I: IntoIterator<Item = S>,
    {
        self.bin_op(BinOper::CastJsonPath, text_array(path))
    }

    /// Express a postgres JSON key existence test (`?`).
    ///
    /// True when the operand is an object holding `key`, an array holding it as
    /// a string element, or the string itself — one operator over the three
    /// readings of "contains this string at the top level".
    ///
    /// The key is text and nothing else: `jsonb ? integer` is not an operator
    /// PostgreSQL defines, so the bound is `Into<String>` rather than
    /// `Into<SimpleExpr>` and a numeric key fails to compile instead of failing
    /// on the server. The family is `jsonb`-only — `json` has no `?` — so a
    /// `json` column needs a cast, which [`Expr::cast_as`] supplies.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Char::Id)
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::UserData).has_json_key("a"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "user_data" ? 'a'"#
    /// );
    ///
    /// let (sql, values) = query.build();
    /// assert_eq!(sql, r#"SELECT "id" FROM "character" WHERE "user_data" ? $1"#);
    /// ```
    // [spec:pgorm:req:sql.ast.expr.json]
    pub fn has_json_key<S>(self, key: S) -> SimpleExpr
    where
        S: Into<String>,
    {
        let key: String = key.into();
        self.bin_op(BinOper::HasJsonKey, key)
    }

    /// Express a postgres any-key existence test (`?|`).
    ///
    /// [`Expr::has_json_key`] over a list, in one `text[]` parameter. An empty
    /// list is false for every operand: no key of none can be present.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Char::Id)
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::UserData).has_any_json_keys(["a", "b"]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "user_data" ?| ARRAY ['a','b']"#
    /// );
    /// ```
    /// Empty key list
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Char::Id)
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::UserData).has_any_json_keys(Vec::<String>::new()))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "user_data" ?| ARRAY []::text[]"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.json]
    pub fn has_any_json_keys<S, I>(self, keys: I) -> SimpleExpr
    where
        S: Into<String>,
        I: IntoIterator<Item = S>,
    {
        self.bin_op(BinOper::HasAnyJsonKeys, text_array(keys))
    }

    /// Express a postgres all-keys existence test (`?&`).
    ///
    /// The conjunctive counterpart of [`Expr::has_any_json_keys`], and its
    /// vacuous mirror: an empty list is true for every operand, because nothing
    /// is missing from a document when nothing was asked for. The same
    /// asymmetry [`Expr::eq_any`] and [`Expr::ne_all`] carry over the empty
    /// array.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .column(Char::Id)
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::UserData).has_all_json_keys(["a", "b"]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "user_data" ?& ARRAY ['a','b']"#
    /// );
    ///
    /// let (sql, values) = query.build();
    /// assert_eq!(sql, r#"SELECT "id" FROM "character" WHERE "user_data" ?& $1"#);
    /// assert_eq!(values.0.len(), 1);
    /// ```
    // [spec:pgorm:req:sql.ast.expr.json]
    pub fn has_all_json_keys<S, I>(self, keys: I) -> SimpleExpr
    where
        S: Into<String>,
        I: IntoIterator<Item = S>,
    {
        self.bin_op(BinOper::HasAllJsonKeys, text_array(keys))
    }
}
