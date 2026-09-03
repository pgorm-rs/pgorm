use crate::{
    extension::{
        ExtensionCreateStatement, ExtensionDropOpt, ExtensionDropStatement, TypeAlterAddOpt,
        TypeAlterOpt, TypeAlterStatement, TypeAs, TypeCreateStatement, TypeDropOpt,
        TypeDropStatement, TypeRef,
    },
    *,
};
use std::ops::Deref;

// [spec:pgorm:req:sql.render.ident-quoting] (the double-quote pair; doubling in Iden::quoted)
const QUOTE: Quote = Quote(b'"', b'"');

// [spec:pgorm:def:sql.render]
// [spec:pgorm:req:sql.render.oracle] (the renderer whose every output the oracle in
// pgorm-query/tests/postgres/oracle.rs holds to the libpg_query grammar)
#[derive(Debug, Clone, Copy)]
pub struct QueryBuilder;

impl QueryBuilder {
    const fn quote(&self) -> Quote {
        QUOTE
    }

    // [spec:pgorm:req:sql.render.placeholders]
    pub const fn placeholder(&self) -> (&str, bool) {
        ("$", true)
    }

    // [spec:pgorm:req:sql.render.custom-expr] (top-level AsEnum rewritten to CAST)
    pub(crate) fn prepare_simple_expr(&self, simple_expr: &SimpleExpr, sql: &mut dyn SqlWriter) {
        match simple_expr {
            SimpleExpr::AsEnum(type_name, expr) => {
                let simple_expr = expr.clone().cast_as(SeaRc::clone(type_name));
                self.prepare_simple_expr_common(&simple_expr, sql);
            }
            _ => QueryBuilder::prepare_simple_expr_common(self, simple_expr, sql),
        }
    }

    fn prepare_select_distinct(&self, select_distinct: &SelectDistinct, sql: &mut dyn SqlWriter) {
        match select_distinct {
            SelectDistinct::All => write!(sql, "ALL").unwrap(),
            SelectDistinct::Distinct => write!(sql, "DISTINCT").unwrap(),
            SelectDistinct::DistinctOn(cols) => {
                write!(sql, "DISTINCT ON (").unwrap();
                cols.iter().fold(true, |first, column_ref| {
                    if !first {
                        write!(sql, ", ").unwrap();
                    }
                    self.prepare_column_ref(column_ref, sql);
                    false
                });
                write!(sql, ")").unwrap();
            }
        };
    }

    // [spec:pgorm:req:sql.render.select-order+2] (order expressions: ASC/DESC, NULLS, Order::Field)
    fn prepare_order_expr(&self, order_expr: &OrderExpr, sql: &mut dyn SqlWriter) {
        if !matches!(order_expr.order, Order::Field(_)) {
            self.prepare_simple_expr(&order_expr.expr, sql);
        }
        self.prepare_order(order_expr, sql);
        match order_expr.nulls {
            None => (),
            Some(NullOrdering::Last) => write!(sql, " NULLS LAST").unwrap(),
            Some(NullOrdering::First) => write!(sql, " NULLS FIRST").unwrap(),
        }
    }

    // [spec:pgorm:req:sql.render.string-escape] (single-quote wrapping; E-string when a backslash is present)
    fn write_string_quoted(&self, string: &str, buffer: &mut String) {
        let escaped = self.escape_string(string);
        let string = if escaped.find('\\').is_some() {
            "E'".to_owned() + &escaped + "'"
        } else {
            "'".to_owned() + &escaped + "'"
        };
        write!(buffer, "{string}").unwrap()
    }

    fn write_bytes(&self, bytes: &[u8], buffer: &mut String) {
        write!(buffer, "'\\x").unwrap();
        for b in bytes {
            write!(buffer, "{b:02X}").unwrap();
        }
        write!(buffer, "'").unwrap();
    }

    fn if_null_function(&self) -> &str {
        "COALESCE"
    }

    /// Prefix for tuples in VALUES list (e.g. ROW for Mysql)
    fn values_list_tuple_prefix(&self) -> &str {
        ""
    }

    /// Translate [`InsertStatement`] into SQL statement.
    // [spec:pgorm:req:sql.render.insert]
    pub(crate) fn prepare_insert_statement(
        &self,
        insert: &InsertStatement,
        sql: &mut dyn SqlWriter,
    ) {
        self.prepare_insert(sql);

        if let Some(table) = &insert.table {
            write!(sql, " INTO ").unwrap();
            self.prepare_named_table(table, sql);
        }

        if let (Some(num_rows), true, true) = (
            insert.default_values,
            insert.columns.is_empty(),
            insert.source.is_none(),
        ) {
            self.prepare_output(&insert.returning, sql);
            write!(sql, " ").unwrap();
            self.insert_default_values(num_rows, sql);
        } else {
            write!(sql, " ").unwrap();
            write!(sql, "(").unwrap();
            insert.columns.iter().fold(true, |first, col| {
                if !first {
                    write!(sql, ", ").unwrap()
                }
                col.prepare(sql.as_writer(), self.quote());
                false
            });
            write!(sql, ")").unwrap();

            self.prepare_output(&insert.returning, sql);

            if let Some(source) = &insert.source {
                write!(sql, " ").unwrap();
                match source {
                    InsertValueSource::Values(values) => {
                        write!(sql, "VALUES ").unwrap();
                        values.iter().fold(true, |first, row| {
                            if !first {
                                write!(sql, ", ").unwrap()
                            }
                            write!(sql, "(").unwrap();
                            row.iter().fold(true, |first, col| {
                                if !first {
                                    write!(sql, ", ").unwrap()
                                }
                                self.prepare_simple_expr(col, sql);
                                false
                            });
                            write!(sql, ")").unwrap();
                            false
                        });
                    }
                    InsertValueSource::Select(select_query) => {
                        self.prepare_select_statement(select_query.deref(), sql);
                    }
                }
            }
        }

        self.prepare_on_conflict(&insert.on_conflict, sql);

        self.prepare_returning(&insert.returning, sql);
    }

    fn prepare_union_statement(
        &self,
        union_type: UnionType,
        select_statement: &SelectStatement,
        sql: &mut dyn SqlWriter,
    ) {
        match union_type {
            UnionType::Intersect => write!(sql, " INTERSECT (").unwrap(),
            UnionType::Distinct => write!(sql, " UNION (").unwrap(),
            UnionType::Except => write!(sql, " EXCEPT (").unwrap(),
            UnionType::All => write!(sql, " UNION ALL (").unwrap(),
        }
        self.prepare_select_statement(select_statement, sql);
        write!(sql, ")").unwrap();
    }

