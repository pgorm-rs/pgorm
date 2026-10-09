//! `queryInt`: the binding's first asynchronous operation, a bound statement
//! run through pgorm's pool whose result settles a JavaScript promise.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, PoisonError},
    time::Duration,
};

use neon::prelude::*;
use pgorm::{ConnectionTrait, DatabasePool, types::ToSql};

use crate::{
    errors::{Failure, Redactions, failure},
    settle,
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

/// Each parameter as an `int4`. A value with no exact `int4` — a fraction, a
/// non-finite number, one outside the 32-bit range, anything but a number — is
/// refused rather than rounded, wrapped or stringified.
fn int4_params<'cx>(
    cx: &mut FunctionContext<'cx>,
    values: &[Handle<'cx, JsValue>],
) -> Result<Vec<i32>, Failure> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let position = index + 1;
            let number = value
                .downcast::<JsNumber, _>(cx)
                .map_err(|_| {
                    Failure::Construction(format!("parameter ${position} is not a number"))
                })?
                .value(cx);
            let in_range = number.fract() == 0.0
                && number >= f64::from(i32::MIN)
                && number <= f64::from(i32::MAX);
            if !in_range {
                return Err(Failure::Construction(format!(
                    "parameter ${position} ({number}) is not a 32-bit integer"
                )));
            }
            #[allow(clippy::cast_possible_truncation)]
            Ok(number as i32)
        })
        .collect()
}

async fn run(source: Source, sql: String, params: Vec<i32>) -> Result<i32, Failure> {
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
    let row = connection
        .query_one(&sql, &bound)
        .await
        .map_err(|error| failure(&error, secrets))?;
    row.try_get::<_, i32>(0)
        .map_err(|error| failure(&pgorm::Error::Postgres(error), secrets))
}

/// `queryInt(dsn, sql, params)`: run `sql` with each of `params` bound as an
/// `int4`, on the pool for `dsn`, and resolve with the `int4` in the first
/// column of the one row it returns.
pub(crate) fn query_int(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let dsn = cx.argument::<JsString>(0)?.value(&mut cx);
    let sql = cx.argument::<JsString>(1)?.value(&mut cx);
    let values = cx.argument::<JsArray>(2)?.to_vec(&mut cx)?;
    let prepared = int4_params(&mut cx, &values).and_then(|params| Ok((source(&dsn)?, params)));
    match prepared {
        Ok((source, params)) => settle::promise(&mut cx, run(source, sql, params), |cx, value| {
            Ok(cx.number(value).upcast())
        }),
        Err(failure) => settle::rejected(&mut cx, failure),
    }
}
