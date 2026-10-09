//! Transactions and savepoints. pgorm's `DatabaseTransaction` borrows its
//! connection, and a savepoint its parent, exclusively; that borrow cannot
//! cross into JavaScript, so a task on the runtime owns each one, nested
//! savepoint frames inside their parent's, and JavaScript's handles send it
//! owned commands. While a savepoint is open its parent's frame is suspended
//! inside it, holding the parent's lock: the parent refuses a statement, a
//! commit, a rollback or another savepoint until the child finishes, and
//! refuses a second request while one runs, rather than queue or race it.

use std::{fmt, sync::Arc, time::Duration};

use futures_util::future::BoxFuture;
use neon::{prelude::*, types::Finalize};
use pgorm::{DatabaseTransaction, IsolationLevel, TransactionMode, TransactionTrait};
use tokio::sync::{Mutex, OwnedMutexGuard, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{
    Abort, ConnectionHandle, ConnectionState, Operation, Outcome, Statement, abort_argument,
};
use crate::{
    errors::{Failure, failure},
    settle,
};

pub(super) fn export(cx: &mut ModuleContext) -> NeonResult<()> {
    cx.export_function("connectionBegin", connection_begin)?;
    cx.export_function("transactionRun", transaction_run)?;
    cx.export_function("transactionBegin", transaction_begin)?;
    cx.export_function("transactionFinish", transaction_finish)?;
    cx.export_function("transactionAbort", transaction_abort)?;
    cx.export_function("transactionWaitClosed", transaction_wait_closed)?;
    cx.export_function("transactionClosed", transaction_closed)?;
    Ok(())
}

type Finish = oneshot::Sender<Result<(), Failure>>;

enum Command {
    Run {
        statement: Statement,
        reply: oneshot::Sender<Result<Outcome, Failure>>,
    },
    Finish {
        commit: bool,
        reply: Finish,
    },
    Begin {
        child: Arc<State>,
        receiver: mpsc::UnboundedReceiver<Command>,
        opened: Finish,
        parent_lock: OwnedMutexGuard<()>,
    },
}

/// What every frame of one transaction shares: `aborted` ends the whole
/// transaction and discards its connection, `finished` fires once its task
/// has ended.
struct Root {
    aborted: CancellationToken,
    finished: CancellationToken,
}

/// One transaction or savepoint, as its JavaScript handle reaches it.
pub(crate) struct State {
    connection: Arc<ConnectionState>,
    commands: mpsc::UnboundedSender<Command>,
    busy: Arc<Mutex<()>>,
    abandoned: CancellationToken,
    finished: CancellationToken,
    root: Arc<Root>,
    released: CancellationToken,
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("closed", &self.closed())
            .finish_non_exhaustive()
    }
}

/// Cancels a token when dropped, however the frame holding it ends.
struct Finished(CancellationToken);

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// A request whose outcome is unknown once abandoned: dropped before it
/// completes — its caller aborted — it ends the whole transaction, whose
/// connection is then discarded.
// [spec:pgorm:req:napi.cancellation]
struct Pending {
    aborted: CancellationToken,
    complete: bool,
}

impl Pending {
    fn new(aborted: CancellationToken) -> Self {
        Self {
            aborted,
            complete: false,
        }
    }

    fn complete(&mut self) {
        self.complete = true;
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.complete {
            self.aborted.cancel();
        }
    }
}

struct Completion {
    healthy: bool,
    reply: Option<Finish>,
    result: Result<(), Failure>,
}

impl Completion {
    fn discarded() -> Self {
        Self {
            healthy: false,
            reply: None,
            result: Err(Failure::Lifecycle(
                "the transaction's connection was discarded".to_owned(),
            )),
        }
    }

    fn respond(self) {
        if let Some(reply) = self.reply {
            let _ = reply.send(self.result);
        }
    }
}

impl State {
    fn channel(
        connection: Arc<ConnectionState>,
        parent: Option<&Arc<Self>>,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<Command>) {
        let (commands, receiver) = mpsc::unbounded_channel();
        let root = parent.map_or_else(
            || {
                Arc::new(Root {
                    aborted: CancellationToken::new(),
                    finished: CancellationToken::new(),
                })
            },
            |parent| parent.root.clone(),
        );
        let abandoned = parent.map_or_else(CancellationToken::new, |parent| {
            parent.abandoned.child_token()
        });
        (
            Arc::new(Self {
                connection,
                commands,
                busy: Arc::new(Mutex::new(())),
                abandoned,
                finished: CancellationToken::new(),
                root,
                released: CancellationToken::new(),
            }),
            receiver,
        )
    }

