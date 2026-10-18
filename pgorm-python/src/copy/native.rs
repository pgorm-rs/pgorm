//! Native COPY handles. A COPY's sink or stream borrows its connection
//! exclusively, so the task owning the connection drives it — a spawned
//! pump for a Connection, the scope task for a Transaction — and the handle
//! talks to that task through bounded channels.

use std::{
    fmt,
    sync::{Arc, Mutex as StdMutex, MutexGuard},
};

use bytes::{Bytes, BytesMut};
use pgorm::Error;
use pyo3::{buffer::PyBuffer, prelude::*, types::PyBytes};
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{
    CopyFrom, CopyTo, PyCopyFrom, PyCopyTo,
    pump::{Message, Piece, QUEUED, drive_in, drive_out, open_in, open_out},
};
use crate::{
    errors::{ConstructionError, InternalError, LifecycleError, database_error},
    execution::Target,
    runtime::{ConnectionState, Operation},
    transactions::{Lease, State},
};

/// The largest `bytes` a COPY TO pull joins from chunks already received.
const GATHERED: usize = 64 * 1024;

type Settled = (Result<Option<u64>, Error>, Operation);

/// How a COPY that started ended, from the task that drove it.
enum Outcome<T> {
    /// The server refused the statement before any data moved.
    Refused(Error),
    Ran(T),
    /// The handle or its opening future went away mid-copy.
    Abandoned,
}

fn lock<T>(mutex: &StdMutex<T>) -> PyResult<MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| InternalError::new_err("COPY handle lock failed"))
}

fn copy_from(statement: &Bound<'_, PyAny>) -> PyResult<CopyFrom> {
    statement
        .extract::<PyRef<'_, PyCopyFrom>>()
        .map(|statement| statement.inner.clone())
        .map_err(|_| ConstructionError::new_err("copy_in requires a CopyFrom statement"))
}

fn copy_to(statement: &Bound<'_, PyAny>) -> PyResult<CopyTo> {
    statement
        .extract::<PyRef<'_, PyCopyTo>>()
        .map(|statement| statement.inner.clone())
        .map_err(|_| ConstructionError::new_err("copy_out requires a CopyTo statement"))
}

/// One chunk of COPY FROM data, copied out of any byte buffer.
fn chunk(value: &Bound<'_, PyAny>) -> PyResult<Bytes> {
    let buffer = PyBuffer::<u8>::get(value)
        .map_err(|_| ConstructionError::new_err("COPY data must be a bytes-like object"))?;
    Ok(Bytes::from(buffer.to_vec(value.py())?))
}

/// Hand back the result of a COPY FROM and, for a Connection, its lease.
enum Settle {
    Connection(oneshot::Receiver<Settled>),
    Transaction(Lease, oneshot::Receiver<Result<Option<u64>, Error>>),
}

impl Settle {
    /// A dropped wait drops the lease with it: an interrupted finish leaves
    /// the outcome unknown, so the connection — or the transaction — is
    /// discarded rather than reused.
    async fn wait(self, connection: &ConnectionState) -> PyResult<Option<u64>> {
        let result = match self {
            Self::Connection(settled) => match settled.await {
                Ok((result, operation)) => {
                    operation.restore();
                    result
                }
                Err(_) => {
                    connection.ensure_open()?;
                    return Err(LifecycleError::new_err("COPY ended without completing"));
                }
            },
            Self::Transaction(lease, settled) => {
                let result = settled
                    .await
                    .map_err(|_| LifecycleError::new_err("transaction is closed"))?;
                lease.complete();
                result
            }
        };
        result.map_err(|error| database_error(error, &connection.pool.secrets))
    }
}

