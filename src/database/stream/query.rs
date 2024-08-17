#![allow(missing_docs, unreachable_code, unused_variables)]

use futures::Stream;
use std::{pin::Pin, task::Poll};
use tracing::instrument;

#[cfg(feature = "sqlx-dep")]
use futures::TryStreamExt;

#[cfg(feature = "sqlx-dep")]
use sqlx::Executor;

use super::metric::MetricStream;
#[cfg(feature = "sqlx-dep")]
use crate::driver::*;
use crate::{DbErr, DatabaseConnection, QueryResult, Statement};

/// Creates a stream from a [QueryResult]
#[ouroboros::self_referencing]
pub struct QueryStream {
    stmt: Statement,
    conn: DatabaseConnection,
    metric_callback: Option<crate::metric::Callback>,
    #[borrows(mut conn, stmt, metric_callback)]
    #[not_covariant]
    stream: MetricStream<'this>,
}

impl std::fmt::Debug for QueryStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "QueryStream")
    }
}

impl QueryStream {
    #[instrument(level = "trace", skip(metric_callback))]
    pub(crate) fn build(
        stmt: Statement,
        conn: DatabaseConnection,
        metric_callback: Option<crate::metric::Callback>,
    ) -> QueryStream {
        QueryStreamBuilder {
            stmt,
            conn,
            metric_callback,
            stream_builder: |conn, stmt, _metric_callback| match conn {
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

impl Stream for QueryStream {
    type Item = Result<QueryResult, DbErr>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.with_stream_mut(|stream| Pin::new(stream).poll_next(cx))
    }
}
