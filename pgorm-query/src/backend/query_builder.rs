use crate::{
    extension::{
        ExtensionCreateStatement, ExtensionDropStatement, TypeAlterAddOpt, TypeAlterOpt,
        TypeAlterStatement, TypeAs, TypeCreateStatement, TypeDropOpt, TypeDropStatement, TypeRef,
    },
    *,
};
use std::ops::Deref;

const QUOTE: Quote = Quote(b'"', b'"');

#[derive(Debug, Clone, Copy)]
pub struct QueryBuilder;

impl QueryBuilder {
    const fn quote(&self) -> Quote {
        QUOTE
    }

    pub const fn placeholder(&self) -> (&str, bool) {
        ("$", true)
    }

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
            _ => {}
        };
    }

    fn prepare_bin_oper(&self, bin_oper: &BinOper, sql: &mut dyn SqlWriter) {
        match bin_oper {
            _ => self.prepare_bin_oper_common(bin_oper, sql),
        }
    }

    fn prepare_query_statement(&self, query: &SubQueryStatement, sql: &mut dyn SqlWriter) {
        query.prepare_statement(self, sql);
    }

    fn prepare_function_name(&self, function: &Function, sql: &mut dyn SqlWriter) {
        match function {
            // TODO: remove this
            _ => self.prepare_function_name_common(function, sql),
        }
    }

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

    fn prepare_value(&self, value: &Value, sql: &mut dyn SqlWriter) {
        sql.push_param(value.clone(), self as _);
    }

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
    pub(crate) fn prepare_insert_statement(
        &self,
        insert: &InsertStatement,
        sql: &mut dyn SqlWriter,
    ) {
        self.prepare_insert(insert.replace, sql);

        if let Some(table) = &insert.table {
            write!(sql, " INTO ").unwrap();
            self.prepare_table_ref(table, sql);
        }

        if insert.default_values.is_some() && insert.columns.is_empty() && insert.source.is_none() {
            self.prepare_output(&insert.returning, sql);
            write!(sql, " ").unwrap();
            let num_rows = insert.default_values.unwrap();
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
    pub(crate) fn prepare_select_statement(
        &self,
        select: &SelectStatement,
        sql: &mut dyn SqlWriter,
    ) {
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
            select.from.iter().fold(true, |first, table_ref| {
                if !first {
                    write!(sql, ", ").unwrap()
                }
                self.prepare_table_ref(table_ref, sql);
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

        if let Some((name, query)) = &select.window {
            write!(sql, " WINDOW ").unwrap();
            name.prepare(sql.as_writer(), self.quote());
            write!(sql, " AS ").unwrap();
            self.prepare_window_statement(query, sql);
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
            self.prepare_value(limit, sql);
        }

        if let Some(offset) = &select.offset {
            write!(sql, " OFFSET ").unwrap();
            self.prepare_value(offset, sql);
        }
    }

    /// Translate [`UpdateStatement`] into SQL statement.
    pub(crate) fn prepare_update_statement(
        &self,
        update: &UpdateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "UPDATE ").unwrap();

        if let Some(table) = &update.table {
            self.prepare_table_ref(table, sql);
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

        self.prepare_update_order_by(update, sql);

        self.prepare_update_limit(update, sql);

        self.prepare_returning(&update.returning, sql);
    }

    /// Translate ORDER BY expression in [`UpdateStatement`].
    fn prepare_update_order_by(&self, update: &UpdateStatement, sql: &mut dyn SqlWriter) {
        if !update.orders.is_empty() {
            write!(sql, " ORDER BY ").unwrap();
            update.orders.iter().fold(true, |first, expr| {
                if !first {
                    write!(sql, ", ").unwrap();
                }
                self.prepare_order_expr(expr, sql);
                false
            });
        }
    }

    /// Translate LIMIT expression in [`UpdateStatement`].
    fn prepare_update_limit(&self, update: &UpdateStatement, sql: &mut dyn SqlWriter) {
        if let Some(limit) = &update.limit {
            write!(sql, " LIMIT ").unwrap();
            self.prepare_value(limit, sql);
        }
    }

    /// Translate [`DeleteStatement`] into SQL statement.
    pub(crate) fn prepare_delete_statement(
        &self,
        delete: &DeleteStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DELETE ").unwrap();

        if let Some(table) = &delete.table {
            write!(sql, "FROM ").unwrap();
            self.prepare_table_ref(table, sql);
        }

        self.prepare_output(&delete.returning, sql);

        self.prepare_condition(&delete.r#where, "WHERE", sql);

        self.prepare_delete_order_by(delete, sql);

        self.prepare_delete_limit(delete, sql);

        self.prepare_returning(&delete.returning, sql);
    }

    /// Translate ORDER BY expression in [`DeleteStatement`].
    fn prepare_delete_order_by(&self, delete: &DeleteStatement, sql: &mut dyn SqlWriter) {
        if !delete.orders.is_empty() {
            write!(sql, " ORDER BY ").unwrap();
            delete.orders.iter().fold(true, |first, expr| {
                if !first {
                    write!(sql, ", ").unwrap();
                }
                self.prepare_order_expr(expr, sql);
                false
            });
        }
    }

    /// Translate LIMIT expression in [`DeleteStatement`].
    fn prepare_delete_limit(&self, delete: &DeleteStatement, sql: &mut dyn SqlWriter) {
        if let Some(limit) = &delete.limit {
            write!(sql, " LIMIT ").unwrap();
            self.prepare_value(limit, sql);
        }
    }

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
                    &SimpleExpr::Value("b".into()),
                    sql,
                ),
                _ => self.binary_expr(left, op, right, sql),
            },
            SimpleExpr::SubQuery(oper, sel) => {
                if let Some(oper) = oper {
                    self.prepare_sub_query_oper(oper, sql);
                }
                write!(sql, "(").unwrap();
                self.prepare_query_statement(sel.deref(), sql);
                write!(sql, ")").unwrap();
            }
            SimpleExpr::Value(val) => {
                self.prepare_value(val, sql);
            }
            SimpleExpr::Values(list) => {
                write!(sql, "(").unwrap();
                list.iter().fold(true, |first, val| {
                    if !first {
                        write!(sql, ", ").unwrap();
                    }
                    self.prepare_value(val, sql);
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
            lock.tables.iter().fold(true, |first, table_ref| {
                if !first {
                    write!(sql, ", ").unwrap();
                }
                self.prepare_table_ref(table_ref, sql);
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
    fn prepare_select_expr(&self, select_expr: &SelectExpr, sql: &mut dyn SqlWriter) {
        self.prepare_simple_expr(&select_expr.expr, sql);
        match &select_expr.window {
            Some(WindowSelectType::Name(name)) => {
                write!(sql, " OVER ").unwrap();
                name.prepare(sql.as_writer(), self.quote())
            }
            Some(WindowSelectType::Query(window)) => {
                write!(sql, " OVER ").unwrap();
                write!(sql, "( ").unwrap();
                self.prepare_window_statement(window, sql);
                write!(sql, " )").unwrap();
            }
            None => {}
        };

        match &select_expr.alias {
            Some(alias) => {
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            None => {}
        };
    }

    /// Translate [`JoinExpr`] into SQL statement.
    fn prepare_join_expr(&self, join_expr: &JoinExpr, sql: &mut dyn SqlWriter) {
        self.prepare_join_type(&join_expr.join, sql);
        write!(sql, " ").unwrap();
        self.prepare_join_table_ref(join_expr, sql);
        if let Some(on) = &join_expr.on {
            self.prepare_join_on(on, sql);
        }
    }

    fn prepare_join_table_ref(&self, join_expr: &JoinExpr, sql: &mut dyn SqlWriter) {
        if join_expr.lateral {
            write!(sql, "LATERAL ").unwrap();
        }
        self.prepare_table_ref(&join_expr.table, sql);
    }

    /// Translate [`TableRef`] into SQL statement.
    fn prepare_table_ref(&self, table_ref: &TableRef, sql: &mut dyn SqlWriter) {
        match table_ref {
            TableRef::SubQuery(query, alias) => {
                write!(sql, "(").unwrap();
                self.prepare_select_statement(query, sql);
                write!(sql, ")").unwrap();
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            TableRef::ValuesList(values, alias) => {
                write!(sql, "(").unwrap();
                self.prepare_values_list(values, sql);
                write!(sql, ")").unwrap();
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            TableRef::FunctionCall(func, alias) => {
                self.prepare_function_name(&func.func, sql);
                self.prepare_function_arguments(func, sql);
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            _ => self.prepare_table_ref_iden(table_ref, sql),
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

    fn prepare_bin_oper_common(&self, bin_oper: &BinOper, sql: &mut dyn SqlWriter) {
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
                BinOper::Escape => "ESCAPE",
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

    /// Translate [`LogicalChainOper`] into SQL statement.
    fn prepare_logical_chain_oper(
        &self,
        log_chain_oper: &LogicalChainOper,
        i: usize,
        length: usize,
        sql: &mut dyn SqlWriter,
    ) {
        let (simple_expr, oper) = match log_chain_oper {
            LogicalChainOper::And(simple_expr) => (simple_expr, "AND"),
            LogicalChainOper::Or(simple_expr) => (simple_expr, "OR"),
        };
        if i > 0 {
            write!(sql, " {oper} ").unwrap();
        }
        let both_binary = match simple_expr {
            SimpleExpr::Binary(_, _, right) => {
                matches!(right.as_ref(), SimpleExpr::Binary(_, _, _))
            }
            _ => false,
        };
        let need_parentheses = length > 1 && both_binary;
        if need_parentheses {
            write!(sql, "(").unwrap();
        }
        self.prepare_simple_expr(simple_expr, sql);
        if need_parentheses {
            write!(sql, ")").unwrap();
        }
    }

    /// Translate [`Function`] into SQL statement.
    fn prepare_function_name_common(&self, function: &Function, sql: &mut dyn SqlWriter) {
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
        self.prepare_query_statement(query.query.as_ref().unwrap().deref(), sql);
    }

    pub(crate) fn prepare_with_clause(&self, with_clause: &WithClause, sql: &mut dyn SqlWriter) {
        self.prepare_with_clause_start(with_clause, sql);
        self.prepare_with_clause_common_tables(with_clause, sql);
        if with_clause.recursive {
            self.prepare_with_clause_recursive_options(with_clause, sql);
        }
    }

    fn prepare_with_clause_recursive_options(
        &self,
        with_clause: &WithClause,
        sql: &mut dyn SqlWriter,
    ) {
        if with_clause.recursive {
            if let Some(search) = &with_clause.search {
                write!(
                    sql,
                    "SEARCH {} FIRST BY ",
                    match &search.order.as_ref().unwrap() {
                        SearchOrder::BREADTH => "BREADTH",
                        SearchOrder::DEPTH => "DEPTH",
                    }
                )
                .unwrap();

                self.prepare_simple_expr(&search.expr.as_ref().unwrap().expr, sql);

                write!(sql, " SET ").unwrap();

                search
                    .expr
                    .as_ref()
                    .unwrap()
                    .alias
                    .as_ref()
                    .unwrap()
                    .prepare(sql.as_writer(), self.quote());
                write!(sql, " ").unwrap();
            }
            if let Some(cycle) = &with_clause.cycle {
                write!(sql, "CYCLE ").unwrap();

                self.prepare_simple_expr(cycle.expr.as_ref().unwrap(), sql);

                write!(sql, " SET ").unwrap();

                cycle
                    .set_as
                    .as_ref()
                    .unwrap()
                    .prepare(sql.as_writer(), self.quote());
                write!(sql, " USING ").unwrap();
                cycle
                    .using
                    .as_ref()
                    .unwrap()
                    .prepare(sql.as_writer(), self.quote());
                write!(sql, " ").unwrap();
            }
        }
    }

    fn prepare_with_clause_common_tables(&self, with_clause: &WithClause, sql: &mut dyn SqlWriter) {
        let mut cte_first = true;
        assert_ne!(
            with_clause.cte_expressions.len(),
            0,
            "Cannot build a with query that has no common table expression!"
        );

        if with_clause.recursive {
            assert_eq!(
                with_clause.cte_expressions.len(),
                1,
                "Cannot build a recursive query with more than one common table! \
                A recursive with query must have a single cte inside it that has a union query of \
                two queries!"
            );
        }
        for cte in &with_clause.cte_expressions {
            if !cte_first {
                write!(sql, ", ").unwrap();
            }
            cte_first = false;

            self.prepare_with_query_clause_common_table(cte, sql);
        }
    }

    fn prepare_with_query_clause_common_table(
        &self,
        cte: &CommonTableExpression,
        sql: &mut dyn SqlWriter,
    ) {
        cte.table_name
            .as_ref()
            .unwrap()
            .prepare(sql.as_writer(), self.quote());

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

        self.prepare_query_statement(cte.query.as_ref().unwrap().deref(), sql);

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

    fn prepare_with_clause_start(&self, with_clause: &WithClause, sql: &mut dyn SqlWriter) {
        write!(sql, "WITH ").unwrap();

        if with_clause.recursive {
            write!(sql, "RECURSIVE ").unwrap();
        }
    }

    fn prepare_insert(&self, replace: bool, sql: &mut dyn SqlWriter) {
        if replace {
            write!(sql, "REPLACE").unwrap();
        } else {
            write!(sql, "INSERT").unwrap();
        }
    }

    /// Translate [`JoinType`] into SQL statement.
    fn prepare_join_type(&self, join_type: &JoinType, sql: &mut dyn SqlWriter) {
        self.prepare_join_type_common(join_type, sql)
    }

    fn prepare_join_type_common(&self, join_type: &JoinType, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match join_type {
                JoinType::Join => "JOIN",
                JoinType::CrossJoin => "CROSS JOIN",
                JoinType::InnerJoin => "INNER JOIN",
                JoinType::LeftJoin => "LEFT JOIN",
                JoinType::RightJoin => "RIGHT JOIN",
                JoinType::FullOuterJoin => "FULL OUTER JOIN",
            }
        )
        .unwrap()
    }

    /// Translate [`JoinOn`] into SQL statement.
    fn prepare_join_on(&self, join_on: &JoinOn, sql: &mut dyn SqlWriter) {
        match join_on {
            JoinOn::Condition(c) => self.prepare_condition(c, "ON", sql),
            JoinOn::Columns(_c) => unimplemented!(),
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
                self.prepare_value(&value, sql);
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
    pub(crate) fn value_to_string(&self, v: &Value) -> String {
        self.value_to_string_common(v)
    }

    fn value_to_string_common(&self, v: &Value) -> String {
        let mut s = String::new();
        match v {
            Value::Bool(None)
            | Value::TinyInt(None)
            | Value::SmallInt(None)
            | Value::Int(None)
            | Value::BigInt(None)
            | Value::TinyUnsigned(None)
            | Value::SmallUnsigned(None)
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
            Value::TinyUnsigned(Some(v)) => write!(s, "{v}").unwrap(),
            Value::SmallUnsigned(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Unsigned(Some(v)) => write!(s, "{v}").unwrap(),
            Value::BigUnsigned(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Float(Some(v)) => write!(s, "{v}").unwrap(),
            Value::Double(Some(v)) => write!(s, "{v}").unwrap(),
            Value::String(Some(v)) => self.write_string_quoted(v, &mut s),
            Value::Char(Some(v)) => {
                self.write_string_quoted(std::str::from_utf8(&[*v as u8]).unwrap(), &mut s)
            }
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
    fn prepare_on_conflict(&self, on_conflict: &Option<OnConflict>, sql: &mut dyn SqlWriter) {
        if let Some(on_conflict) = on_conflict {
            self.prepare_on_conflict_keywords(sql);
            self.prepare_on_conflict_target(&on_conflict.targets, sql);
            self.prepare_on_conflict_condition(&on_conflict.target_where, sql);
            self.prepare_on_conflict_action(&on_conflict.action, sql);
            self.prepare_on_conflict_condition(&on_conflict.action_where, sql);
        }
    }

    #[doc(hidden)]
    /// Write ON CONFLICT target
    fn prepare_on_conflict_target(
        &self,
        on_conflict_targets: &[OnConflictTarget],
        sql: &mut dyn SqlWriter,
    ) {
        if on_conflict_targets.is_empty() {
            return;
        }

        write!(sql, "(").unwrap();
        on_conflict_targets.iter().fold(true, |first, target| {
            if !first {
                write!(sql, ", ").unwrap()
            }
            match target {
                OnConflictTarget::ConflictColumn(col) => {
                    col.prepare(sql.as_writer(), self.quote());
                }

                OnConflictTarget::ConflictExpr(expr) => {
                    self.prepare_simple_expr(expr, sql);
                }
            }
            false
        });
        write!(sql, ")").unwrap();
    }

    #[doc(hidden)]
    /// Write ON CONFLICT action
    fn prepare_on_conflict_action(
        &self,
        on_conflict_action: &Option<OnConflictAction>,
        sql: &mut dyn SqlWriter,
    ) {
        self.prepare_on_conflict_action_common(on_conflict_action, sql);
    }

    fn prepare_on_conflict_action_common(
        &self,
        on_conflict_action: &Option<OnConflictAction>,
        sql: &mut dyn SqlWriter,
    ) {
        if let Some(action) = on_conflict_action {
            match action {
                OnConflictAction::DoNothing(_) => {
                    write!(sql, " DO NOTHING").unwrap();
                }
                OnConflictAction::Update(update_strats) => {
                    self.prepare_on_conflict_do_update_keywords(sql);
                    update_strats.iter().fold(true, |first, update_strat| {
                        if !first {
                            write!(sql, ", ").unwrap()
                        }
                        match update_strat {
                            OnConflictUpdate::Column(col) => {
                                col.prepare(sql.as_writer(), self.quote());
                                write!(sql, " = ").unwrap();
                                self.prepare_on_conflict_excluded_table(col, sql);
                            }
                            OnConflictUpdate::Expr(col, expr) => {
                                col.prepare(sql.as_writer(), self.quote());
                                write!(sql, " = ").unwrap();
                                self.prepare_simple_expr(expr, sql);
                            }
                        }
                        false
                    });
                }
            }
        }
    }

    #[doc(hidden)]
    /// Write ON CONFLICT keywords
    fn prepare_on_conflict_keywords(&self, sql: &mut dyn SqlWriter) {
        write!(sql, " ON CONFLICT ").unwrap();
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
    fn prepare_on_conflict_condition(
        &self,
        on_conflict_condition: &ConditionHolder,
        sql: &mut dyn SqlWriter,
    ) {
        self.prepare_condition(on_conflict_condition, "WHERE", sql)
    }

    #[doc(hidden)]
    /// Hook to insert "OUTPUT" expressions.
    fn prepare_output(&self, _returning: &Option<ReturningClause>, _sql: &mut dyn SqlWriter) {}

    #[doc(hidden)]
    /// Hook to insert "RETURNING" statements.
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
    fn prepare_condition(
        &self,
        condition: &ConditionHolder,
        keyword: &str,
        sql: &mut dyn SqlWriter,
    ) {
        match &condition.contents {
            ConditionHolderContents::Empty => (),
            ConditionHolderContents::Chain(conditions) => {
                write!(sql, " {keyword} ").unwrap();
                for (i, log_chain_oper) in conditions.iter().enumerate() {
                    self.prepare_logical_chain_oper(log_chain_oper, i, conditions.len(), sql);
                }
            }
            ConditionHolderContents::Condition(c) => {
                write!(sql, " {keyword} ").unwrap();
                self.prepare_condition_where(c, sql);
            }
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
    fn prepare_frame(&self, frame: &Frame, sql: &mut dyn SqlWriter) {
        match *frame {
            Frame::UnboundedPreceding => write!(sql, "UNBOUNDED PRECEDING").unwrap(),
            Frame::Preceding(v) => {
                self.prepare_value(&v.into(), sql);
                write!(sql, "PRECEDING").unwrap();
            }
            Frame::CurrentRow => write!(sql, "CURRENT ROW").unwrap(),
            Frame::Following(v) => {
                self.prepare_value(&v.into(), sql);
                write!(sql, "FOLLOWING").unwrap();
            }
            Frame::UnboundedFollowing => write!(sql, "UNBOUNDED FOLLOWING").unwrap(),
        }
    }

    #[doc(hidden)]
    /// Translate [`WindowStatement`] into SQL statement.
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
        self.prepare_simple_expr(left, sql);
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

        // Due to representation of trinary op like/not like with optional arg escape as nested binary ops.
        let drop_right_escape_hack = op_as_oper.is_like()
            && right.is_binary()
            && matches!(right.get_bin_oper(), Some(&BinOper::Escape));

        // Due to custom representation of casting AS datatype
        let drop_right_as_hack = (op == &BinOper::As) && matches!(right, SimpleExpr::Custom(_));

        let right_paren = !drop_right_higher_precedence
            && !drop_right_escape_hack
            && !drop_right_between_hack
            && !drop_right_as_hack;
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
    fn prepare_column_auto_increment(&self, column_type: &ColumnType, sql: &mut dyn SqlWriter) {
        match &column_type {
            ColumnType::SmallInteger => write!(sql, "smallserial").unwrap(),
            ColumnType::Integer => write!(sql, "serial").unwrap(),
            ColumnType::BigInteger => write!(sql, "bigserial").unwrap(),
            _ => unimplemented!("{:?} doesn't support auto increment", column_type),
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
                ColumnType::TinyInteger | ColumnType::TinyUnsigned => "smallint".into(),
                ColumnType::SmallInteger | ColumnType::SmallUnsigned => "smallint".into(),
                ColumnType::Integer | ColumnType::Unsigned => "integer".into(),
                ColumnType::BigInteger | ColumnType::BigUnsigned => "bigint".into(),
                ColumnType::Float => "real".into(),
                ColumnType::Double => "double precision".into(),
                ColumnType::Decimal(precision) => match precision {
                    Some((precision, scale)) => format!("decimal({precision}, {scale})"),
                    None => "decimal".into(),
                },
                ColumnType::DateTime => "timestamp without time zone".into(),
                ColumnType::Timestamp => "timestamp".into(),
                ColumnType::TimestampWithTimeZone => "timestamp with time zone".into(),
                ColumnType::Time => "time".into(),
                ColumnType::Date => "date".into(),
                ColumnType::Interval(fields, precision) => {
                    let mut typ = "interval".to_string();
                    if let Some(fields) = fields {
                        write!(typ, " {fields}").unwrap();
                    }
                    if let Some(precision) = precision {
                        write!(typ, "({precision})").unwrap();
                    }
                    typ
                }
                ColumnType::Binary(_) | ColumnType::VarBinary(_) | ColumnType::Blob =>
                    "bytea".into(),
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
                ColumnType::Money(precision) => match precision {
                    Some((precision, scale)) => format!("money({precision}, {scale})"),
                    None => "money".into(),
                },
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
                ColumnType::Year => unimplemented!("Year is not available in Postgres."),
                ColumnType::LTree => "ltree".into(),
            }
        )
        .unwrap()
    }

    fn column_spec_auto_increment_keyword(&self) -> &str {
        ""
    }

    pub(crate) fn prepare_table_alter_statement(
        &self,
        alter: &TableAlterStatement,
        sql: &mut dyn SqlWriter,
    ) {
        if alter.options.is_empty() {
            panic!("No alter option found")
        };
        write!(sql, "ALTER TABLE ").unwrap();
        if let Some(table) = &alter.table {
            self.prepare_table_ref_table_stmt(table, sql);
            write!(sql, " ").unwrap();
        }

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
                TableAlterOption::RenameColumn(from_name, to_name) => {
                    write!(sql, "RENAME COLUMN ").unwrap();
                    from_name.prepare(sql.as_writer(), self.quote());
                    write!(sql, " TO ").unwrap();
                    to_name.prepare(sql.as_writer(), self.quote());
                }
                TableAlterOption::DropColumn(column_name) => {
                    write!(sql, "DROP COLUMN ").unwrap();
                    column_name.prepare(sql.as_writer(), self.quote());
                }
                TableAlterOption::DropForeignKey(name) => {
                    let mut foreign_key = TableForeignKey::new();
                    foreign_key.name(name.to_string());
                    let drop = ForeignKeyDropStatement {
                        foreign_key,
                        table: None,
                    };
                    self.prepare_foreign_key_drop_statement_internal(&drop, sql, Mode::TableAlter);
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

    pub(crate) fn prepare_table_rename_statement(
        &self,
        rename: &TableRenameStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER TABLE ").unwrap();
        if let Some(from_name) = &rename.from_name {
            self.prepare_table_ref_table_stmt(from_name, sql);
        }
        write!(sql, " RENAME TO ").unwrap();
        if let Some(to_name) = &rename.to_name {
            self.prepare_table_ref_table_stmt(to_name, sql);
        }
    }

    /// Translate [`TableCreateStatement`] into SQL statement.
    pub(crate) fn prepare_table_create_statement(
        &self,
        create: &TableCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE TABLE ").unwrap();

        self.prepare_create_table_if_not_exists(create, sql);

        if let Some(table_ref) = &create.table {
            self.prepare_table_ref_table_stmt(table_ref, sql);
        }

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

        self.prepare_table_opt(create, sql);

        if let Some(extra) = &create.extra {
            write!(sql, " {extra}").unwrap();
        }
    }

    /// Translate [`TableRef`] into SQL statement.
    pub(crate) fn prepare_table_ref_table_stmt(
        &self,
        table_ref: &TableRef,
        sql: &mut dyn SqlWriter,
    ) {
        match table_ref {
            TableRef::Table(_)
            | TableRef::SchemaTable(_, _)
            | TableRef::DatabaseSchemaTable(_, _, _) => self.prepare_table_ref_iden(table_ref, sql),
            _ => panic!("Not supported"),
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
            ColumnSpec::Comment(comment) => self.column_comment(comment, sql),
        }
    }

    /// column comment
    fn column_comment(&self, _comment: &str, _sql: &mut dyn SqlWriter) {}

    /// Translate [`TableOpt`] into SQL statement.
    fn prepare_table_opt(&self, create: &TableCreateStatement, sql: &mut dyn SqlWriter) {
        self.prepare_table_opt_def(create, sql)
    }

    /// Default function
    fn prepare_table_opt_def(&self, create: &TableCreateStatement, sql: &mut dyn SqlWriter) {
        for table_opt in create.options.iter() {
            write!(sql, " ").unwrap();
            write!(
                sql,
                "{}",
                match table_opt {
                    TableOpt::Engine(s) => format!("ENGINE={s}"),
                    TableOpt::Collate(s) => format!("COLLATE={s}"),
                    TableOpt::CharacterSet(s) => format!("DEFAULT CHARSET={s}"),
                }
            )
            .unwrap()
        }
    }

    // /// Translate [`TablePartition`] into SQL statement.
    // fn prepare_table_partition(&self, _table_partition: &TablePartition, _sql: &mut dyn SqlWriter) {
    // }

    /// Translate [`TableDropStatement`] into SQL statement.
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
            self.prepare_table_ref_table_stmt(table, sql);
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

        if let Some(table) = &truncate.table {
            self.prepare_table_ref_table_stmt(table, sql);
        }
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
        gen: &SimpleExpr,
        stored: bool,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "GENERATED ALWAYS AS (").unwrap();
        QueryBuilder::prepare_simple_expr(self, gen, sql);
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
            write!(
                sql,
                "CONSTRAINT {}{}{} ",
                self.quote().left(),
                name,
                self.quote().right()
            )
            .unwrap();
        }

        self.prepare_index_prefix(create, sql);

        if create.nulls_not_distinct {
            write!(sql, "NULLS NOT DISTINCT ").unwrap();
        }

        self.prepare_index_columns(&create.index.columns, sql);
    }

    pub(crate) fn prepare_index_create_statement(
        &self,
        create: &IndexCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE ").unwrap();
        self.prepare_index_prefix(create, sql);
        write!(sql, "INDEX ").unwrap();

        if create.if_not_exists {
            write!(sql, "IF NOT EXISTS ").unwrap();
        }

        if let Some(name) = &create.index.name {
            write!(
                sql,
                "{}{}{}",
                self.quote().left(),
                name,
                self.quote().right()
            )
            .unwrap();
        }

        write!(sql, " ON ").unwrap();
        if let Some(table) = &create.table {
            self.prepare_table_ref_index_stmt(table, sql);
        }

        self.prepare_index_type(&create.index_type, sql);
        write!(sql, " ").unwrap();
        self.prepare_index_columns(&create.index.columns, sql);

        if create.nulls_not_distinct {
            write!(sql, " NULLS NOT DISTINCT").unwrap();
        }
    }

    fn prepare_table_ref_index_stmt(&self, table_ref: &TableRef, sql: &mut dyn SqlWriter) {
        match table_ref {
            TableRef::Table(_) | TableRef::SchemaTable(_, _) => {
                self.prepare_table_ref_iden(table_ref, sql)
            }
            _ => panic!("Not supported"),
        }
    }

    pub(crate) fn prepare_index_drop_statement(
        &self,
        drop: &IndexDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP INDEX ").unwrap();

        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }

        if let Some(table) = &drop.table {
            match table {
                TableRef::Table(_) => {}
                TableRef::SchemaTable(schema, _) => {
                    schema.prepare(sql.as_writer(), self.quote());
                    write!(sql, ".").unwrap();
                }
                _ => panic!("Not supported"),
            }
        }
        if let Some(name) = &drop.index.name {
            write!(
                sql,
                "{}{}{}",
                self.quote().left(),
                name,
                self.quote().right()
            )
            .unwrap();
        }
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

    fn prepare_index_prefix(&self, create: &IndexCreateStatement, sql: &mut dyn SqlWriter) {
        if create.primary {
            write!(sql, "PRIMARY KEY ").unwrap();
        }
        if create.unique {
            write!(sql, "UNIQUE ").unwrap();
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

    fn prepare_foreign_key_drop_statement_internal(
        &self,
        drop: &ForeignKeyDropStatement,
        sql: &mut dyn SqlWriter,
        mode: Mode,
    ) {
        if mode == Mode::Alter {
            write!(sql, "ALTER TABLE ").unwrap();
            if let Some(table) = &drop.table {
                self.prepare_table_ref_fk_stmt(table, sql);
            }
            write!(sql, " ").unwrap();
        }

        write!(sql, "DROP CONSTRAINT ").unwrap();
        if let Some(name) = &drop.foreign_key.name {
            write!(
                sql,
                "{}{}{}",
                self.quote().left(),
                name,
                self.quote().right()
            )
            .unwrap();
        }
    }

    fn prepare_foreign_key_create_statement_internal(
        &self,
        create: &ForeignKeyCreateStatement,
        sql: &mut dyn SqlWriter,
        mode: Mode,
    ) {
        if mode == Mode::Alter {
            write!(sql, "ALTER TABLE ").unwrap();
            if let Some(table) = &create.foreign_key.table {
                self.prepare_table_ref_fk_stmt(table, sql);
            }
            write!(sql, " ").unwrap();
        }

        if mode != Mode::Creation {
            write!(sql, "ADD ").unwrap();
        }

        if let Some(name) = &create.foreign_key.name {
            write!(sql, "CONSTRAINT ").unwrap();
            write!(
                sql,
                "{}{}{} ",
                self.quote().left(),
                name,
                self.quote().right()
            )
            .unwrap();
        }

        write!(sql, "FOREIGN KEY (").unwrap();
        create.foreign_key.columns.iter().fold(true, |first, col| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            col.prepare(sql.as_writer(), self.quote());
            false
        });
        write!(sql, ")").unwrap();

        write!(sql, " REFERENCES ").unwrap();
        if let Some(ref_table) = &create.foreign_key.ref_table {
            self.prepare_table_ref_fk_stmt(ref_table, sql);
        }
        write!(sql, " ").unwrap();

        write!(sql, "(").unwrap();
        create
            .foreign_key
            .ref_columns
            .iter()
            .fold(true, |first, col| {
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

    fn prepare_table_ref_fk_stmt(&self, table_ref: &TableRef, sql: &mut dyn SqlWriter) {
        match table_ref {
            TableRef::Table(_)
            | TableRef::SchemaTable(_, _)
            | TableRef::DatabaseSchemaTable(_, _, _) => self.prepare_table_ref_iden(table_ref, sql),
            _ => panic!("Not supported"),
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

    /// Translate [`ForeignKeyDropStatement`] into SQL statement.
    pub(crate) fn prepare_foreign_key_drop_statement(
        &self,
        drop: &ForeignKeyDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        self.prepare_foreign_key_drop_statement_internal(drop, sql, Mode::Alter)
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

    // TABLE REF
    /// Translate [`TableRef`] that without values into SQL statement.
    fn prepare_table_ref_iden(&self, table_ref: &TableRef, sql: &mut dyn SqlWriter) {
        match table_ref {
            TableRef::Table(iden) => {
                iden.prepare(sql.as_writer(), self.quote());
            }
            TableRef::SchemaTable(schema, table) => {
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                table.prepare(sql.as_writer(), self.quote());
            }
            TableRef::DatabaseSchemaTable(database, schema, table) => {
                database.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                table.prepare(sql.as_writer(), self.quote());
            }
            TableRef::TableAlias(iden, alias) => {
                iden.prepare(sql.as_writer(), self.quote());
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            TableRef::SchemaTableAlias(schema, table, alias) => {
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                table.prepare(sql.as_writer(), self.quote());
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            TableRef::DatabaseSchemaTableAlias(database, schema, table, alias) => {
                database.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                schema.prepare(sql.as_writer(), self.quote());
                write!(sql, ".").unwrap();
                table.prepare(sql.as_writer(), self.quote());
                write!(sql, " AS ").unwrap();
                alias.prepare(sql.as_writer(), self.quote());
            }
            TableRef::SubQuery(_, _)
            | TableRef::ValuesList(_, _)
            | TableRef::FunctionCall(_, _) => {
                panic!("TableRef with values is not support")
            }
        }
    }

    // TYPE BUILDER
    fn prepare_create_as_type(&self, as_type: &TypeAs, sql: &mut dyn SqlWriter) {
        write!(
            sql,
            "{}",
            match as_type {
                TypeAs::Enum => "ENUM",
            }
        )
        .unwrap()
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

    fn prepare_alter_type_opt(&self, opt: &TypeAlterOpt, sql: &mut dyn SqlWriter) {
        match opt {
            TypeAlterOpt::Add(value, placement) => {
                write!(sql, " ADD VALUE ").unwrap();
                match placement {
                    Some(add_option) => match add_option {
                        TypeAlterAddOpt::Before(before_value) => {
                            self.prepare_value(&value.to_string().into(), sql);
                            write!(sql, " BEFORE ").unwrap();
                            self.prepare_value(&before_value.to_string().into(), sql);
                        }
                        TypeAlterAddOpt::After(after_value) => {
                            self.prepare_value(&value.to_string().into(), sql);
                            write!(sql, " AFTER ").unwrap();
                            self.prepare_value(&after_value.to_string().into(), sql);
                        }
                    },
                    None => self.prepare_value(&value.to_string().into(), sql),
                }
            }
            TypeAlterOpt::Rename(new_name) => {
                write!(sql, " RENAME TO ").unwrap();
                self.prepare_value(&new_name.to_string().into(), sql);
            }
            TypeAlterOpt::RenameValue(existing, new_name) => {
                write!(sql, " RENAME VALUE ").unwrap();
                self.prepare_value(&existing.to_string().into(), sql);
                write!(sql, " TO ").unwrap();
                self.prepare_value(&new_name.to_string().into(), sql);
            }
        }
    }

    pub(crate) fn prepare_type_create_statement(
        &self,
        create: &TypeCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE TYPE ").unwrap();

        if let Some(name) = &create.name {
            self.prepare_type_ref(name, sql);
        }

        if let Some(as_type) = &create.as_type {
            write!(sql, " AS ").unwrap();
            self.prepare_create_as_type(as_type, sql);
        }

        if !create.values.is_empty() {
            write!(sql, " (").unwrap();

            for (count, val) in create.values.iter().enumerate() {
                if count > 0 {
                    write!(sql, ", ").unwrap();
                }
                self.prepare_value(&val.to_string().into(), sql);
            }

            write!(sql, ")").unwrap();
        }
    }

    pub(crate) fn prepare_type_drop_statement(
        &self,
        drop: &TypeDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP TYPE ").unwrap();

        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }

        drop.names.iter().fold(true, |first, name| {
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

    pub(crate) fn prepare_type_alter_statement(
        &self,
        alter: &TypeAlterStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER TYPE ").unwrap();

        if let Some(name) = &alter.name {
            self.prepare_type_ref(name, sql);
        }

        if let Some(option) = &alter.option {
            self.prepare_alter_type_opt(option, sql)
        }
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
    pub(crate) fn prepare_extension_create_statement(
        &self,
        create: &ExtensionCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE EXTENSION ").unwrap();

        if create.if_not_exists {
            write!(sql, "IF NOT EXISTS ").unwrap()
        }

        write!(sql, "{}", create.name).unwrap();

        if let Some(schema) = create.schema.as_ref() {
            write!(sql, " WITH SCHEMA {}", schema).unwrap();
        }

        if let Some(version) = create.version.as_ref() {
            write!(sql, " VERSION {}", version).unwrap();
        }

        if create.cascade {
            write!(sql, " CASCADE").unwrap();
        }
    }

    pub(crate) fn prepare_extension_drop_statement(
        &self,
        drop: &ExtensionDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP EXTENSION ").unwrap();

        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }

        write!(sql, "{}", drop.name).unwrap();

        if drop.cascade {
            write!(sql, " CASCADE").unwrap();
        }

        if drop.restrict {
            write!(sql, " RESTRICT").unwrap();
        }
    }

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
    )
}

fn is_ilike(b: &BinOper) -> bool {
    matches!(b, BinOper::ILike | BinOper::NotILike)
}

impl SubQueryStatement {
    pub(crate) fn prepare_statement(&self, query_builder: &QueryBuilder, sql: &mut dyn SqlWriter) {
        use SubQueryStatement::*;
        match self {
            SelectStatement(stmt) => query_builder.prepare_select_statement(stmt, sql),
            InsertStatement(stmt) => query_builder.prepare_insert_statement(stmt, sql),
            UpdateStatement(stmt) => query_builder.prepare_update_statement(stmt, sql),
            DeleteStatement(stmt) => query_builder.prepare_delete_statement(stmt, sql),
            WithStatement(stmt) => query_builder.prepare_with_query(stmt, sql),
        }
    }
}

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