/// Drive a COPY FROM on a Connection, owning its lease. Every early return
/// drops the lease, which discards the connection.
async fn pump_in(
    state: Arc<ConnectionState>,
    mut operation: Operation,
    statement: CopyFrom,
    mut messages: mpsc::Receiver<Message>,
    opened: oneshot::Sender<Result<(), Error>>,
    settled: oneshot::Sender<Settled>,
) {
    let mut opened = Some(opened);
    let outcome = tokio::select! {
        _ = state.cancelled.cancelled() => return,
        _ = state.pool.cancelled.cancelled() => return,
        outcome = async {
            let Ok(connection) = operation.connection_mut() else {
                return Outcome::Abandoned;
            };
            let sink = match open_in(connection, &statement).await {
                Ok(sink) => sink,
                Err(error) => return Outcome::Refused(error),
            };
            match opened.take().map(|opened| opened.send(Ok(()))) {
                Some(Ok(())) => Outcome::Ran(drive_in(sink, &mut messages).await),
                _ => Outcome::Abandoned,
            }
        } => outcome,
    };
    match outcome {
        // A refused COPY FROM closes its connection, so the lease is
        // dropped before the refusal is reported.
        Outcome::Refused(error) => {
            drop(operation);
            if let Some(opened) = opened {
                let _ = opened.send(Err(error));
            }
        }
        Outcome::Ran(result) => {
            let _ = settled.send((result, operation));
        }
        Outcome::Abandoned => {}
    }
}

/// Drive a COPY TO on a Connection, owning its lease. Every early return
/// drops the lease, which discards the connection: the server would
/// otherwise keep sending.
async fn pump_out(
    state: Arc<ConnectionState>,
    mut operation: Operation,
    statement: CopyTo,
    pieces: mpsc::Sender<Piece>,
    opened: oneshot::Sender<Result<(), Error>>,
) {
    let mut opened = Some(opened);
    let outcome = tokio::select! {
        _ = state.cancelled.cancelled() => return,
        _ = state.pool.cancelled.cancelled() => return,
        outcome = async {
            let Ok(connection) = operation.connection_mut() else {
                return Outcome::Abandoned;
            };
            let stream = match open_out(connection, &statement).await {
                Ok(stream) => stream,
                Err(error) => return Outcome::Refused(error),
            };
            match opened.take().map(|opened| opened.send(Ok(()))) {
                Some(Ok(())) => drive_out(stream, &pieces).await.map_or(Outcome::Abandoned, Outcome::Ran),
                _ => Outcome::Abandoned,
            }
        } => outcome,
    };
    match outcome {
        // A refused COPY TO leaves the connection idle, as any failed
        // statement does; it is restored before the refusal is reported.
        Outcome::Refused(error) => {
            operation.restore();
            if let Some(opened) = opened {
                let _ = opened.send(Err(error));
            }
        }
        Outcome::Ran(result) => {
            let _ = pieces
                .send(Piece::End(Box::new((result, Some(operation)))))
                .await;
        }
        Outcome::Abandoned => {}
    }
}

/// Start a COPY FROM on a Connection or Transaction and answer the handle
/// its data is written through.
// [spec:pgorm:req:python.copy+1]
#[pyfunction]
#[pyo3(name = "_copy_in")]
pub(crate) fn copy_in<'py>(
    py: Python<'py>,
    target: &Bound<'_, PyAny>,
    statement: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let target = Target::extract(py, target)?;
    let statement = copy_from(statement)?;
    future_into_py(py, async move {
        let (messages, receiver) = mpsc::channel(QUEUED);
        let (connection, settle) = match target {
            Target::Connection(state) => {
                let operation = Operation::begin(state.clone())?;
                let (opened, open) = oneshot::channel();
                let (settled, settle) = oneshot::channel();
                tokio::spawn(pump_in(
                    state.clone(),
                    operation,
                    statement,
                    receiver,
                    opened,
                    settled,
                ));
                match open.await {
                    Ok(Ok(())) => (state, Settle::Connection(settle)),
                    Ok(Err(error)) => return Err(database_error(error, &state.pool.secrets)),
                    Err(_) => {
                        state.ensure_open()?;
                        return Err(LifecycleError::new_err("COPY ended before it started"));
                    }
                }
            }
            Target::Transaction(state) => {
                let (lease, settle) = state.copy_in(statement, receiver).await?;
                (state.connection.clone(), Settle::Transaction(lease, settle))
            }
        };
        Ok(NativeCopyIn {
            connection,
            messages: StdMutex::new(Some(messages)),
            settle: StdMutex::new(Some(settle)),
        })
    })
}

