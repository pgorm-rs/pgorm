//! `query`: a bound statement run through pgorm's pool, whose decoded rows
//! settle a JavaScript promise.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, PoisonError},
    time::Duration,
};

use neon::prelude::*;
use pgorm::{ConnectionTrait, DatabasePool, types::ToSql};

use crate::{
    codec::Codec,
    errors::{Failure, Redactions, failure},
    params::{self, Param},
    rows::{self, Decoded},
    settle,
    values::read,
};

/// Connections each data source's pool keeps open.
const POOL_SIZE: usize = 10;

/// How long opening a connection may take when the connection string does not
/// say.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
struct Source {
    pool: DatabasePool,
    secrets: Redactions,
}

/// One pool per connection string, shared by every call that names it, so
/// concurrent calls queue for a bounded number of connections. Pools are never
/// closed; their idle connections hold nothing the JavaScript event loop waits
/// on.
static SOURCES: LazyLock<Mutex<HashMap<String, Source>>> = LazyLock::new(Mutex::default);

fn source(dsn: &str) -> Result<Source, Failure> {
    let mut sources = SOURCES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(source) = sources.get(dsn) {
        return Ok(source.clone());
    }
    let mut config: pgorm::Config = dsn
        .parse()
        .map_err(|_| Failure::Construction("invalid PostgreSQL connection string".to_owned()))?;
    if config.get_connect_timeout().is_none() {
        config.connect_timeout(CONNECT_TIMEOUT);
    }
    let secrets = Redactions::from_config(&config);
    let pool = pgorm::connect_with_builder(config, |builder| builder.max_size(POOL_SIZE))
        .map_err(|error| failure(&error, &secrets))?;
    let source = Source { pool, secrets };
    sources.insert(dsn.to_owned(), source.clone());
    Ok(source)
}

async fn run(source: Source, sql: String, params: Vec<Param>) -> Result<Decoded, Failure> {
    let secrets = &source.secrets;
    let connection = source
        .pool
        .get()
        .await
        .map_err(|error| failure(&error, secrets))?;
    let bound: Vec<&(dyn ToSql + Sync)> = params
        .iter()
        .map(|param| param as &(dyn ToSql + Sync))
        .collect();
    let rows = connection
        .query_all(&sql, &bound)
        .await
        .map_err(|error| failure(&error, secrets))?;
    rows::decode(&rows)
}

/// `query(dsn, sql, params, tagged)`: run `sql` with `params` bound, on the
/// pool for `dsn`, and resolve with `[names, rows]`.
pub(crate) fn query(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let dsn = cx.argument::<JsString>(0)?.value(&mut cx);
    let sql = cx.argument::<JsValue>(1)?;
    let sql = read::sql(&mut cx, sql)?;
    let values = cx.argument::<JsArray>(2)?;
    let tagged = cx.argument::<JsBoolean>(3)?.value(&mut cx);
    let codec = Codec::get(&mut cx)?;
    let params = params::read(&mut cx, codec, values)?;
    match source(&dsn) {
        Ok(source) => settle::promise(&mut cx, run(source, sql, params), move |cx, decoded| {
            rows::to_js(cx, decoded, tagged)
        }),
        Err(failure) => settle::rejected(&mut cx, failure),
    }
}