    fn closed(&self) -> bool {
        self.finished.is_cancelled()
            || self.abandoned.is_cancelled()
            || self.root.aborted.is_cancelled()
            || self.root.finished.is_cancelled()
            || self.connection.ensure_open().is_err()
    }

    fn ensure_open(&self) -> Result<(), Failure> {
        if self.closed() {
            Err(Failure::Lifecycle("the transaction is closed".to_owned()))
        } else {
            Ok(())
        }
    }

    /// The frame's lock, refusing rather than waiting when a request or an
    /// open savepoint holds it. Taken on the JavaScript thread as each request
    /// is made, so requests are refused in the order JavaScript made them.
    // [spec:pgorm:req:napi.transactions]
    fn lock(&self) -> Result<OwnedMutexGuard<()>, Failure> {
        self.ensure_open()?;
        self.busy.clone().try_lock_owned().map_err(|_| {
            Failure::Lifecycle(
                "the transaction is busy: a statement runs on it, or a savepoint of it is open"
                    .to_owned(),
            )
        })
    }

    fn send(&self, command: Command) -> Result<(), Failure> {
        self.commands
            .send(command)
            .map_err(|_| Failure::Lifecycle("the transaction has ended".to_owned()))
    }

    /// Run a statement, holding the lock taken as JavaScript started it.
    async fn run(
        &self,
        _lock: OwnedMutexGuard<()>,
        statement: Statement,
        abort: &Abort,
    ) -> Result<Outcome, Failure> {
        let (reply, receive) = oneshot::channel();
        let mut pending = Pending::new(self.root.aborted.clone());
        self.send(Command::Run { statement, reply })?;
        let outcome = abort
            .guard(async {
                receive
                    .await
                    .map_err(|_| Failure::Lifecycle("the transaction has ended".to_owned()))
            })
            .await?;
        pending.complete();
        outcome
    }

    async fn finish(&self, _lock: OwnedMutexGuard<()>, commit: bool) -> Result<(), Failure> {
        let (reply, receive) = oneshot::channel();
        let mut pending = Pending::new(self.root.aborted.clone());
        self.send(Command::Finish { commit, reply })?;
        let outcome = receive
            .await
            .map_err(|_| Failure::Lifecycle("the transaction has ended".to_owned()))?;
        pending.complete();
        outcome?;
        self.released.cancelled().await;
        Ok(())
    }

    /// A savepoint, which holds this frame's lock until it finishes.
    async fn begin(
        self: &Arc<Self>,
        parent_lock: OwnedMutexGuard<()>,
        abort: &Abort,
    ) -> Result<Arc<Self>, Failure> {
        let (child, receiver) = Self::channel(self.connection.clone(), Some(self));
        let (opened, receive) = oneshot::channel();
        let mut pending = Pending::new(self.root.aborted.clone());
        self.send(Command::Begin {
            child: child.clone(),
            receiver,
            opened,
            parent_lock,
        })?;
        abort
            .guard(async {
                receive
                    .await
                    .map_err(|_| Failure::Lifecycle("the transaction has ended".to_owned()))?
            })
            .await?;
        pending.complete();
        Ok(child)
    }
}

