//! How a test tells whether a pool cleared a connection's statements: by the
//! prepared statement the connection's session holds, not by what it writes.
//!
//! A clear closes every statement a connection holds, and the next use
//! prepares the statement again under a new name, so a session that answers
//! [`PREPARED_STATEMENT`] the same before and after has not been cleared in
//! between, and one that answers differently, or nothing, has.
//!
//! What a statement writes cannot tell. A domain's changed default reaches no
//! plan PostgreSQL holds (`sql.ddl.domain`), but PostgreSQL re-plans every
//! statement a session keeps prepared, under the same name, whenever that
//! session's caches are reset: `DISCARD PLANS`, a schema created in its
//! database, or the session falling behind the server's shared invalidation
//! queue, which catalogue churn in any database on the server can cause. A
//! statement prepared before the default changed can then write the new
//! default with no clear at all, and the old one is no proof of none.

/// The prepared statement a session holds whose text begins with `$1`, as
/// `pg_prepared_statements` names and stamps it: `<name> prepared at
/// <prepare_time>`. A session holding none answers no row, and one holding
/// two answers two, which `query_opt` refuses.
pub const PREPARED_STATEMENT: &str = "SELECT name || ' prepared at ' || prepare_time \
                                      FROM pg_prepared_statements WHERE starts_with(statement, $1)";
