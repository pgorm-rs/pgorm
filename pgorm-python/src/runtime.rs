use std::{
    fmt,
    sync::{Arc, Mutex as RegistryMutex, Weak},
    time::Duration,
};

use pgorm::ConnectionTrait;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use crate::{
    config::PoolConfig,
    errors::{InternalError, LifecycleError, Redactions, TimeoutError, database_error},
};

pub(crate) struct PoolState {
    pub(crate) pool: pgorm::DatabasePool,
    pub(crate) secrets: Redactions,
    pub(crate) cancelled: CancellationToken,
    owner: Py<PyAny>,
    acquire_timeout: Duration,
    connections: RegistryMutex<Vec<Weak<ConnectionState>>>,
}

impl fmt::Debug for PoolState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PoolState")
            .field("closed", &self.cancelled.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl PoolState {
    pub(crate) fn check_owner(&self, py: Python<'_>) -> PyResult<()> {
        let current = running_loop(py)?;
        if !self.owner.bind(py).is(&current) {
            return Err(LifecycleError::new_err(
                "pool belongs to another asyncio event loop",
            ));
        }
        Ok(())
    }

    pub(crate) fn ensure_open(&self) -> PyResult<()> {
        if self.cancelled.is_cancelled() {
            Err(LifecycleError::new_err("pool is closed"))
        } else {
            Ok(())
        }
    }

    fn close_now(&self) -> PyResult<Vec<Arc<ConnectionState>>> {
        self.cancelled.cancel();
        self.pool.close();
        let mut registry = self
            .connections
            .lock()
            .map_err(|_| InternalError::new_err("connection registry lock failed"))?;
        let connections: Vec<_> = registry.iter().filter_map(Weak::upgrade).collect();
        registry.clear();
        for connection in &connections {
            connection.close_now();
        }
        Ok(connections)
    }
}

fn running_loop(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("asyncio")?
        .call_method0("get_running_loop")
        .map_err(|_| {
            LifecycleError::new_err("pgorm resources require a running asyncio event loop")
        })
}

// [spec:pgorm:req:python.runtime]
// [spec:pgorm:req:python.connections]
#[derive(Debug)]
#[pyclass(name = "_Pool", module = "pgorm._native", frozen)]
pub(crate) struct NativePool {
    pub(crate) state: Arc<PoolState>,
}

#[pymethods]
impl NativePool {
    #[new]
    #[pyo3(signature = (dsn, *, tls=None, ca_pem=None, max_size=10, connect_timeout=10.0, acquire_timeout=30.0, statement_cache_size=128, recycle="verified"))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        dsn: String,
        tls: Option<String>,
        ca_pem: Option<Vec<u8>>,
        max_size: usize,
        connect_timeout: f64,
        acquire_timeout: f64,
        statement_cache_size: usize,
        recycle: &str,
    ) -> PyResult<Self> {
        let owner = running_loop(py)?.unbind();
        let (pool, secrets, acquire_timeout) = PoolConfig {
            dsn,
            tls,
            ca_pem,
            max_size,
            connect_timeout,
            acquire_timeout,
            statement_cache_size,
            recycle: recycle.to_owned(),
        }
        .build()?;
        Ok(Self {
            state: Arc::new(PoolState {
                pool,
                secrets,
                owner,
                acquire_timeout,
                cancelled: CancellationToken::new(),
                connections: RegistryMutex::new(Vec::new()),
            }),
        })
    }

    fn acquire<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state.check_owner(py)?;
        self.state.ensure_open()?;
        let pool = self.state.clone();
        future_into_py(py, async move {
            let connection = tokio::select! {
                _ = pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
                result = tokio::time::timeout(pool.acquire_timeout, pool.pool.get()) => {
                    pool.ensure_open()?;
                    result.map_err(|_| TimeoutError::new_err("pool acquisition timed out"))?
                        .map_err(|error| database_error(error, &pool.secrets))?
                }
            };
            if pool.cancelled.is_cancelled() {
                connection.discard();
                return Err(LifecycleError::new_err("pool is closed"));
            }
            let state = Arc::new(ConnectionState {
                pool: pool.clone(),
                cancelled: CancellationToken::new(),
                slot: Arc::new(Mutex::new(Some(connection))),
            });
            let mut registry = pool
                .connections
                .lock()
                .map_err(|_| InternalError::new_err("connection registry lock failed"))?;
            if pool.cancelled.is_cancelled() {
                state.close_now();
                return Err(LifecycleError::new_err("pool is closed"));
            }
            registry.retain(|item| item.strong_count() > 0);
            registry.push(Arc::downgrade(&state));
            Ok(NativeConnection { state })
        })
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state.check_owner(py)?;
        let connections = self.state.close_now()?;
        future_into_py(py, async move {
            for connection in connections {
                drop(connection.slot.lock().await.take());
            }
            Ok(())
        })
    }

    fn closed(&self) -> bool {
        self.state.cancelled.is_cancelled()
    }

    fn status(&self, py: Python<'_>) -> PyResult<(usize, usize, usize, usize)> {
        self.state.check_owner(py)?;
        let status = self.state.pool.status();
        Ok((
            status.max_size,
            status.size,
            status.available,
            status.waiting,
        ))
    }

    fn __repr__(&self) -> String {
        format!("<pgorm native pool closed={}>", self.closed())
    }
}