/// Serve one frame's commands until it commits or rolls back, a savepoint's
/// frame nested inside while it is open. Abandoned — its handle collected
/// unfinished — a frame rolls back within five seconds or discards the
/// connection.
// [spec:pgorm:req:napi.transactions]
fn serve<'a>(
    mut tx: DatabaseTransaction<'a>,
    state: Arc<State>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) -> BoxFuture<'a, Completion> {
    Box::pin(async move {
        let _finished = Finished(state.finished.clone());
        let secrets = state.connection.pool.secrets.clone();
        loop {
            let command = tokio::select! {
                () = state.abandoned.cancelled() => None,
                command = commands.recv() => command,
            };
            let Some(command) = command else {
                let rolled_back = tokio::time::timeout(Duration::from_secs(5), tx.rollback()).await;
                return Completion {
                    healthy: matches!(rolled_back, Ok(Ok(()))),
                    reply: None,
                    result: Ok(()),
                };
            };
            match command {
                Command::Run { statement, reply } => {
                    let outcome = tokio::select! {
                        () = state.abandoned.cancelled() => return Completion::discarded(),
                        outcome = statement.run(&tx, &secrets) => outcome,
                    };
                    if reply.send(outcome).is_err() {
                        return Completion::discarded();
                    }
                }
                Command::Finish { commit, reply } => {
                    let outcome = if commit {
                        tx.commit().await
                    } else {
                        tx.rollback().await
                    };
                    return Completion {
                        healthy: outcome.is_ok(),
                        reply: Some(reply),
                        result: outcome.map_err(|error| failure(&error, &secrets)),
                    };
                }
                Command::Begin {
                    child,
                    receiver,
                    opened,
                    parent_lock,
                } => {
                    let nested = match tx.begin().await {
                        Ok(nested) => nested,
                        Err(error) => {
                            if opened.send(Err(failure(&error, &secrets))).is_err() {
                                return Completion::discarded();
                            }
                            continue;
                        }
                    };
                    if opened.send(Ok(())).is_err() {
                        return Completion::discarded();
                    }
                    let completion = serve(nested, child.clone(), receiver).await;
                    let healthy = completion.healthy;
                    drop(parent_lock);
                    completion.respond();
                    child.released.cancel();
                    if !healthy {
                        return Completion::discarded();
                    }
                }
            }
        }
    })
}

/// Open a transaction on `connection`, which it holds until it finishes: a
/// task takes the connection out of its slot and owns the transaction.
// [spec:pgorm:req:napi.transactions]
async fn begin(
    mut operation: Operation,
    mode: TransactionMode,
    abort: Abort,
) -> Result<Arc<State>, Failure> {
    let connection = operation.state.clone();
    let (state, receiver) = State::channel(connection, None);
    let (opened, receive) = oneshot::channel::<Result<(), Failure>>();
    let mut pending = Pending::new(state.root.aborted.clone());
    let owner = state.clone();
    tokio::spawn(async move {
        let _finished = Finished(owner.root.finished.clone());
        let served = tokio::select! {
            () = owner.root.aborted.cancelled() => None,
            () = owner.connection.closing() => None,
            served = async {
                let secrets = owner.connection.pool.secrets.clone();
                let db = match operation.connection_mut() {
                    Ok(db) => db,
                    Err(error) => {
                        let _ = opened.send(Err(error));
                        return None;
                    }
                };
                let tx = match db.begin_with(mode).await {
                    Ok(tx) => tx,
                    Err(error) => {
                        let _ = opened.send(Err(failure(&error, &secrets)));
                        return None;
                    }
                };
                if opened.send(Ok(())).is_err() {
                    return None;
                }
                Some(serve(tx, owner.clone(), receiver).await)
            } => served,
        };
        match served {
            Some(completion) => {
                if completion.healthy {
                    operation.restore();
                } else {
                    drop(operation);
                }
                completion.respond();
                owner.released.cancel();
            }
            None => drop(operation),
        }
    });
    abort
        .guard(async {
            receive
                .await
                .map_err(|_| Failure::Lifecycle("the transaction ended as it began".to_owned()))?
        })
        .await?;
    pending.complete();
    Ok(state)
}

/// The JavaScript `Transaction`'s native half. Collected unfinished, it rolls
/// back as a backstop; `commit`, `rollback` and `close` are what end it.
#[derive(Debug)]
pub(crate) struct TransactionHandle(Arc<State>);

impl Finalize for TransactionHandle {}

impl Drop for TransactionHandle {
    fn drop(&mut self) {
        self.0.abandoned.cancel();
    }
}

fn state(cx: &mut FunctionContext) -> NeonResult<Arc<State>> {
    Ok(cx.argument::<JsBox<TransactionHandle>>(0)?.0.clone())
}

