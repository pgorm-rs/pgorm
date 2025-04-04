#![allow(missing_docs)]

use std::{ops::DerefMut, pin::Pin, task::Poll};
use tracing::instrument;

use futures::{lock::MutexGuard, Stream};

use super::metric::MetricStream;
use crate::{DbErr, DatabaseConnection, QueryResult, Statement};

/// `TransactionStream` cannot be used in a `transaction` closure as it does not impl `Send`.
/// It seems to be a Rust limitation right now, and solution to work around this deemed to be extremely hard.
#[ouroboros::self_referencing]
pub struct TransactionStream<'a> {
    stmt: Statement,
    conn: MutexGuard<'a, DatabaseConnection>,
    metric_callback: Option<crate::metric::Callback>,
    #[borrows(mut conn, stmt, metric_callback)]
    #[not_covariant]
    stream: MetricStream<'this>,
}

impl<'a> std::fmt::Debug for TransactionStream<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TransactionStream")
    }
}

impl<'a> TransactionStream<'a> {
    #[instrument(level = "trace", skip(metric_callback))]
    #[allow(unused_variables)]
    pub(crate) fn build(
        conn: MutexGuard<'a, DatabaseConnection>,
        stmt: Statement,
        metric_callback: Option<crate::metric::Callback>,
    ) -> TransactionStream<'a> {
        TransactionStreamBuilder {
            stmt,
            conn,
            metric_callback,
            stream_builder: |conn, stmt, _metric_callback| match conn.deref_mut() {
                DatabaseConnection(c) => {
                    let query = crate::driver::sqlx_postgres::sqlx_query(stmt);
                    let _start = _metric_callback.is_some().then(std::time::SystemTime::now);
                    let stream = c
                        .fetch(query)
                        .map_ok(Into::into)
                        .map_err(sqlx_error_to_query_err);
                    let elapsed = _start.map(|s| s.elapsed().unwrap_or_default());
                    MetricStream::new(_metric_callback, stmt, elapsed, stream)
                }
                #[allow(unreachable_patterns)]
                _ => unreachable!(),
            },
        }
        .build()
    }
}

impl<'a> Stream for TransactionStream<'a> {
    type Item = Result<QueryResult, DbErr>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.with_stream_mut(|stream| Pin::new(stream).poll_next(cx))
    }
}