impl Drop for NativePool {
    fn drop(&mut self) {
        // Cleanup may run after the Python event loop has stopped.
        self.state.cancelled.cancel();
        self.state.pool.close();
        if let Ok(registry) = self.state.connections.lock() {
            for state in registry.iter().filter_map(Weak::upgrade) {
                state.close_now();
            }
        }
    }
}

pub(crate) struct ConnectionState {
    pub(crate) pool: Arc<PoolState>,
    pub(crate) cancelled: CancellationToken,
    pub(crate) slot: Arc<Mutex<Option<pgorm::DatabaseConnection>>>,
}

impl fmt::Debug for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionState")
            .field("closed", &self.cancelled.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl ConnectionState {
    fn close_now(&self) {
        self.cancelled.cancel();
        if let Ok(mut slot) = self.slot.try_lock() {
            drop(slot.take());
        }
    }

    pub(crate) fn ensure_open(&self) -> PyResult<()> {
        self.pool.ensure_open()?;
        if self.cancelled.is_cancelled() {
            Err(LifecycleError::new_err("connection is closed"))
        } else {
            Ok(())
        }
    }
}

/// Cancellation drops the active connection instead of returning unknown state.
pub(crate) struct Operation {
    slot: OwnedMutexGuard<Option<pgorm::DatabaseConnection>>,
    connection: Option<pgorm::DatabaseConnection>,
    state: Arc<ConnectionState>,
}

impl Operation {
    pub(crate) fn begin(state: Arc<ConnectionState>) -> PyResult<Self> {
        state.ensure_open()?;
        let mut slot =
            state.slot.clone().try_lock_owned().map_err(|_| {
                LifecycleError::new_err("connection already has an active operation")
            })?;
        let connection = slot
            .take()
            .ok_or_else(|| LifecycleError::new_err("connection is closed"))?;
        Ok(Self {
            slot,
            connection: Some(connection),
            state,
        })
    }

    pub(crate) fn connection(&self) -> PyResult<&pgorm::DatabaseConnection> {
        self.connection
            .as_ref()
            .ok_or_else(|| InternalError::new_err("operation has no connection"))
    }

    pub(crate) fn connection_mut(&mut self) -> PyResult<&mut pgorm::DatabaseConnection> {
        self.connection
            .as_mut()
            .ok_or_else(|| InternalError::new_err("operation has no connection"))
    }

    pub(crate) fn restore(mut self) {
        if self.state.ensure_open().is_ok() {
            *self.slot = self.connection.take();
        }
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            self.state.cancelled.cancel();
            connection.discard();
        }
    }
}

#[derive(Debug)]
#[pyclass(name = "_Connection", module = "pgorm._native", frozen)]
pub(crate) struct NativeConnection {
    pub(crate) state: Arc<ConnectionState>,
}

#[pymethods]
impl NativeConnection {
    #[pyo3(signature=(*, mode="default", isolation=None))]
    fn begin<'py>(
        &self,
        py: Python<'py>,
        mode: &str,
        isolation: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.state.pool.check_owner(py)?;
        self.state.ensure_open()?;
        let mode = crate::transactions::mode(mode, isolation)?;
        let state = self.state.clone();
        future_into_py(py, async move {
            Ok(crate::transactions::NativeTransaction {
                state: crate::transactions::begin(state, mode).await?,
            })
        })
    }

    fn execute<'py>(
        &self,
        py: Python<'py>,
        query: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::results::execute(
            py,
            crate::execution::Target::Connection(self.state.clone()),
            query,
        )
    }

    fn fetch<'py>(
        &self,
        py: Python<'py>,
        query: &Bound<'_, PyAny>,
        mode: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::results::fetch(
            py,
            crate::execution::Target::Connection(self.state.clone()),
            query,
            mode,
        )
    }

    fn stream<'py>(
        &self,
        py: Python<'py>,
        query: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::results::open(py, self.state.clone(), query)
    }

    fn ping<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state.pool.check_owner(py)?;
        let state = self.state.clone();
        future_into_py(py, async move {
            let operation = Operation::begin(state.clone())?;
            let result = tokio::select! {
                _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
                _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
                result = operation.connection()?.query_one("SELECT TRUE", &[]) => result,
            };
            operation.restore();
            let row = result.map_err(|error| database_error(error, &state.pool.secrets))?;
            row.try_get::<_, bool>(0)
                .map_err(|error| database_error(error.into(), &state.pool.secrets))
        })
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state.pool.check_owner(py)?;
        self.state.close_now();
        let state = self.state.clone();
        future_into_py(py, async move {
            drop(state.slot.lock().await.take());
            Ok(())
        })
    }

    fn closed(&self) -> bool {
        self.state.cancelled.is_cancelled() || self.state.pool.cancelled.is_cancelled()
    }

    fn __repr__(&self) -> String {
        format!("<pgorm native connection closed={}>", self.closed())
    }
}

impl Drop for NativeConnection {
    fn drop(&mut self) {
        self.state.close_now();
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativePool>()?;
    module.add_class::<NativeConnection>()?;
    module.add_class::<crate::transactions::NativeTransaction>()?;
    Ok(())
}