    /// Translate [`SelectStatement`] into SQL statement.
    // [spec:pgorm:req:sql.render.select-order+2]
    // [spec:pgorm:sem:query.build.with.attach]
    pub(crate) fn prepare_select_statement(
        &self,
        select: &SelectStatement,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(with_clause) = &select.with {
            self.prepare_with_clause(with_clause, sql);
        }

        write!(sql, "SELECT ").unwrap();

        if let Some(distinct) = &select.distinct {
            self.prepare_select_distinct(distinct, sql);
            write!(sql, " ").unwrap();
        }

        select.selects.iter().fold(true, |first, expr| {
            if !first {
                write!(sql, ", ").unwrap()
            }
            self.prepare_select_expr(expr, sql);
            false
        });

        if !select.from.is_empty() {
            write!(sql, " FROM ").unwrap();
            select.from.iter().fold(true, |first, from_item| {
                if !first {
                    write!(sql, ", ").unwrap()
                }
                self.prepare_from_item(from_item, sql);
                false
            });
            self.prepare_index_hints(select, sql);
        }

        if !select.join.is_empty() {
            for expr in select.join.iter() {
                write!(sql, " ").unwrap();
                self.prepare_join_expr(expr, sql);
            }
        }

        self.prepare_condition(&select.r#where, "WHERE", sql);

        if !select.groups.is_empty() {
            write!(sql, " GROUP BY ").unwrap();
            select.groups.iter().fold(true, |first, expr| {
                if !first {
                    write!(sql, ", ").unwrap()
                }
                self.prepare_simple_expr(expr, sql);
                false
            });
        }

        self.prepare_condition(&select.having, "HAVING", sql);

        if let Some((name, query)) = &select.window {
            write!(sql, " WINDOW ").unwrap();
            name.prepare(sql.as_writer(), self.quote());
            write!(sql, " AS ").unwrap();
            self.prepare_window_spec(query, sql);
        }

        if !select.unions.is_empty() {
            select.unions.iter().for_each(|(union_type, query)| {
                self.prepare_union_statement(*union_type, query, sql);
            });
        }

        if !select.orders.is_empty() {
            write!(sql, " ORDER BY ").unwrap();
            select.orders.iter().fold(true, |first, expr| {
                if !first {
                    write!(sql, ", ").unwrap()
                }
                self.prepare_order_expr(expr, sql);
                false
            });
        }

        self.prepare_select_limit_offset(select, sql);

        if let Some(lock) = &select.lock {
            write!(sql, " ").unwrap();
            self.prepare_select_lock(lock, sql);
        }
    }

    // Translate the LIMIT and OFFSET expression in [`SelectStatement`]
    pub(crate) fn prepare_select_limit_offset(
        &self,
        select: &SelectStatement,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(limit) = &select.limit {
            write!(sql, " LIMIT ").unwrap();
            sql.push_param(limit.clone());
        }

        if let Some(offset) = &select.offset {
            write!(sql, " OFFSET ").unwrap();
            sql.push_param(offset.clone());
        }
    }

    /// Translate [`UpdateStatement`] into SQL statement.
    // [spec:pgorm:req:sql.render.update-delete+1] (UPDATE half)
    pub(crate) fn prepare_update_statement(
        &self,
        update: &UpdateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "UPDATE ").unwrap();

        if let Some(table) = &update.table {
            self.prepare_named_table(table, sql);
        }

        write!(sql, " SET ").unwrap();

        update.values.iter().fold(true, |first, row| {
            if !first {
                write!(sql, ", ").unwrap()
            }
            let (col, v) = row;
            col.prepare(sql.as_writer(), self.quote());
            write!(sql, " = ").unwrap();
            self.prepare_simple_expr(v, sql);
            false
        });

        self.prepare_output(&update.returning, sql);

        self.prepare_condition(&update.r#where, "WHERE", sql);

        self.prepare_returning(&update.returning, sql);
    }

    /// Translate [`DeleteStatement`] into SQL statement.
    // [spec:pgorm:req:sql.render.update-delete+1] (DELETE half)
    pub(crate) fn prepare_delete_statement(
        &self,
        delete: &DeleteStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DELETE ").unwrap();

        if let Some(table) = &delete.table {
            write!(sql, "FROM ").unwrap();
            self.prepare_named_table(table, sql);
        }

        self.prepare_output(&delete.returning, sql);

        self.prepare_condition(&delete.r#where, "WHERE", sql);

        self.prepare_returning(&delete.returning, sql);
    }

    // [spec:pgorm:sem:sql.render.empty-in+1]
    // [spec:pgorm:req:sql.render.subquery+1] (SubQuery/Tuple/Values expression arms)
    // [spec:pgorm:req:sql.render.custom-expr]
    fn prepare_simple_expr_common(&self, simple_expr: &SimpleExpr, sql: &mut dyn SqlWriter) {
        match simple_expr {
            SimpleExpr::Column(column_ref) => {
                self.prepare_column_ref(column_ref, sql);
            }
            SimpleExpr::Tuple(exprs) => {
                self.prepare_tuple(exprs, sql);
            }
            SimpleExpr::Unary(op, expr) => {
                self.prepare_un_oper(op, sql);
                write!(sql, " ").unwrap();
                let drop_expr_paren =
                    self.inner_expr_well_known_greater_precedence(expr, &(*op).into());
                if !drop_expr_paren {
                    write!(sql, "(").unwrap();
                }
                self.prepare_simple_expr(expr, sql);
                if !drop_expr_paren {
                    write!(sql, ")").unwrap();
                }
            }
            SimpleExpr::FunctionCall(func) => {
                self.prepare_function_name(&func.func, sql);
                self.prepare_function_arguments(func, sql);
            }
            SimpleExpr::Binary(left, op, right) => match (op, right.as_ref()) {
                (BinOper::In, SimpleExpr::Tuple(t)) if t.is_empty() => self.binary_expr(
                    &SimpleExpr::Value("a".into()),
                    &BinOper::Equal,
                    &SimpleExpr::Value("b".into()),
                    sql,
                ),
                (BinOper::NotIn, SimpleExpr::Tuple(t)) if t.is_empty() => self.binary_expr(
                    &SimpleExpr::Value("a".into()),
                    &BinOper::Equal,
                    &SimpleExpr::Value("a".into()),
                    sql,
                ),
                _ => self.binary_expr(left, op, right, sql),
            },
            SimpleExpr::SubQuery(oper, sel) => {
                if let Some(oper) = oper {
                    self.prepare_sub_query_oper(oper, sql);
                }
                write!(sql, "(").unwrap();
                sel.prepare_statement(sql);
                write!(sql, ")").unwrap();
            }
            // [spec:pgorm:req:sql.render.param-vs-inline+1]
            SimpleExpr::Value(val) => {
                sql.push_param(val.clone());
            }
            SimpleExpr::Values(list) => {
                write!(sql, "(").unwrap();
                list.iter().fold(true, |first, val| {
                    if !first {
                        write!(sql, ", ").unwrap();
                    }
                    sql.push_param(val.clone());
                    false
                });
                write!(sql, ")").unwrap();
            }
            SimpleExpr::Custom(s) => {
                write!(sql, "{s}").unwrap();
            }
            SimpleExpr::CustomWithExpr(expr, values) => {
                let (placeholder, numbered) = self.placeholder();
                let mut tokenizer = Tokenizer::new(expr).iter().peekable();
                let mut count = 0;
                while let Some(token) = tokenizer.next() {
                    match token {
                        Token::Punctuation(mark) if mark == placeholder => match tokenizer.peek() {
                            Some(Token::Punctuation(mark)) if mark == placeholder => {
                                write!(sql, "{mark}").unwrap();
                                tokenizer.next();
                            }
                            Some(Token::Unquoted(tok)) if numbered => {
                                if let Ok(num) = tok.parse::<usize>() {
                                    self.prepare_simple_expr(&values[num - 1], sql);
                                }
                                tokenizer.next();
                            }
                            _ => {
                                self.prepare_simple_expr(&values[count], sql);
                                count += 1;
                            }
                        },
                        _ => write!(sql, "{token}").unwrap(),
                    };
                }
            }
            SimpleExpr::Keyword(keyword) => {
                self.prepare_keyword(keyword, sql);
            }
            SimpleExpr::AsEnum(_, expr) => {
                self.prepare_simple_expr(expr, sql);
            }
            SimpleExpr::Case(case_stmt) => {
                self.prepare_case_statement(case_stmt, sql);
            }
            SimpleExpr::Constant(val) => {
                self.prepare_constant(val, sql);
            }
            SimpleExpr::LikePattern(like) => {
                self.prepare_like_expr(like, sql);
            }
        }
    }

    /// Translate a [`LikeExpr`] into the pattern and optional `ESCAPE` tail of a
    /// `LIKE` / `ILIKE`.
    // [spec:pgorm:def:sql.render.operators+3]
    fn prepare_like_expr(&self, like: &LikeExpr, sql: &mut dyn SqlWriter) {
        sql.push_param(like.pattern.clone().into());
        if let Some(escape) = like.escape {
            write!(sql, " ESCAPE ").unwrap();
            self.prepare_constant(&escape.into(), sql);
        }
    }

    /// Translate [`CaseStatement`] into SQL statement.
    fn prepare_case_statement(&self, stmts: &CaseStatement, sql: &mut dyn SqlWriter) {
        write!(sql, "(CASE").unwrap();

        let CaseStatement { when, r#else } = stmts;

        for case in when.iter() {
            write!(sql, " WHEN (").unwrap();
            self.prepare_condition_where(&case.condition, sql);
            write!(sql, ") THEN ").unwrap();

            self.prepare_simple_expr(&case.result, sql);
        }
        if let Some(r#else) = r#else.clone() {
            write!(sql, " ELSE ").unwrap();
            self.prepare_simple_expr(&r#else, sql);
        }

        write!(sql, " END)").unwrap();
    }

    /// Translate [`IndexHint`] into SQL statement.
    fn prepare_index_hints(&self, _select: &SelectStatement, _sql: &mut dyn SqlWriter) {}

    /// Translate [`LockType`] into SQL statement.
    // [spec:pgorm:sem:sql.render.locking]
    fn prepare_select_lock(&self, lock: &LockClause, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "FOR {}",
            match lock.r#type {
                LockType::Update => "UPDATE",
                LockType::NoKeyUpdate => "NO KEY UPDATE",
                LockType::Share => "SHARE",
                LockType::KeyShare => "KEY SHARE",
            }
        )
        .unwrap();
        if !lock.tables.is_empty() {
            write!(sql, " OF ").unwrap();
            lock.tables.iter().fold(true, |first, from_item| {
                if !first {
                    write!(sql, ", ").unwrap();
                }
                self.prepare_from_item(from_item, sql);
                false
            });
        }
        if let Some(behavior) = lock.behavior {
            match behavior {
                LockBehavior::Nowait => write!(sql, " NOWAIT").unwrap(),
                LockBehavior::SkipLocked => write!(sql, " SKIP LOCKED").unwrap(),
            }
        }
    }

    /// Translate [`SelectExpr`] into SQL statement.
    // [spec:pgorm:req:sql.render.window+3] (OVER attachment: named reference, inline spec, alias)
    fn prepare_select_expr(&self, select_expr: &SelectExpr, sql: &mut dyn SqlWriter) {
        self.prepare_simple_expr(&select_expr.expr, sql);
        match &select_expr.window {
            Some(WindowSelectType::Name(name)) => {
                write!(sql, " OVER ").unwrap();
                name.prepare(sql.as_writer(), self.quote())
            }
            Some(WindowSelectType::Query(window)) => {
                write!(sql, " OVER ").unwrap();
                self.prepare_window_spec(window, sql);
            }
            None => {}
        };

        if let Some(alias) = &select_expr.alias {
            write!(sql, " AS ").unwrap();
            alias.prepare(sql.as_writer(), self.quote());
        }
    }

    /// Translate [`JoinExpr`] into SQL statement.
    // [spec:pgorm:req:sql.render.joins+2]
    fn prepare_join_expr(&self, join_expr: &JoinExpr, sql: &mut dyn SqlWriter) {
        match &join_expr.join {
            JoinKind::Cross => write!(sql, "CROSS JOIN").unwrap(),
            JoinKind::Qualified(join_type, _) => self.prepare_join_type(join_type, sql),
        }
        write!(sql, " ").unwrap();
        self.prepare_join_from_item(join_expr, sql);
        if let JoinKind::Qualified(_, on) = &join_expr.join {
            self.prepare_join_on(on, sql);
        }
    }

    fn prepare_join_from_item(&self, join_expr: &JoinExpr, sql: &mut dyn SqlWriter) {
        if join_expr.lateral {
            write!(sql, "LATERAL ").unwrap();
        }
        self.prepare_from_item(&join_expr.table, sql);
    }

    /// Translate [`FromItem`] into SQL statement.
    // [spec:pgorm:req:sql.render.subquery+1] (value-bearing from items carry mandatory aliases)
    fn prepare_from_item(&self, from_item: &FromItem, sql: &mut dyn SqlWriter) {
        match from_item {
            FromItem::Table(table) => self.prepare_named_table(table, sql),
            FromItem::SubQuery(query, alias) => {
                write!(sql, "(").unwrap();
                self.prepare_select_statement(query, sql);
                write!(sql, ")").unwrap();
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            FromItem::ValuesList(values, alias) => {
                write!(sql, "(").unwrap();
                self.prepare_values_list(values, sql);
                write!(sql, ")").unwrap();
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            FromItem::FunctionCall(func, alias) => {
                self.prepare_function_name(&func.func, sql);
                self.prepare_function_arguments(func, sql);
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
        }
    }

    fn prepare_column_ref(&self, column_ref: &ColumnRef, sql: &mut dyn SqlWriter) {
        match column_ref {
            ColumnRef::Column(column) => column.prepare(sql.as_writer(), self.quote()),
            ColumnRef::TableColumn(table, column) => {
                table.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                column.prepare(sql.as_writer(), self.quote());
            }
            ColumnRef::SchemaTableColumn(schema, table, column) => {
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                table.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                column.prepare(sql.as_writer(), self.quote());
            }
            ColumnRef::Asterisk => {
                write!(sql, "*").unwrap();
            }
            ColumnRef::TableAsterisk(table) => {
                table.prepare(sql.as_writer(), self.quote());
                write!(sql, ".*").unwrap();
            }
        };
    }

    /// Translate [`UnOper`] into SQL statement.
    // [spec:pgorm:def:sql.render.operators+3] (the only unary operator: NOT)
    fn prepare_un_oper(&self, un_oper: &UnOper, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match un_oper {
                UnOper::Not => "NOT",
            }
        )
        .unwrap();
    }

    // [spec:pgorm:def:sql.render.operators+3]
    fn prepare_bin_oper(&self, bin_oper: &BinOper, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match bin_oper {
                BinOper::And => "AND",
                BinOper::Or => "OR",
                BinOper::Like => "LIKE",
                BinOper::NotLike => "NOT LIKE",
                BinOper::Is => "IS",
                BinOper::IsNot => "IS NOT",
                BinOper::In => "IN",
                BinOper::NotIn => "NOT IN",
                BinOper::Between => "BETWEEN",
                BinOper::NotBetween => "NOT BETWEEN",
                BinOper::Equal => "=",
                BinOper::NotEqual => "<>",
                BinOper::SmallerThan => "<",
                BinOper::GreaterThan => ">",
                BinOper::SmallerThanOrEqual => "<=",
                BinOper::GreaterThanOrEqual => ">=",
                BinOper::Add => "+",
                BinOper::Sub => "-",
                BinOper::Mul => "*",
                BinOper::Div => "/",
                BinOper::Mod => "%",
                BinOper::LShift => "<<",
                BinOper::RShift => ">>",
                BinOper::As => "AS",
                BinOper::Custom(raw) => raw,
                BinOper::ILike => "ILIKE",
                BinOper::NotILike => "NOT ILIKE",
                BinOper::Matches => "@@",
                BinOper::Contains => "@>",
                BinOper::Contained => "<@",
                BinOper::Concatenate => "||",
                BinOper::Overlap => "&&",
                BinOper::Similarity => "%",
                BinOper::WordSimilarity => "<%",
                BinOper::StrictWordSimilarity => "<<%",
                BinOper::SimilarityDistance => "<->",
                BinOper::WordSimilarityDistance => "<<->",
                BinOper::StrictWordSimilarityDistance => "<<<->",
                BinOper::GetJsonField => "->",
                BinOper::CastJsonField => "->>",
                BinOper::GetJsonPath => "#>",
                BinOper::CastJsonPath => "#>>",
                BinOper::HasJsonKey => "?",
                BinOper::HasAnyJsonKeys => "?|",
                BinOper::HasAllJsonKeys => "?&",
                BinOper::Regex => "~",
                BinOper::RegexCaseInsensitive => "~*",
                BinOper::EuclideanDistance => "<->",
                BinOper::NegativeInnerProduct => "<#>",
                BinOper::CosineDistance => "<=>",
            }
        )
        .unwrap();
    }

    /// Translate [`SubQueryOper`] into SQL statement.
    fn prepare_sub_query_oper(&self, oper: &SubQueryOper, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match oper {
                SubQueryOper::Exists => "EXISTS",
                SubQueryOper::Any => "ANY",
                SubQueryOper::Some => "SOME",
                SubQueryOper::All => "ALL",
            }
        )
        .unwrap();
    }

    /// Translate [`Function`] into SQL statement.
    fn prepare_function_name(&self, function: &Function, sql: &mut dyn SqlWriter) {
        if let Function::Custom(iden) = function {
            iden.unquoted(sql.as_writer());
        } else {
            write!(
                sql,
                "{}",
                match function {
                    Function::Max => "MAX",
                    Function::Min => "MIN",
                    Function::Sum => "SUM",
                    Function::Avg => "AVG",
                    Function::Abs => "ABS",
                    Function::Coalesce => "COALESCE",
                    Function::Count => "COUNT",
                    Function::IfNull => self.if_null_function(),
                    Function::CharLength => self.char_length_function(),
                    Function::Cast => "CAST",
                    Function::Lower => "LOWER",
                    Function::Upper => "UPPER",
                    Function::BitAnd => "BIT_AND",
                    Function::BitOr => "BIT_OR",
                    Function::Custom(_) => "",
                    Function::Random => self.random_function(),
                    Function::Round => "ROUND",
                    Function::ToTsquery => "TO_TSQUERY",
                    Function::ToTsvector => "TO_TSVECTOR",
                    Function::PhrasetoTsquery => "PHRASETO_TSQUERY",
                    Function::PlaintoTsquery => "PLAINTO_TSQUERY",
                    Function::WebsearchToTsquery => "WEBSEARCH_TO_TSQUERY",
                    Function::TsRank => "TS_RANK",
                    Function::TsRankCd => "TS_RANK_CD",
                    Function::StartsWith => "STARTS_WITH",
                    Function::GenRandomUUID => "GEN_RANDOM_UUID",
                    Function::Any => "ANY",
                    Function::Some => "SOME",
                    Function::All => "ALL",
                }
            )
            .unwrap();
        }
    }

    fn prepare_function_arguments(&self, func: &FunctionCall, sql: &mut dyn SqlWriter) {
        write!(sql, "(").unwrap();
        for (i, expr) in func.args.iter().enumerate() {
            if i != 0 {
                write!(sql, ", ").unwrap();
            }
            if func.mods[i].distinct {
                write!(sql, "DISTINCT ").unwrap();
            }
            self.prepare_simple_expr(expr, sql);
        }
        write!(sql, ")").unwrap();
    }

    pub(crate) fn prepare_with_query(&self, query: &WithQuery, sql: &mut dyn SqlWriter) {
        self.prepare_with_clause(&query.with_clause, sql);
        query.query.prepare_statement(sql);
    }

    // [spec:pgorm:req:sql.render.cte+2]
    pub(crate) fn prepare_with_clause(&self, with_clause: &AnyWithClause, sql: &mut dyn SqlWriter) {
        match with_clause {
            AnyWithClause::Plain(plain) => {
                write!(sql, "WITH ").unwrap();
                for (i, cte) in plain.ctes().enumerate() {
                    if i != 0 {
                        write!(sql, ", ").unwrap();
                    }
                    self.prepare_with_query_clause_common_table(cte, sql);
                }
            }
            AnyWithClause::Recursive(recursive) => {
                write!(sql, "WITH RECURSIVE ").unwrap();
                self.prepare_with_query_clause_common_table(&recursive.cte, sql);
                self.prepare_with_clause_recursive_options(recursive, sql);
            }
        }
    }

    fn prepare_with_clause_recursive_options(
        &self,
        with_clause: &RecursiveWithClause,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(search) = &with_clause.search {
            write!(
                sql,
                "SEARCH {} FIRST BY ",
                match &search.order {
                    SearchOrder::BREADTH => "BREADTH",
                    SearchOrder::DEPTH => "DEPTH",
                }
            )
            .unwrap();

            self.prepare_simple_expr(&search.expr, sql);

            write!(sql, " SET ").unwrap();

            search.alias.prepare(sql.as_writer(), self.quote());
            write!(sql, " ").unwrap();
        }
        if let Some(cycle) = &with_clause.cycle {
            write!(sql, "CYCLE ").unwrap();

            self.prepare_simple_expr(&cycle.expr, sql);

            write!(sql, " SET ").unwrap();

            cycle.set_as.prepare(sql.as_writer(), self.quote());
            write!(sql, " USING ").unwrap();
            cycle.using.prepare(sql.as_writer(), self.quote());
            write!(sql, " ").unwrap();
        }
    }

    fn prepare_with_query_clause_common_table(
        &self,
        cte: &CommonTableExpression,
        sql: &mut dyn SqlWriter,
    ) {
        cte.table_name.prepare(sql.as_writer(), self.quote());

        if cte.cols.is_empty() {
            write!(sql, " ").unwrap();
        } else {
            write!(sql, " (").unwrap();

            let mut col_first = true;
            for col in &cte.cols {
                if !col_first {
                    write!(sql, ", ").unwrap();
                }
                col_first = false;
                col.prepare(sql.as_writer(), self.quote());
            }

            write!(sql, ") ").unwrap();
        }

        write!(sql, "AS ").unwrap();

        self.prepare_with_query_clause_materialization(cte, sql);

        write!(sql, "(").unwrap();

        cte.query.prepare_statement(sql);

        write!(sql, ") ").unwrap();
    }

    fn prepare_with_query_clause_materialization(
        &self,
        cte: &CommonTableExpression,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(materialized) = cte.materialized {
            write!(
                sql,
                "{} MATERIALIZED ",
                if materialized { "" } else { "NOT" }
            )
            .unwrap()
        }
    }

    fn prepare_insert(&self, sql: &mut dyn SqlWriter) {
        write!(sql, "INSERT").unwrap();
    }

    /// Translate [`JoinType`] into SQL statement.
    fn prepare_join_type(&self, join_type: &JoinType, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match join_type {
                JoinType::Join => "JOIN",
                JoinType::InnerJoin => "INNER JOIN",
                JoinType::LeftJoin => "LEFT JOIN",
                JoinType::RightJoin => "RIGHT JOIN",
                JoinType::FullOuterJoin => "FULL OUTER JOIN",
            }
        )
        .unwrap()
    }

    /// Translate [`JoinOn`] into SQL statement.
    // [spec:pgorm:req:sql.render.joins+2]
    fn prepare_join_on(&self, join_on: &JoinOn, sql: &mut dyn SqlWriter) {
        match join_on {
            JoinOn::Condition(c) => self.prepare_condition(c, "ON", sql),
        }
    }

    /// Translate [`Order`] into SQL statement.
    fn prepare_order(&self, order_expr: &OrderExpr, sql: &mut dyn SqlWriter) {
        match &order_expr.order {
            Order::Asc => write!(sql, " ASC").unwrap(),
            Order::Desc => write!(sql, " DESC").unwrap(),
            Order::Field(values) => self.prepare_field_order(order_expr, values, sql),
        }
    }

    /// Translate [`Order::Field`] into SQL statement
    fn prepare_field_order(
        &self,
        order_expr: &OrderExpr,
        values: &Values,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CASE ").unwrap();
        let mut i = 0;
        for value in &values.0 {
            write!(sql, "WHEN ").unwrap();
            self.prepare_simple_expr(&order_expr.expr, sql);
            write!(sql, "=").unwrap();
            let value = self.value_to_string(value);
            write!(sql, "{value}").unwrap();
            write!(sql, " THEN {i} ").unwrap();
            i += 1;
        }
        write!(sql, "ELSE {i} END").unwrap();
    }

    /// Write [`Value`] inline.
    // [spec:pgorm:req:sql.render.param-vs-inline+1] (Constant is always rendered inline)
    fn prepare_constant(&self, value: &Value, sql: &mut dyn SqlWriter) {
        let string = self.value_to_string(value);
        write!(sql, "{string}").unwrap();
    }

    /// Translate a `&[ValueTuple]` into a VALUES list.
    fn prepare_values_list(&self, value_tuples: &[ValueTuple], sql: &mut dyn SqlWriter) {
        write!(sql, "VALUES ").unwrap();
        value_tuples.iter().fold(true, |first, value_tuple| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            write!(sql, "{}", self.values_list_tuple_prefix()).unwrap();
            write!(sql, "(").unwrap();
            value_tuple.clone().into_iter().fold(true, |first, value| {
                if !first {
                    write!(sql, ", ").unwrap();
                }
                sql.push_param(value);
                false
            });

            write!(sql, ")").unwrap();
            false
        });
    }