/// Start a COPY TO on a Connection or Transaction and answer the stream its
/// data is read from.
// [spec:pgorm:req:python.copy+1]
#[pyfunction]
#[pyo3(name = "_copy_out")]
pub(crate) fn copy_out<'py>(
    py: Python<'py>,
    target: &Bound<'_, PyAny>,
    statement: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let target = Target::extract(py, target)?;
    let statement = copy_to(statement)?;
    future_into_py(py, async move {
        let (pieces, receiver) = mpsc::channel(QUEUED);
        let (connection, transaction, holder) = match target {
            Target::Connection(state) => {
                let operation = Operation::begin(state.clone())?;
                let (opened, open) = oneshot::channel();
                let stopped = CancellationToken::new();
                let pump = pump_out(state.clone(), operation, statement, pieces, opened);
                let done = stopped.clone();
                tokio::spawn(async move {
                    pump.await;
                    done.cancel();
                });
                match open.await {
                    Ok(Ok(())) => (state, None, Holder::Connection(stopped)),
                    Ok(Err(error)) => return Err(database_error(error, &state.pool.secrets)),
                    Err(_) => {
                        state.ensure_open()?;
                        return Err(LifecycleError::new_err("COPY ended before it started"));
                    }
                }
            }
            Target::Transaction(state) => {
                let lease = state.copy_out(statement, pieces).await?;
                (
                    state.connection.clone(),
                    Some(state.clone()),
                    Holder::Transaction(lease, state),
                )
            }
        };
        Ok(NativeCopyOut {
            state: Arc::new(OutState {
                connection,
                transaction,
                active: Arc::new(Mutex::new(Some(Active {
                    pieces: receiver,
                    stashed: None,
                    holder,
                }))),
                closing: CancellationToken::new(),
                finished: CancellationToken::new(),
            }),
        })
    })
}

// [spec:pgorm:req:python.copy+1]
// [spec:pgorm:req:python.cancellation]
/// The data side of a COPY FROM: `write` each chunk, then `finish`, or
/// `abort` to send `CopyFail`.
#[pyclass(name = "_CopyIn", module = "pgorm._native", frozen)]
pub(crate) struct NativeCopyIn {
    connection: Arc<ConnectionState>,
    messages: StdMutex<Option<mpsc::Sender<Message>>>,
    settle: StdMutex<Option<Settle>>,
}

impl fmt::Debug for NativeCopyIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let finished = self.settle.lock().map(|settle| settle.is_none());
        f.debug_struct("NativeCopyIn")
            .field("finished", &finished.unwrap_or(true))
            .finish_non_exhaustive()
    }
}

#[pymethods]
impl NativeCopyIn {
    /// Queue one chunk, waiting while the queue is full; `False` once the
    /// COPY has stopped taking data, whose cause `finish` reports.
    fn write<'py>(&self, py: Python<'py>, data: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.connection.pool.check_owner(py)?;
        let data = chunk(data)?;
        let messages = lock(&self.messages)?
            .clone()
            .ok_or_else(|| LifecycleError::new_err("COPY has ended"))?;
        future_into_py(py, async move {
            Ok(messages.send(Message::Data(data)).await.is_ok())
        })
    }

    fn finish<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.connection.pool.check_owner(py)?;
        let messages = lock(&self.messages)?.take();
        let settle = lock(&self.settle)?
            .take()
            .ok_or_else(|| LifecycleError::new_err("COPY has ended"))?;
        let connection = self.connection.clone();
        future_into_py(py, async move {
            if let Some(messages) = messages {
                let _ = messages.send(Message::Finish).await;
            }
            settle
                .wait(&connection)
                .await?
                .ok_or_else(|| InternalError::new_err("COPY was aborted before it finished"))
        })
    }

    /// Abandon the data: the server discards every row. The abandoned COPY's
    /// own outcome is not reported, so the exception that caused it is.
    fn abort<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.connection.pool.check_owner(py)?;
        drop(lock(&self.messages)?.take());
        let settle = lock(&self.settle)?.take();
        let connection = self.connection.clone();
        future_into_py(py, async move {
            if let Some(settle) = settle {
                let _ = settle.wait(&connection).await;
            }
            Ok(())
        })
    }

    fn closed(&self) -> PyResult<bool> {
        Ok(lock(&self.settle)?.is_none())
    }
}

/// What a COPY TO in progress holds.
enum Holder {
    /// Cancelled once the pump has restored or discarded the connection.
    Connection(CancellationToken),
    Transaction(Lease, Arc<State>),
}

struct Active {
    pieces: mpsc::Receiver<Piece>,
    stashed: Option<Piece>,
    holder: Holder,
}

impl Active {
    /// The data has ended and the owner is free for the next statement.
    fn complete(self) {
        if let Holder::Transaction(lease, _) = self.holder {
            lease.complete();
        }
    }