/// A transaction's mode and isolation level, named as JavaScript names them.
fn mode(cx: &mut FunctionContext, at: usize) -> NeonResult<TransactionMode> {
    let mode = cx.argument::<JsString>(at)?.value(cx);
    let isolation = cx.argument::<JsValue>(at + 1)?;
    let isolation = if isolation.is_a::<JsNull, _>(cx) || isolation.is_a::<JsUndefined, _>(cx) {
        None
    } else {
        let name = isolation.downcast_or_throw::<JsString, _>(cx)?.value(cx);
        Some(match name.as_str() {
            "readUncommitted" => IsolationLevel::ReadUncommitted,
            "readCommitted" => IsolationLevel::ReadCommitted,
            "repeatableRead" => IsolationLevel::RepeatableRead,
            "serializable" => IsolationLevel::Serializable,
            _ => {
                return crate::values::read::refuse(
                    cx,
                    "isolation is \"readUncommitted\", \"readCommitted\", \"repeatableRead\" or \
                     \"serializable\"",
                );
            }
        })
    };
    match (mode.as_str(), isolation) {
        ("default", None) => Ok(TransactionMode::Default),
        ("readWrite", isolation) => Ok(TransactionMode::ReadWrite { isolation }),
        ("readOnly", isolation) => Ok(TransactionMode::ReadOnly { isolation }),
        ("deferrable", None) => Ok(TransactionMode::DeferrableSnapshot),
        ("default" | "deferrable", Some(_)) => crate::values::read::refuse(
            cx,
            "the default and deferrable modes take no isolation level: choose readWrite or readOnly",
        ),
        _ => crate::values::read::refuse(
            cx,
            "mode is \"default\", \"readWrite\", \"readOnly\" or \"deferrable\"",
        ),
    }
}

/// `connectionBegin(connection, mode, isolation, abort)`.
fn connection_begin(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let connection = cx.argument::<JsBox<ConnectionHandle>>(0)?.0.clone();
    let mode = mode(&mut cx, 1)?;
    let abort = abort_argument(&mut cx, 3)?;
    let operation = match Operation::begin(connection) {
        Ok(operation) => operation,
        Err(failure) => return settle::rejected(&mut cx, failure),
    };
    settle::promise(&mut cx, begin(operation, mode, abort), |cx, state| {
        Ok(cx.boxed(TransactionHandle(state)).upcast())
    })
}

/// `transactionRun(transaction, terminal, sql, params, tagged, abort)`.
fn transaction_run(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let state = state(&mut cx)?;
    let statement = Statement::read(&mut cx, 1)?;
    let tagged = cx.argument::<JsBoolean>(4)?.value(&mut cx);
    let abort = abort_argument(&mut cx, 5)?;
    let lock = match state.lock() {
        Ok(lock) => lock,
        Err(failure) => return settle::rejected(&mut cx, failure),
    };
    let work = async move { state.run(lock, statement, &abort).await };
    settle::promise(&mut cx, work, move |cx, outcome| {
        outcome.into_js(cx, tagged)
    })
}

/// `transactionBegin(transaction, abort)`: a savepoint.
fn transaction_begin(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let state = state(&mut cx)?;
    let abort = abort_argument(&mut cx, 1)?;
    let lock = match state.lock() {
        Ok(lock) => lock,
        Err(failure) => return settle::rejected(&mut cx, failure),
    };
    let work = async move { state.begin(lock, &abort).await };
    settle::promise(&mut cx, work, |cx, child| {
        Ok(cx.boxed(TransactionHandle(child)).upcast())
    })
}

/// `transactionFinish(transaction, commit)`: commit or roll back, resolving
/// once the parent — the connection, or the savepoint's transaction — is
/// free again.
fn transaction_finish(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let state = state(&mut cx)?;
    let commit = cx.argument::<JsBoolean>(1)?.value(&mut cx);
    let lock = match state.lock() {
        Ok(lock) => lock,
        Err(failure) => return settle::rejected(&mut cx, failure),
    };
    let work = async move { state.finish(lock, commit).await };
    settle::promise(&mut cx, work, |cx, ()| Ok(cx.undefined().upcast()))
}

/// `transactionAbort(transaction)`: end the whole transaction now,
/// discarding its connection, and resolve once its task has ended.
fn transaction_abort(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let state = state(&mut cx)?;
    state.root.aborted.cancel();
    let work = async move {
        state.root.finished.cancelled().await;
        Ok(())
    };
    settle::promise(&mut cx, work, |cx, ()| Ok(cx.undefined().upcast()))
}

/// `transactionWaitClosed(transaction)`: resolve once the frame has
/// released its parent.
fn transaction_wait_closed(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let state = state(&mut cx)?;
    let work = async move {
        tokio::select! {
            () = state.released.cancelled() => {}
            () = state.root.finished.cancelled() => {}
        }
        Ok(())
    };
    settle::promise(&mut cx, work, |cx, ()| Ok(cx.undefined().upcast()))
}

fn transaction_closed(mut cx: FunctionContext) -> JsResult<JsBoolean> {
    let closed = state(&mut cx)?.closed();
    Ok(cx.boolean(closed))
}