    /// Translate [`SimpleExpr::Tuple`] into SQL statement.
    fn prepare_tuple(&self, exprs: &[SimpleExpr], sql: &mut dyn SqlWriter) {
        write!(sql, "(").unwrap();
        for (i, expr) in exprs.iter().enumerate() {
            if i != 0 {
                write!(sql, ", ").unwrap();
            }
            self.prepare_simple_expr(expr, sql);
        }
        write!(sql, ")").unwrap();
    }

    /// Translate [`Keyword`] into SQL statement.
    fn prepare_keyword(&self, keyword: &Keyword, sql: &mut dyn SqlWriter) {
        match keyword {
            Keyword::Null => write!(sql, "NULL").unwrap(),
            Keyword::CurrentDate => write!(sql, "CURRENT_DATE").unwrap(),
            Keyword::CurrentTime => write!(sql, "CURRENT_TIME").unwrap(),
            Keyword::CurrentTimestamp => write!(sql, "CURRENT_TIMESTAMP").unwrap(),
            Keyword::Custom(iden) => iden.unquoted(sql.as_writer()),
        }
    }

    /// Convert a SQL value into syntax-specific string
    // [spec:pgorm:sem:sql.value.render]
    // [spec:pgorm:def:sql.render.value-literals+2]
    pub(crate) fn value_to_string(&self, v: &Value) -> String {
        let mut s = String::new();
        match v {
            Value::Bool(None)
            | Value::TinyInt(None)
            | Value::SmallInt(None)
            | Value::Int(None)
            | Value::BigInt(None)
            | Value::Unsigned(None)
            | Value::BigUnsigned(None)
            | Value::Float(None)
            | Value::Double(None)
            | Value::String(None)
            | Value::Char(None)
            | Value::Bytes(None) => write!(s, "NULL").unwrap(),
            Value::Json(None) => write!(s, "NULL").unwrap(),
            Value::ChronoDate(None) => write!(s, "NULL").unwrap(),
            Value::ChronoTime(None) => write!(s, "NULL").unwrap(),
            Value::ChronoDateTime(None) => write!(s, "NULL").unwrap(),
            Value::ChronoDateTimeUtc(None) => write!(s, "NULL").unwrap(),
            Value::ChronoDateTimeLocal(None) => write!(s, "NULL").unwrap(),
            Value::ChronoDateTimeWithTimeZone(None) => write!(s, "NULL").unwrap(),
            Value::Decimal(None) => write!(s, "NULL").unwrap(),
            Value::Uuid(None) => write!(s, "NULL").unwrap(),
            Value::IpNetwork(None) => write!(s, "NULL").unwrap(),
            Value::MacAddress(None) => write!(s, "NULL").unwrap(),
            Value::Array(_, None) => write!(s, "NULL").unwrap(),
            Value::Vector(None) => write!(s, "NULL").unwrap(),
            Value::Bool(Some(b)) => write!(s, "{}", if *b { "TRUE" } else { "FALSE" }).unwrap(),
            Value::TinyInt(Some(v)) => write!(s, "{v}").unwrap(),
            Value::SmallInt(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Int(Some(v)) => write!(s, "{v}").unwrap(),
            Value::BigInt(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Unsigned(Some(v)) => write!(s, "{v}").unwrap(),
            Value::BigUnsigned(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Float(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Double(Some(v)) => write!(s, "{v}").unwrap(),
            Value::String(Some(v)) => self.write_string_quoted(v, &mut s),
            Value::Char(Some(v)) => self.write_string_quoted(v.encode_utf8(&mut [0u8; 4]), &mut s),
            Value::Bytes(Some(v)) => self.write_bytes(v, &mut s),
            Value::Json(Some(v)) => self.write_string_quoted(&v.to_string(), &mut s),
            Value::ChronoDate(Some(v)) => write!(s, "'{}'", v.format("%Y-%m-%d")).unwrap(),
            Value::ChronoTime(Some(v)) => write!(s, "'{}'", v.format("%H:%M:%S")).unwrap(),
            Value::ChronoDateTime(Some(v)) => {
                write!(s, "'{}'", v.format("%Y-%m-%d %H:%M:%S")).unwrap()
            }
            Value::ChronoDateTimeUtc(Some(v)) => {
                write!(s, "'{}'", v.format("%Y-%m-%d %H:%M:%S %:z")).unwrap()
            }
            Value::ChronoDateTimeLocal(Some(v)) => {
                write!(s, "'{}'", v.format("%Y-%m-%d %H:%M:%S %:z")).unwrap()
            }
            Value::ChronoDateTimeWithTimeZone(Some(v)) => {
                write!(s, "'{}'", v.format("%Y-%m-%d %H:%M:%S %:z")).unwrap()
            }
            Value::Decimal(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Uuid(Some(v)) => write!(s, "'{v}'").unwrap(),
            Value::Array(ty, Some(v)) if v.is_empty() => match ty.source_type_name() {
                Some(element) => write!(s, "ARRAY []::{element}[]").unwrap(),
                None => write!(s, "ARRAY []").unwrap(),
            },
            Value::Array(_, Some(v)) => write!(
                s,
                "ARRAY [{}]",
                v.iter()
                    .map(|element| self.value_to_string(element))
                    .collect::<Vec<String>>()
                    .join(",")
            )
            .unwrap(),
            Value::Vector(Some(v)) => {
                write!(s, "'[").unwrap();
                for (i, &element) in v.as_slice().iter().enumerate() {
                    if i != 0 {
                        write!(s, ",").unwrap();
                    }
                    write!(s, "{element}").unwrap();
                }
                write!(s, "]'").unwrap();
            }
            Value::IpNetwork(Some(v)) => write!(s, "'{v}'").unwrap(),
            Value::MacAddress(Some(v)) => write!(s, "'{v}'").unwrap(),
        };
        s
    }

    #[doc(hidden)]
    /// Write ON CONFLICT expression
    // [spec:pgorm:req:sql.render.on-conflict+1]
    fn prepare_on_conflict(&self, on_conflict: &Option<OnConflict>, sql: &mut dyn SqlWriter) {
        let Some(on_conflict) = on_conflict else {
            return;
        };
        self.prepare_on_conflict_keywords(sql);
        match on_conflict {
            OnConflict::AnyDoNothing => write!(sql, " DO NOTHING").unwrap(),
            OnConflict::Targeted { target, action } => {
                self.prepare_on_conflict_target(target, sql);
                self.prepare_on_conflict_action(action, sql);
            }
        }
    }

    #[doc(hidden)]
    /// Write ON CONFLICT target
    fn prepare_on_conflict_target(&self, target: &ConflictTarget, sql: &mut dyn SqlWriter) {
        write!(sql, " (").unwrap();
        target.elements().fold(true, |first, element| {
            if !first {
                write!(sql, ", ").unwrap()
            }
            match element {
                ConflictElement::Column(col) => {
                    col.prepare(sql.as_writer(), self.quote());
                }
                ConflictElement::Expr(expr) => {
                    self.prepare_simple_expr(expr, sql);
                }
            }
            false
        });
        write!(sql, ")").unwrap();
        self.prepare_on_conflict_condition(&target.filter, sql);
    }

    #[doc(hidden)]
    /// Write ON CONFLICT action
    fn prepare_on_conflict_action(&self, action: &ConflictAction, sql: &mut dyn SqlWriter) {
        match action {
            ConflictAction::DoNothing => write!(sql, " DO NOTHING").unwrap(),
            ConflictAction::Update { sets, filter } => {
                self.prepare_on_conflict_do_update_keywords(sql);
                sets.iter().fold(true, |first, assignment| {
                    if !first {
                        write!(sql, ", ").unwrap()
                    }
                    match assignment {
                        ConflictAssignment::Column(col) => {
                            col.prepare(sql.as_writer(), self.quote());
                            write!(sql, " = ").unwrap();
                            self.prepare_on_conflict_excluded_table(col, sql);
                        }
                        ConflictAssignment::Expr(col, expr) => {
                            col.prepare(sql.as_writer(), self.quote());
                            write!(sql, " = ").unwrap();
                            self.prepare_simple_expr(expr, sql);
                        }
                    }
                    false
                });
                self.prepare_on_conflict_condition(filter, sql);
            }
        }
    }

    #[doc(hidden)]
    /// Write ON CONFLICT keywords
    fn prepare_on_conflict_keywords(&self, sql: &mut dyn SqlWriter) {
        write!(sql, " ON CONFLICT").unwrap();
    }

    #[doc(hidden)]
    /// Write ON CONFLICT keywords
    fn prepare_on_conflict_do_update_keywords(&self, sql: &mut dyn SqlWriter) {
        write!(sql, " DO UPDATE SET ").unwrap();
    }

    #[doc(hidden)]
    /// Write ON CONFLICT update action by retrieving value from the excluded table
    fn prepare_on_conflict_excluded_table(&self, col: &DynIden, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}excluded{}",
            self.quote().left(),
            self.quote().right()
        )
        .unwrap();
        write!(sql, ".").unwrap();
        col.prepare(sql.as_writer(), self.quote());
    }

    #[doc(hidden)]
    /// Write ON CONFLICT conditions
    fn prepare_on_conflict_condition(&self, filter: &Option<Condition>, sql: &mut dyn SqlWriter) {
        if let Some(condition) = filter {
            write!(sql, " WHERE ").unwrap();
            self.prepare_condition_where(condition, sql);
        }
    }

    #[doc(hidden)]
    /// Hook to insert "OUTPUT" expressions.
    // [spec:pgorm:req:sql.render.returning] (pre-source OUTPUT hook is a no-op)
    fn prepare_output(&self, _returning: &Option<ReturningClause>, _sql: &mut dyn SqlWriter) {}

    #[doc(hidden)]
    /// Hook to insert "RETURNING" statements.
    // [spec:pgorm:req:sql.render.returning]
    fn prepare_returning(&self, returning: &Option<ReturningClause>, sql: &mut dyn SqlWriter) {
        if let Some(returning) = returning {
            write!(sql, " RETURNING ").unwrap();
            match &returning {
                ReturningClause::All => write!(sql, "*").unwrap(),
                ReturningClause::Columns(cols) => {
                    cols.iter().fold(true, |first, column_ref| {
                        if !first {
                            write!(sql, ", ").unwrap()
                        }
                        self.prepare_column_ref(column_ref, sql);
                        false
                    });
                }
                ReturningClause::Exprs(exprs) => {
                    exprs.iter().fold(true, |first, expr| {
                        if !first {
                            write!(sql, ", ").unwrap()
                        }
                        self.prepare_simple_expr(expr, sql);
                        false
                    });
                }
            }
        }
    }

    #[doc(hidden)]
    /// Translate a condition to a "WHERE" clause.
    // [spec:pgorm:req:sql.render.condition-chain+1]
    fn prepare_condition(
        &self,
        condition: &ConditionHolder,
        keyword: &str,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(c) = &condition.contents {
            write!(sql, " {keyword} ").unwrap();
            self.prepare_condition_where(c, sql);
        }
    }

    #[doc(hidden)]
    /// Translate part of a condition to part of a "WHERE" clause.
    fn prepare_condition_where(&self, condition: &Condition, sql: &mut dyn SqlWriter) {
        let simple_expr = condition.to_simple_expr();
        self.prepare_simple_expr(&simple_expr, sql);
    }

    #[doc(hidden)]
    /// Translate [`Frame`] into SQL statement.
    // [spec:pgorm:req:sql.render.window+3] (frame bounds)
    fn prepare_frame(&self, frame: &Frame, sql: &mut dyn SqlWriter) {
        match *frame {
            Frame::UnboundedPreceding => write!(sql, "UNBOUNDED PRECEDING").unwrap(),
            Frame::Preceding(v) => {
                sql.push_param(v.into());
                write!(sql, " PRECEDING").unwrap();
            }
            Frame::CurrentRow => write!(sql, "CURRENT ROW").unwrap(),
            Frame::Following(v) => {
                sql.push_param(v.into());
                write!(sql, " FOLLOWING").unwrap();
            }
            Frame::UnboundedFollowing => write!(sql, "UNBOUNDED FOLLOWING").unwrap(),
        }
    }

    #[doc(hidden)]
    /// Translate a [`WindowStatement`] into the parenthesized window
    /// specification PostgreSQL requires after `OVER` and after `WINDOW n AS`.
    // [spec:pgorm:req:sql.render.window+3]
    fn prepare_window_spec(&self, window: &WindowStatement, sql: &mut dyn SqlWriter) {
        write!(sql, "( ").unwrap();
        self.prepare_window_statement(window, sql);
        write!(sql, " )").unwrap();
    }

    #[doc(hidden)]
    /// Translate [`WindowStatement`] into SQL statement.
    // [spec:pgorm:req:sql.render.window+3]
    fn prepare_window_statement(&self, window: &WindowStatement, sql: &mut dyn SqlWriter) {
        if !window.partition_by.is_empty() {
            write!(sql, "PARTITION BY ").unwrap();
            window.partition_by.iter().fold(true, |first, expr| {
                if !first {
                    write!(sql, ", ").unwrap()
                }
                self.prepare_simple_expr(expr, sql);
                false
            });
        }

        if !window.order_by.is_empty() {
            write!(sql, " ORDER BY ").unwrap();
            window.order_by.iter().fold(true, |first, expr| {
                if !first {
                    write!(sql, ", ").unwrap()
                }
                self.prepare_order_expr(expr, sql);
                false
            });
        }

        if let Some(frame) = &window.frame {
            match frame.r#type {
                FrameType::Range => write!(sql, " RANGE ").unwrap(),
                FrameType::Rows => write!(sql, " ROWS ").unwrap(),
            };
            if let Some(end) = &frame.end {
                write!(sql, "BETWEEN ").unwrap();
                self.prepare_frame(&frame.start, sql);
                write!(sql, " AND ").unwrap();
                self.prepare_frame(end, sql);
            } else {
                self.prepare_frame(&frame.start, sql);
            }
        }
    }

    #[doc(hidden)]
    /// Translate a binary expr to SQL.
    // [spec:pgorm:req:sql.render.parens]
    fn binary_expr(
        &self,
        left: &SimpleExpr,
        op: &BinOper,
        right: &SimpleExpr,
        sql: &mut dyn SqlWriter,
    ) {
        // If left has higher precedence than op, we can drop parentheses around left
        let drop_left_higher_precedence =
            self.inner_expr_well_known_greater_precedence(left, &(*op).into());

        // Figure out if left associativity rules allow us to drop the left parenthesis
        let drop_left_assoc = left.is_binary()
            && op == left.get_bin_oper().unwrap()
            && self.well_known_left_associative(op);

        let left_paren = !drop_left_higher_precedence && !drop_left_assoc;
        if left_paren {
            write!(sql, "(").unwrap();
        }
        match (op, left) {
            // [spec:pgorm:req:sql.render.cast-param-type]
            (BinOper::As, SimpleExpr::Value(value)) => sql.push_param_source_typed(value.clone()),
            _ => self.prepare_simple_expr(left, sql),
        }
        if left_paren {
            write!(sql, ")").unwrap();
        }

        write!(sql, " ").unwrap();
        self.prepare_bin_oper(op, sql);
        write!(sql, " ").unwrap();

        // If right has higher precedence than op, we can drop parentheses around right
        let drop_right_higher_precedence =
            self.inner_expr_well_known_greater_precedence(right, &(*op).into());

        let op_as_oper = Oper::BinOper(*op);
        // Due to representation of trinary op between as nested binary ops.
        let drop_right_between_hack = op_as_oper.is_between()
            && right.is_binary()
            && matches!(right.get_bin_oper(), Some(&BinOper::And));

        // Due to custom representation of casting AS datatype
        let drop_right_as_hack = (op == &BinOper::As) && matches!(right, SimpleExpr::Custom(_));

        let right_paren =
            !drop_right_higher_precedence && !drop_right_between_hack && !drop_right_as_hack;
        if right_paren {
            write!(sql, "(").unwrap();
        }
        self.prepare_simple_expr(right, sql);
        if right_paren {
            write!(sql, ")").unwrap();
        }
    }

    #[doc(hidden)]
    /// The name of the function that returns the char length.
    fn char_length_function(&self) -> &str {
        "CHAR_LENGTH"
    }

    #[doc(hidden)]
    /// The name of the function that returns a random number
    fn random_function(&self) -> &str {
        // Returning it with parens as part of the name because the tuple preparer can't deal with empty lists
        "RANDOM"
    }

    /// The keywords for insert default row.
    fn insert_default_keyword(&self) -> &str {
        "(DEFAULT)"
    }

    /// Write insert default rows expression.
    fn insert_default_values(&self, num_rows: u32, sql: &mut dyn SqlWriter) {
        write!(sql, "VALUES ").unwrap();
        (0..num_rows).fold(true, |first, _| {
            if !first {
                write!(sql, ", ").unwrap()
            }
            write!(sql, "{}", self.insert_default_keyword()).unwrap();
            false
        });
    }

    /// Write TRUE constant
    pub fn prepare_constant_true(&self, sql: &mut dyn SqlWriter) {
        self.prepare_constant(&true.into(), sql);
    }

    /// Write FALSE constant
    pub fn prepare_constant_false(&self, sql: &mut dyn SqlWriter) {
        self.prepare_constant(&false.into(), sql);
    }

    // COMMON
    // START: impl that ought not be here
    // [spec:pgorm:sem:sql.ddl.panics+4]
    // [spec:pgorm:def:sql.render.ddl.types+3] (serial family for auto-increment columns)
    fn prepare_column_auto_increment(&self, column_type: &ColumnType, sql: &mut dyn SqlWriter) {
        match column_type.serial_spelling() {
            Some(serial) => write!(sql, "{serial}").unwrap(),
            None => self.prepare_column_type(column_type, sql),
        }
    }

    fn prepare_column_type_check_auto_increment(
        &self,
        column_def: &ColumnDef,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(column_type) = &column_def.types {
            let is_auto_increment = column_def
                .spec
                .iter()
                .position(|s| matches!(s, ColumnSpec::AutoIncrement));
            if is_auto_increment.is_some() {
                write!(sql, " ").unwrap();
                self.prepare_column_auto_increment(column_type, sql);
            } else {
                write!(sql, " ").unwrap();
                self.prepare_column_type(column_type, sql);
            }
        }
    }

    // [spec:pgorm:req:sql.ddl.column-def+3]
    fn prepare_column_def_common<F>(&self, column_def: &ColumnDef, sql: &mut dyn SqlWriter, f: F)
    where
        F: Fn(&ColumnDef, &mut dyn SqlWriter),
    {
        column_def.name.prepare(sql.as_writer(), self.quote());

        f(column_def, sql);

        for column_spec in column_def.spec.iter() {
            if let ColumnSpec::AutoIncrement = column_spec {
                continue;
            }
            if let ColumnSpec::Comment(_) = column_spec {
                continue;
            }
            write!(sql, " ").unwrap();
            self.prepare_column_spec(column_spec, sql);
        }
    }
    // END: lol

    fn prepare_column_def(&self, column_def: &ColumnDef, sql: &mut dyn SqlWriter) {
        let f = |column_def: &ColumnDef, sql: &mut dyn SqlWriter| {
            self.prepare_column_type_check_auto_increment(column_def, sql);
        };
        self.prepare_column_def_common(column_def, sql, f);
    }

    // [spec:pgorm:req:sql.ddl.column-types+3]
    // [spec:pgorm:def:sql.render.ddl.types+3]
    fn prepare_column_type(&self, column_type: &ColumnType, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match column_type {
                ColumnType::Char(length) => match length {
                    Some(length) => format!("char({length})"),
                    None => "char".into(),
                },
                ColumnType::String(length) => match length {
                    StringLen::N(length) => format!("varchar({length})"),
                    _ => "varchar".into(),
                },
                ColumnType::Text => "text".into(),
                ColumnType::SmallInteger => "smallint".into(),
                ColumnType::Integer => "integer".into(),
                ColumnType::BigInteger => "bigint".into(),
                ColumnType::Float => "real".into(),
                ColumnType::Double => "double precision".into(),
                ColumnType::Decimal(precision) => match precision {
                    Some((precision, scale)) => format!("decimal({precision}, {scale})"),
                    None => "decimal".into(),
                },
                ColumnType::Timestamp => "timestamp".into(),
                ColumnType::TimestampWithTimeZone => "timestamp with time zone".into(),
                ColumnType::Time => "time".into(),
                ColumnType::Date => "date".into(),
                ColumnType::Interval(spec) => {
                    let mut typ = "interval".to_string();
                    match spec {
                        IntervalSpec::Any(None) => {}
                        IntervalSpec::Any(Some(precision)) => {
                            write!(typ, "({precision})").unwrap();
                        }
                        IntervalSpec::Fields(fields) => write!(typ, " {fields}").unwrap(),
                    }
                    typ
                }
                ColumnType::Bytea => "bytea".into(),
                ColumnType::Bit(length) => {
                    match length {
                        Some(length) => format!("bit({length})"),
                        None => "bit".into(),
                    }
                }
                ColumnType::VarBit(length) => {
                    format!("varbit({length})")
                }
                ColumnType::Boolean => "bool".into(),
                ColumnType::Money => "money".into(),
                ColumnType::Json => "json".into(),
                ColumnType::JsonBinary => "jsonb".into(),
                ColumnType::Uuid => "uuid".into(),
                ColumnType::Array(elem_type) => {
                    let mut sql = String::new();
                    self.prepare_column_type(elem_type, &mut sql);
                    format!("{sql}[]")
                }
                ColumnType::Vector(size) => match size {
                    Some(size) => format!("vector({size})"),
                    None => "vector".into(),
                },
                ColumnType::Custom(iden) => iden.to_string(),
                ColumnType::Enum { name, .. } => name.to_string(),
                ColumnType::Cidr => "cidr".into(),
                ColumnType::Inet => "inet".into(),
                ColumnType::MacAddr => "macaddr".into(),
                ColumnType::LTree => "ltree".into(),
            }
        )
        .unwrap()
    }

    fn column_spec_auto_increment_keyword(&self) -> &str {
        ""
    }

    // [spec:pgorm:req:sql.ddl.alter-table+3]
    pub(crate) fn prepare_table_alter_statement(
        &self,
        alter: &TableAlterStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER TABLE ").unwrap();
        self.prepare_table_name(&alter.table, sql);
        write!(sql, " ").unwrap();

        alter.options.iter().fold(true, |first, option| {
            if !first {
                write!(sql, ", ").unwrap();
            };
            match option {
                TableAlterOption::AddColumn(AddColumnOption {
                    column,
                    if_not_exists,
                }) => {
                    write!(sql, "ADD COLUMN ").unwrap();
                    if *if_not_exists {
                        write!(sql, "IF NOT EXISTS ").unwrap();
                    }
                    let f = |column_def: &ColumnDef, sql: &mut dyn SqlWriter| {
                        if let Some(column_type) = &column_def.types {
                            write!(sql, " ").unwrap();
                            if column_def
                                .spec
                                .iter()
                                .any(|v| matches!(v, ColumnSpec::AutoIncrement))
                            {
                                self.prepare_column_auto_increment(column_type, sql);
                            } else {
                                self.prepare_column_type(column_type, sql);
                            }
                        }
                    };
                    self.prepare_column_def_common(column, sql, f);
                }
                TableAlterOption::ModifyColumn(column_def) => {
                    if let Some(column_type) = &column_def.types {
                        write!(sql, "ALTER COLUMN ").unwrap();
                        column_def.name.prepare(sql.as_writer(), self.quote());
                        write!(sql, " TYPE ").unwrap();
                        self.prepare_column_type(column_type, sql);
                    }
                    let first = column_def.types.is_none();

                    column_def.spec.iter().fold(first, |first, column_spec| {
                        if !first
                            && !matches!(
                                column_spec,
                                ColumnSpec::AutoIncrement | ColumnSpec::Generated { .. }
                            )
                        {
                            write!(sql, ", ").unwrap();
                        }
                        match column_spec {
                            ColumnSpec::AutoIncrement => {}
                            ColumnSpec::Null => {
                                write!(sql, "ALTER COLUMN ").unwrap();
                                column_def.name.prepare(sql.as_writer(), self.quote());
                                write!(sql, " DROP NOT NULL").unwrap();
                            }
                            ColumnSpec::NotNull => {
                                write!(sql, "ALTER COLUMN ").unwrap();
                                column_def.name.prepare(sql.as_writer(), self.quote());
                                write!(sql, " SET NOT NULL").unwrap()
                            }
                            ColumnSpec::Default(v) => {
                                write!(sql, "ALTER COLUMN ").unwrap();
                                column_def.name.prepare(sql.as_writer(), self.quote());
                                write!(sql, " SET DEFAULT ").unwrap();
                                QueryBuilder::prepare_simple_expr(self, v, sql);
                            }
                            ColumnSpec::UniqueKey => {
                                write!(sql, "ADD UNIQUE (").unwrap();
                                column_def.name.prepare(sql.as_writer(), self.quote());
                                write!(sql, ")").unwrap();
                            }
                            ColumnSpec::PrimaryKey => {
                                write!(sql, "ADD PRIMARY KEY (").unwrap();
                                column_def.name.prepare(sql.as_writer(), self.quote());
                                write!(sql, ")").unwrap();
                            }
                            ColumnSpec::Check(check) => self.prepare_check_constraint(check, sql),
                            ColumnSpec::Generated { .. } => {}
                            ColumnSpec::Extra(string) => write!(sql, "{string}").unwrap(),
                            ColumnSpec::Comment(_) => {}
                        }
                        false
                    });
                }
                TableAlterOption::DropColumn(column_name) => {
                    write!(sql, "DROP COLUMN ").unwrap();
                    column_name.prepare(sql.as_writer(), self.quote());
                }
                TableAlterOption::DropForeignKey(name) => {
                    write!(sql, "DROP CONSTRAINT ").unwrap();
                    name.prepare(sql.as_writer(), self.quote());
                }
                TableAlterOption::AddForeignKey(foreign_key) => {
                    let create = ForeignKeyCreateStatement {
                        foreign_key: foreign_key.to_owned(),
                    };
                    self.prepare_foreign_key_create_statement_internal(
                        &create,
                        sql,
                        Mode::TableAlter,
                    );
                }
            }
            false
        });
    }

    // [spec:pgorm:req:sql.ddl.drop-rename-truncate+3]
    pub(crate) fn prepare_table_rename_statement(
        &self,
        rename: &TableRenameStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER TABLE ").unwrap();
        self.prepare_table_name(&rename.from_name, sql);
        write!(sql, " RENAME TO ").unwrap();
        rename.to_name.prepare(sql.as_writer(), self.quote());
    }

    /// Translate [`ColumnRenameStatement`] into SQL statement.
    // [spec:pgorm:req:sql.ddl.alter-table+3]
    pub(crate) fn prepare_column_rename_statement(
        &self,
        rename: &ColumnRenameStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER TABLE ").unwrap();
        self.prepare_table_name(&rename.table, sql);
        write!(sql, " RENAME COLUMN ").unwrap();
        rename.from_name.prepare(sql.as_writer(), self.quote());
        write!(sql, " TO ").unwrap();
        rename.to_name.prepare(sql.as_writer(), self.quote());
    }

    /// Translate [`TableCreateStatement`] into SQL statement.
    // [spec:pgorm:req:sql.ddl.create-table+6]
    pub(crate) fn prepare_table_create_statement(
        &self,
        create: &TableCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE TABLE ").unwrap();

        self.prepare_create_table_if_not_exists(create, sql);

        self.prepare_table_name(&create.table, sql);

        write!(sql, " ( ").unwrap();
        let mut first = true;

        create.columns.iter().for_each(|column_def| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            self.prepare_column_def(column_def, sql);
            first = false;
        });

        create.indexes.iter().for_each(|index| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            self.prepare_table_index_expression(index, sql);
            first = false;
        });