    /// Stop reading part way, which discards the connection — and with it a
    /// transaction — and wait until it is gone, so the close is observable.
    async fn abandon(self) {
        drop(self.pieces);
        match self.holder {
            Holder::Connection(stopped) => stopped.cancelled().await,
            Holder::Transaction(lease, state) => {
                drop(lease);
                state.released().await;
            }
        }
    }

    /// Join `first` with the chunks already received, up to `GATHERED`
    /// bytes. COPY promises no chunk boundary, so a pull is any slice of the
    /// byte stream; joining saves a Python round trip per row.
    fn gather(&mut self, first: Bytes) -> Bytes {
        let mut joined: Option<BytesMut> = None;
        let mut length = first.len();
        while length < GATHERED {
            match self.pieces.try_recv() {
                Ok(Piece::Chunk(next)) => {
                    length += next.len();
                    joined
                        .get_or_insert_with(|| BytesMut::from(&first[..]))
                        .extend_from_slice(&next);
                }
                Ok(end) => {
                    self.stashed = Some(end);
                    break;
                }
                Err(_) => break,
            }
        }
        joined.map_or(first, BytesMut::freeze)
    }
}

struct OutState {
    connection: Arc<ConnectionState>,
    transaction: Option<Arc<State>>,
    active: Arc<Mutex<Option<Active>>>,
    closing: CancellationToken,
    finished: CancellationToken,
}

impl OutState {
    fn ensure_open(&self) -> PyResult<()> {
        match &self.transaction {
            Some(transaction) if transaction.closed() => {
                Err(LifecycleError::new_err("transaction is closed"))
            }
            _ => self.connection.ensure_open(),
        }
    }
}

// [spec:pgorm:req:python.copy+1]
// [spec:pgorm:req:python.cancellation]
/// The data side of a COPY TO. The end of the data frees the connection or
/// transaction; closing before it discards the connection.
#[pyclass(name = "_CopyOut", module = "pgorm._native", frozen)]
pub(crate) struct NativeCopyOut {
    state: Arc<OutState>,
}

impl fmt::Debug for NativeCopyOut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeCopyOut")
            .field("closed", &self.closed())
            .finish()
    }
}

#[pymethods]
impl NativeCopyOut {
    fn next<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state.connection.pool.check_owner(py)?;
        let state = self.state.clone();
        future_into_py(py, async move {
            state.ensure_open()?;
            if state.finished.is_cancelled() {
                return Ok(None);
            }
            let mut slot = state.active.clone().try_lock_owned().map_err(|_| {
                LifecycleError::new_err("copy stream already has an active next operation")
            })?;
            let Some(active) = slot.as_mut() else {
                return Ok(None);
            };
            let piece = match active.stashed.take() {
                Some(piece) => Some(piece),
                None => tokio::select! {
                    _ = state.closing.cancelled() => return Err(LifecycleError::new_err("copy stream is closed")),
                    piece = active.pieces.recv() => piece,
                },
            };
            match piece {
                Some(Piece::Chunk(first)) => {
                    let data = active.gather(first);
                    Ok(Some(Python::attach(|py| PyBytes::new(py, &data).unbind())))
                }
                Some(Piece::End(end)) => {
                    let (result, operation) = *end;
                    if let Some(operation) = operation {
                        operation.restore();
                    }
                    if let Some(active) = slot.take() {
                        active.complete();
                    }
                    state.finished.cancel();
                    result
                        .map(|()| None)
                        .map_err(|error| database_error(error, &state.connection.pool.secrets))
                }
                None => {
                    drop(slot.take());
                    state.finished.cancel();
                    state.ensure_open()?;
                    Err(LifecycleError::new_err("COPY ended without completing"))
                }
            }
        })
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.state.connection.pool.check_owner(py)?;
        self.state.closing.cancel();
        let state = self.state.clone();
        future_into_py(py, async move {
            let active = state.active.lock().await.take();
            state.finished.cancel();
            if let Some(active) = active {
                active.abandon().await;
            }
            Ok(())
        })
    }

    fn closed(&self) -> bool {
        self.state.closing.is_cancelled() || self.state.finished.is_cancelled()
    }
}

impl Drop for NativeCopyOut {
    fn drop(&mut self) {
        self.state.closing.cancel();
        if let Ok(mut slot) = self.state.active.try_lock() {
            drop(slot.take());
        }
    }
}
