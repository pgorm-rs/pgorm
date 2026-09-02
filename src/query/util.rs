use crate::QueryTrait;

/// This structure provides debug capabilities
// [spec:pgorm:def:query.build.debug-query]
#[derive(Debug)]
pub struct DebugQuery<'a, Q, T> {
    /// The query to debug
    pub query: &'a Q,
    /// The value of the query
    pub value: T,
}

macro_rules! debug_query_build {
    ($impl_obj:ty, $db_expr:expr) => {
        impl<'a, Q> DebugQuery<'a, Q, $impl_obj>
        where
            Q: QueryTrait,
        {
            /// This macro builds a [Statement] when invoked
            pub fn build(&self) -> Statement {
                let func = $db_expr;
                // let db_backend = func(self);
                self.query.build()
            }
        }
    };
}

// debug_query_build!(DbBackend, |x: &DebugQuery<_>| x.value);
// debug_query_build!(&DbBackend, |x: &DebugQuery<_>| *x.value);
// debug_query_build!(DatabaseConnection, |x: &DebugQuery<
//     _,
//     DatabaseConnection,
// >| x.value.get_database_backend());
// debug_query_build!(&DatabaseConnection, |x: &DebugQuery<
//     _,
//     &DatabaseConnection,
// >| x.value.get_database_backend());

/// Helper to get a statement from an object that impl `QueryTrait`.
///
/// # Non-functional
///
/// This macro expands to `DebugQuery { .. }.build()`, but every
/// `debug_query_build!` invocation that would generate those `build` impls is
/// commented out, so [`DebugQuery`] has no `build` method and this macro has no
/// working expansion target. It is retained only for API compatibility.
///
/// Use [`QueryTrait::build`](crate::QueryTrait::build) for the parameterised
/// statement, or `as_query().to_string(QueryBuilder)` for a value-inlined one:
///
/// ```
/// use pgorm::pgorm_query::QueryBuilder;
/// use pgorm::{entity::*, query::*, tests_cfg::cake};
///
/// let c = cake::Entity::insert(cake::ActiveModel {
///     id: ActiveValue::set(1),
///     name: ActiveValue::set("Apple Pie".to_owned()),
/// });
///
/// assert_eq!(
///     c.build().0,
///     r#"INSERT INTO "cake" ("id", "name") VALUES ($1, $2)"#
/// );
/// assert_eq!(
///     c.as_query().to_string(QueryBuilder),
///     r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#
/// );
/// ```
// [spec:pgorm:def:query.build.debug-query]
#[macro_export]
macro_rules! debug_query_stmt {
    ($query:expr,$value:expr) => {
        $crate::DebugQuery {
            query: $query,
            value: $value,
        }
        .build();
    };
}

/// Helper to get a raw SQL string from an object that impl `QueryTrait`.
///
/// # Non-functional
///
/// This macro delegates to [`debug_query_stmt!`], which has no working
/// expansion target — see that macro's documentation. Use
/// `as_query().to_string(QueryBuilder)` to render a query with its values
/// inlined:
///
/// ```
/// use pgorm::pgorm_query::QueryBuilder;
/// use pgorm::{entity::*, query::*, tests_cfg::cake};
///
/// let c = cake::Entity::insert(cake::ActiveModel {
///     id: ActiveValue::set(1),
///     name: ActiveValue::set("Apple Pie".to_owned()),
/// });
///
/// assert_eq!(
///     c.as_query().to_string(QueryBuilder),
///     r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#
/// );
/// ```
#[macro_export]
macro_rules! debug_query {
    ($query:expr,$value:expr) => {
        $crate::debug_query_stmt!($query, $value).to_string();
    };
}