        create.foreign_keys.iter().for_each(|foreign_key| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            self.prepare_foreign_key_create_statement_internal(foreign_key, sql, Mode::Creation);
            first = false;
        });

        create.check.iter().for_each(|check| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            self.prepare_check_constraint(check, sql);
            first = false;
        });

        write!(sql, " )").unwrap();

        if let Some(extra) = &create.extra {
            write!(sql, " {extra}").unwrap();
        }
    }

    /// Translate [`ColumnSpec`] into SQL statement.
    fn prepare_column_spec(&self, column_spec: &ColumnSpec, sql: &mut dyn SqlWriter) {
        match column_spec {
            ColumnSpec::Null => write!(sql, "NULL").unwrap(),
            ColumnSpec::NotNull => write!(sql, "NOT NULL").unwrap(),
            ColumnSpec::Default(value) => {
                write!(sql, "DEFAULT ").unwrap();
                QueryBuilder::prepare_simple_expr(self, value, sql);
            }
            ColumnSpec::AutoIncrement => {
                write!(sql, "{}", self.column_spec_auto_increment_keyword()).unwrap()
            }
            ColumnSpec::UniqueKey => write!(sql, "UNIQUE").unwrap(),
            ColumnSpec::PrimaryKey => write!(sql, "PRIMARY KEY").unwrap(),
            ColumnSpec::Check(check) => self.prepare_check_constraint(check, sql),
            ColumnSpec::Generated { expr, stored } => {
                self.prepare_generated_column(expr, *stored, sql)
            }
            ColumnSpec::Extra(string) => write!(sql, "{string}").unwrap(),
            ColumnSpec::Comment(_) => {}
        }
    }

    /// Translate [`CommentStatement`] into SQL statement.
    // [spec:pgorm:req:sql.ddl.comment+2]
    pub(crate) fn prepare_comment_statement(
        &self,
        statement: &CommentStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "COMMENT ON ").unwrap();
        match &statement.target {
            CommentTarget::Table(table) => {
                write!(sql, "TABLE ").unwrap();
                self.prepare_table_name(table, sql);
            }
            CommentTarget::Column(table, column) => {
                write!(sql, "COLUMN ").unwrap();
                self.prepare_table_name(table, sql);
                write!(sql, ".").unwrap();
                column.prepare(sql.as_writer(), self.quote());
            }
        }
        write!(sql, " IS ").unwrap();
        self.prepare_comment_text(&statement.comment, sql);
    }

    /// Write comment text as a standard-conforming string literal.
    // [spec:pgorm:req:sql.ddl.comment+2]
    fn prepare_comment_text(&self, comment: &str, sql: &mut dyn SqlWriter) {
        write!(sql, "'{}'", comment.replace('\'', "''")).unwrap();
    }

    /// Translate [`TableDropStatement`] into SQL statement.
    // [spec:pgorm:req:sql.ddl.drop-rename-truncate+3]
    pub(crate) fn prepare_table_drop_statement(
        &self,
        drop: &TableDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP TABLE ").unwrap();

        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }

        drop.tables.iter().fold(true, |first, table| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            self.prepare_table_name(table, sql);
            false
        });

        for drop_opt in drop.options.iter() {
            self.prepare_table_drop_opt(drop_opt, sql);
        }
    }

    /// Translate [`TableDropOpt`] into SQL statement.
    fn prepare_table_drop_opt(&self, drop_opt: &TableDropOpt, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            " {}",
            match drop_opt {
                TableDropOpt::Restrict => "RESTRICT",
                TableDropOpt::Cascade => "CASCADE",
            }
        )
        .unwrap();
    }

    /// Translate [`TableTruncateStatement`] into SQL statement.
    pub(crate) fn prepare_table_truncate_statement(
        &self,
        truncate: &TableTruncateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "TRUNCATE TABLE ").unwrap();
        self.prepare_table_name(&truncate.table, sql);
    }

    /// Translate the check constraint into SQL statement
    pub(crate) fn prepare_check_constraint(&self, check: &SimpleExpr, sql: &mut dyn SqlWriter) {
        write!(sql, "CHECK (").unwrap();
        QueryBuilder::prepare_simple_expr(self, check, sql);
        write!(sql, ")").unwrap();
    }

    /// Translate the generated column into SQL statement
    pub(crate) fn prepare_generated_column(
        &self,
        gen_: &SimpleExpr,
        stored: bool,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "GENERATED ALWAYS AS (").unwrap();
        QueryBuilder::prepare_simple_expr(self, gen_, sql);
        write!(sql, ")").unwrap();
        if stored {
            write!(sql, " STORED").unwrap();
        } else {
            write!(sql, " VIRTUAL").unwrap();
        }
    }

    /// Translate IF NOT EXISTS expression in [`TableCreateStatement`].
    fn prepare_create_table_if_not_exists(
        &self,
        create: &TableCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        if create.if_not_exists {
            write!(sql, "IF NOT EXISTS ").unwrap();
        }
    }

    // INDEX
    // Overriden due to different "NULLS NOT UNIQUE" position in table index expression
    // (as opposed to the regular index expression)
    fn prepare_table_index_expression(
        &self,
        create: &IndexCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(name) = &create.index.name {
            write!(sql, "CONSTRAINT ").unwrap();
            name.prepare(sql.as_writer(), self.quote());
            write!(sql, " ").unwrap();
        }

        match create.kind {
            IndexKind::Plain => {}
            IndexKind::Unique => write!(sql, "UNIQUE ").unwrap(),
            IndexKind::PrimaryKey => write!(sql, "PRIMARY KEY ").unwrap(),
        }

        if create.nulls_not_distinct && create.kind == IndexKind::Unique {
            write!(sql, "NULLS NOT DISTINCT ").unwrap();
        }

        self.prepare_index_columns(&create.index.columns, sql);
    }

    // [spec:pgorm:req:sql.ddl.index-create+4]
    pub(crate) fn prepare_index_create_statement(
        &self,
        create: &IndexCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        let kind = create.kind.standalone();

        write!(sql, "CREATE ").unwrap();
        match kind {
            Some(StandaloneIndexKind::Unique) => write!(sql, "UNIQUE ").unwrap(),
            Some(StandaloneIndexKind::Plain) | None => {}
        }
        write!(sql, "INDEX ").unwrap();

        if create.if_not_exists {
            write!(sql, "IF NOT EXISTS ").unwrap();
        }

        if let Some(name) = &create.index.name {
            name.prepare(sql.as_writer(), self.quote());
        }

        write!(sql, " ON ").unwrap();
        self.prepare_table_name(&create.table, sql);

        self.prepare_index_type(&create.index_type, sql);
        write!(sql, " ").unwrap();
        self.prepare_index_columns(&create.index.columns, sql);

        if create.nulls_not_distinct && kind == Some(StandaloneIndexKind::Unique) {
            write!(sql, " NULLS NOT DISTINCT").unwrap();
        }
    }

    // [spec:pgorm:req:sql.ddl.index-drop+2]
    pub(crate) fn prepare_index_drop_statement(
        &self,
        drop: &IndexDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP INDEX ").unwrap();

        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }

        if let Some(schema) = drop.table.as_ref().and_then(TableName::schema) {
            schema.prepare(sql.as_writer(), self.quote());
            write!(sql, ".").unwrap();
        }
        drop.name.prepare(sql.as_writer(), self.quote());
    }

    fn prepare_index_type(&self, col_index_type: &Option<IndexType>, sql: &mut dyn SqlWriter) {
        if let Some(index_type) = col_index_type {
            write!(
                sql,
                " USING {}",
                match index_type {
                    IndexType::BTree => "BTREE".to_owned(),
                    IndexType::FullText => "GIN".to_owned(),
                    IndexType::Hash => "HASH".to_owned(),
                    IndexType::Custom(custom) => custom.to_string(),
                }
            )
            .unwrap();
        }
    }

    #[doc(hidden)]
    /// Write the column index prefix.
    fn write_column_index_prefix(&self, col_prefix: &Option<u32>, sql: &mut dyn SqlWriter) {
        if let Some(prefix) = col_prefix {
            write!(sql, " ({prefix})").unwrap();
        }
    }

    #[doc(hidden)]
    /// Write the column index prefix.
    fn prepare_index_columns(&self, columns: &[IndexColumn], sql: &mut dyn SqlWriter) {
        write!(sql, "(").unwrap();
        columns.iter().fold(true, |first, col| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            col.name.prepare(sql.as_writer(), self.quote());
            self.write_column_index_prefix(&col.prefix, sql);
            if let Some(order) = &col.order {
                match order {
                    IndexOrder::Asc => write!(sql, " ASC").unwrap(),
                    IndexOrder::Desc => write!(sql, " DESC").unwrap(),
                }
            }
            false
        });
        write!(sql, ")").unwrap();
    }

    // FOREIGN KEY

    /// Translate [`ForeignKeyDropStatement`] into SQL statement.
    // [spec:pgorm:req:sql.ddl.foreign-key+3]
    pub(crate) fn prepare_foreign_key_drop_statement(
        &self,
        drop: &ForeignKeyDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER TABLE ").unwrap();
        self.prepare_table_name(&drop.table, sql);
        write!(sql, " DROP CONSTRAINT ").unwrap();
        drop.name.prepare(sql.as_writer(), self.quote());
    }

    // [spec:pgorm:req:sql.ddl.foreign-key+3]
    fn prepare_foreign_key_create_statement_internal(
        &self,
        create: &ForeignKeyCreateStatement,
        sql: &mut dyn SqlWriter,
        mode: Mode,
    ) {
        if mode == Mode::Alter {
            write!(sql, "ALTER TABLE ").unwrap();
            self.prepare_table_name(&create.foreign_key.table, sql);
            write!(sql, " ").unwrap();
        }

        if mode != Mode::Creation {
            write!(sql, "ADD ").unwrap();
        }

        if let Some(name) = &create.foreign_key.name {
            write!(sql, "CONSTRAINT ").unwrap();
            name.prepare(sql.as_writer(), self.quote());
            write!(sql, " ").unwrap();
        }

        write!(sql, "FOREIGN KEY (").unwrap();
        create.foreign_key.columns().fold(true, |first, (col, _)| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            col.prepare(sql.as_writer(), self.quote());
            false
        });
        write!(sql, ")").unwrap();

        write!(sql, " REFERENCES ").unwrap();
        self.prepare_table_name(&create.foreign_key.ref_table, sql);
        write!(sql, " ").unwrap();

        write!(sql, "(").unwrap();
        create.foreign_key.columns().fold(true, |first, (_, col)| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            col.prepare(sql.as_writer(), self.quote());
            false
        });
        write!(sql, ")").unwrap();

        if let Some(foreign_key_action) = &create.foreign_key.on_delete {
            write!(sql, " ON DELETE ").unwrap();
            self.prepare_foreign_key_action(foreign_key_action, sql);
        }

        if let Some(foreign_key_action) = &create.foreign_key.on_update {
            write!(sql, " ON UPDATE ").unwrap();
            self.prepare_foreign_key_action(foreign_key_action, sql);
        }
    }

    /// Translate [`ForeignKeyCreateStatement`] into SQL statement.
    pub(crate) fn prepare_foreign_key_create_statement(
        &self,
        create: &ForeignKeyCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        self.prepare_foreign_key_create_statement_internal(create, sql, Mode::Alter)
    }

    /// Translate [`ForeignKeyAction`] into SQL statement.
    pub(crate) fn prepare_foreign_key_action(
        &self,
        foreign_key_action: &ForeignKeyAction,
        sql: &mut dyn SqlWriter,
    ) {
        write!(
            sql,
            "{}",
            match foreign_key_action {
                ForeignKeyAction::Restrict => "RESTRICT",
                ForeignKeyAction::Cascade => "CASCADE",
                ForeignKeyAction::SetNull => "SET NULL",
                ForeignKeyAction::NoAction => "NO ACTION",
                ForeignKeyAction::SetDefault => "SET DEFAULT",
            }
        )
        .unwrap()
    }

    // ESCAPE

    /// Escape a SQL string literal
    // [spec:pgorm:req:sql.render.string-escape]
    pub fn escape_string(&self, string: &str) -> String {
        string
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\'', "\\'")
            .replace('\0', "\\0")
            .replace('\x08', "\\b")
            .replace('\x09', "\\t")
            .replace('\x1a', "\\z")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    }

    /// Unescape a SQL string literal
    pub fn unescape_string(&self, string: &str) -> String {
        let mut escape = false;
        let mut output = String::new();
        for c in string.chars() {
            if !escape && c == '\\' {
                escape = true;
            } else if escape {
                write!(
                    output,
                    "{}",
                    match c {
                        '0' => '\0',
                        'b' => '\x08',
                        't' => '\x09',
                        'z' => '\x1a',
                        'n' => '\n',
                        'r' => '\r',
                        c => c,
                    }
                )
                .unwrap();
                escape = false;
            } else {
                write!(output, "{c}").unwrap();
            }
        }
        output
    }

    // TABLE NAME
    /// Translate [`TableName`] into SQL statement.
    fn prepare_table_name(&self, name: &TableName, sql: &mut dyn SqlWriter) {
        match name {
            TableName::Table(table) => table.prepare(sql.as_writer(), self.quote()),
            TableName::SchemaTable(schema, table) => {
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                table.prepare(sql.as_writer(), self.quote());
            }
        }
    }

    /// Translate [`NamedTable`] into SQL statement.
    // [spec:pgorm:def:sql.types.table-ref+2]
    fn prepare_named_table(&self, table: &NamedTable, sql: &mut dyn SqlWriter) {
        self.prepare_table_name(&table.name, sql);
        if let Some(alias) = &table.alias {
            write!(sql, " AS ").unwrap();
            alias.prepare(sql.as_writer(), self.quote());
        }
    }

    // TYPE BUILDER
    // [spec:pgorm:req:sql.ddl.type-enum+2]
    fn prepare_create_as_type(&self, as_type: &TypeAs, sql: &mut dyn SqlWriter) {
        match as_type {
            TypeAs::Enum(values) => {
                write!(sql, "ENUM (").unwrap();
                for (count, val) in values.iter().enumerate() {
                    if count > 0 {
                        write!(sql, ", ").unwrap();
                    }
                    sql.push_param(val.to_string().into());
                }
                write!(sql, ")").unwrap();
            }
        }
    }

    fn prepare_drop_type_opt(&self, opt: &TypeDropOpt, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match opt {
                TypeDropOpt::Cascade => "CASCADE",
                TypeDropOpt::Restrict => "RESTRICT",
            }
        )
        .unwrap()
    }

    // [spec:pgorm:req:sql.render.ddl.enum-type+1] (ALTER TYPE label operands parameterized)
    fn prepare_alter_type_opt(&self, opt: &TypeAlterOpt, sql: &mut dyn SqlWriter) {
        match opt {
            TypeAlterOpt::Add(value, placement) => {
                write!(sql, " ADD VALUE ").unwrap();
                match placement {
                    Some(add_option) => match add_option {
                        TypeAlterAddOpt::Before(before_value) => {
                            sql.push_param(value.to_string().into());
                            write!(sql, " BEFORE ").unwrap();
                            sql.push_param(before_value.to_string().into());
                        }
                        TypeAlterAddOpt::After(after_value) => {
                            sql.push_param(value.to_string().into());
                            write!(sql, " AFTER ").unwrap();
                            sql.push_param(after_value.to_string().into());
                        }
                    },
                    None => sql.push_param(value.to_string().into()),
                }
            }
            TypeAlterOpt::Rename(new_name) => {
                write!(sql, " RENAME TO ").unwrap();
                new_name.prepare(sql.as_writer(), self.quote());
            }
            TypeAlterOpt::RenameValue(existing, new_name) => {
                write!(sql, " RENAME VALUE ").unwrap();
                sql.push_param(existing.to_string().into());
                write!(sql, " TO ").unwrap();
                sql.push_param(new_name.to_string().into());
            }
        }
    }

    // [spec:pgorm:req:sql.ddl.type-enum+2]
    // [spec:pgorm:req:sql.render.ddl.enum-type+1]
    pub(crate) fn prepare_type_create_statement(
        &self,
        create: &TypeCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE TYPE ").unwrap();

        self.prepare_type_ref(&create.name, sql);

        if let Some(as_type) = &create.as_type {
            write!(sql, " AS ").unwrap();
            self.prepare_create_as_type(as_type, sql);
        }
    }

    // [spec:pgorm:req:sql.ddl.type-alter-drop+3]
    pub(crate) fn prepare_type_drop_statement(
        &self,
        drop: &TypeDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP TYPE ").unwrap();

        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }

        drop.names_iter().fold(true, |first, name| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            self.prepare_type_ref(name, sql);
            false
        });

        if let Some(option) = &drop.option {
            write!(sql, " ").unwrap();
            self.prepare_drop_type_opt(option, sql);
        }
    }

    // [spec:pgorm:req:sql.ddl.type-alter-drop+3]
    pub(crate) fn prepare_type_alter_statement(
        &self,
        alter: &TypeAlterStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER TYPE ").unwrap();
        self.prepare_type_ref(&alter.name, sql);
        self.prepare_alter_type_opt(&alter.option, sql);
    }

    /// Translate [`TypeRef`] into SQL statement.
    fn prepare_type_ref(&self, type_ref: &TypeRef, sql: &mut dyn SqlWriter) {
        match type_ref {
            TypeRef::Type(name) => {
                name.prepare(sql.as_writer(), self.quote());
            }
            TypeRef::SchemaType(schema, name) => {
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                name.prepare(sql.as_writer(), self.quote());
            }
            TypeRef::DatabaseSchemaType(database, schema, name) => {
                database.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                name.prepare(sql.as_writer(), self.quote());
            }
        }
    }

    // EXTENSION
    // [spec:pgorm:req:sql.ddl.extension+3]
    // [spec:pgorm:sem:sql.render.ddl.extension+1] (CREATE EXTENSION)
    pub(crate) fn prepare_extension_create_statement(
        &self,
        create: &ExtensionCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE EXTENSION ").unwrap();

        if create.if_not_exists {
            write!(sql, "IF NOT EXISTS ").unwrap()
        }

        create.name.prepare(sql.as_writer(), self.quote());

        if let Some(schema) = create.schema.as_ref() {
            write!(sql, " WITH SCHEMA ").unwrap();
            self.prepare_extension_ident(schema, sql);
        }

        if let Some(version) = create.version.as_ref() {
            write!(sql, " VERSION ").unwrap();
            let mut literal = String::new();
            self.write_string_quoted(version, &mut literal);
            write!(sql, "{literal}").unwrap();
        }

        if create.cascade {
            write!(sql, " CASCADE").unwrap();
        }
    }

    // [spec:pgorm:req:sql.ddl.extension+3]
    // [spec:pgorm:sem:sql.render.ddl.extension+1] (DROP EXTENSION)
    pub(crate) fn prepare_extension_drop_statement(
        &self,
        drop: &ExtensionDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP EXTENSION ").unwrap();

        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }

        drop.name.prepare(sql.as_writer(), self.quote());

        match drop.option {
            Some(ExtensionDropOpt::Cascade) => write!(sql, " CASCADE").unwrap(),
            Some(ExtensionDropOpt::Restrict) => write!(sql, " RESTRICT").unwrap(),
            None => {}
        }
    }

    /// Write an extension name or schema as a quoted identifier.
    // [spec:pgorm:sem:sql.render.ddl.extension+1]
    fn prepare_extension_ident(&self, ident: &str, sql: &mut dyn SqlWriter) {
        Alias::new(ident).prepare(sql.as_writer(), self.quote());
    }

    // [spec:pgorm:def:sql.render.precedence+1]
    fn inner_expr_well_known_greater_precedence(
        &self,
        inner: &SimpleExpr,
        outer_oper: &Oper,
    ) -> bool {
        let common_answer = common_inner_expr_well_known_greater_precedence(inner, outer_oper);
        let pg_specific_answer = match inner {
            SimpleExpr::Binary(_, inner_bin_oper, _) => {
                let inner_oper: Oper = (*inner_bin_oper).into();
                if inner_oper.is_arithmetic() || inner_oper.is_shift() {
                    is_ilike(inner_bin_oper)
                } else if is_pg_comparison(inner_bin_oper) {
                    outer_oper.is_logical()
                } else {
                    false
                }
            }
            _ => false,
        };
        common_answer || pg_specific_answer
    }

    // [spec:pgorm:req:sql.render.parens] (left-associative flattening incl. || for Postgres)
    fn well_known_left_associative(&self, op: &BinOper) -> bool {
        let common_answer = common_well_known_left_associative(op);
        let pg_specific_answer = matches!(op, BinOper::Concatenate);
        common_answer || pg_specific_answer
    }
}

fn is_pg_comparison(b: &BinOper) -> bool {
    matches!(
        b,
        BinOper::Contained
            | BinOper::Contains
            | BinOper::Similarity
            | BinOper::WordSimilarity
            | BinOper::StrictWordSimilarity
            | BinOper::Matches
            | BinOper::HasJsonKey
            | BinOper::HasAnyJsonKeys
            | BinOper::HasAllJsonKeys
    )
}

fn is_ilike(b: &BinOper) -> bool {
    matches!(b, BinOper::ILike | BinOper::NotILike)
}

impl SubQueryStatement {
    pub(crate) fn prepare_statement(&self, sql: &mut dyn SqlWriter) {
        use SubQueryStatement::*;
        match self {
            SelectStatement(stmt) => QueryBuilder.prepare_select_statement(stmt, sql),
            InsertStatement(stmt) => QueryBuilder.prepare_insert_statement(stmt, sql),
            UpdateStatement(stmt) => QueryBuilder.prepare_update_statement(stmt, sql),
            DeleteStatement(stmt) => QueryBuilder.prepare_delete_statement(stmt, sql),
            WithStatement(stmt) => QueryBuilder.prepare_with_query(stmt, sql),
        }
    }
}

// [spec:pgorm:def:sql.render.precedence+1] (backend-independent portion of the elision table)
pub(crate) fn common_inner_expr_well_known_greater_precedence(
    inner: &SimpleExpr,
    outer_oper: &Oper,
) -> bool {
    match inner {
        // We only consider the case where an inner expression is contained in either a
        // unary or binary expression (with an outer_oper).
        // We do not need to wrap with parentheses:
        // Columns, tuples (already wrapped), constants, function calls, values,
        // keywords, subqueries (already wrapped), case (already wrapped)
        SimpleExpr::Column(_)
        | SimpleExpr::Tuple(_)
        | SimpleExpr::Constant(_)
        | SimpleExpr::FunctionCall(_)
        | SimpleExpr::Value(_)
        | SimpleExpr::Keyword(_)
        | SimpleExpr::Case(_)
        | SimpleExpr::LikePattern(_)
        | SimpleExpr::SubQuery(_, _) => true,
        SimpleExpr::Binary(_, inner_oper, _) => {
            let inner_oper: Oper = (*inner_oper).into();
            if inner_oper.is_arithmetic() || inner_oper.is_shift() {
                outer_oper.is_comparison()
                    || outer_oper.is_between()
                    || outer_oper.is_in()
                    || outer_oper.is_like()
                    || outer_oper.is_logical()
            } else if inner_oper.is_comparison()
                || inner_oper.is_in()
                || inner_oper.is_like()
                || inner_oper.is_is()
            {
                outer_oper.is_logical()
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(crate) fn common_well_known_left_associative(op: &BinOper) -> bool {
    matches!(
        op,
        BinOper::And | BinOper::Or | BinOper::Add | BinOper::Sub | BinOper::Mul | BinOper::Mod
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Creation,
    Alter,
    TableAlter,
}
