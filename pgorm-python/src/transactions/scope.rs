//! A recursive task frame serves each real borrowed transaction/savepoint
//! inside the closure pgorm opens it around.

use super::{
    state::{Finished, State},
    work::{Output, Work},
};
use crate::{
    copy::{Columns, CopyFrom, CopyTo, Message, Piece, Unopened},
    errors::database_error,
    execution::Database,
};
use futures_util::future::BoxFuture;
use pgorm::{DatabaseTransaction, Error, TransactionError, TransactionTrait};
use pyo3::PyResult;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::sync::{OwnedMutexGuard, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

type Reply = oneshot::Sender<Result<Output, Error>>;

/// The wait for a frame to open, answered with the error Python raises.
pub(super) type Opened = oneshot::Sender<PyResult<()>>;

pub(super) enum Command {
    Run {
        work: Work,
        reply: Reply,
    },
    Finish {
        commit: bool,
        reply: Reply,
    },
    Begin {
        child: Arc<State>,
        receiver: mpsc::UnboundedReceiver<Command>,
        opened: Opened,
        parent_lock: OwnedMutexGuard<()>,
    },
    CopyIn {
        statement: CopyFrom,
        rows: Option<Columns>,
        messages: mpsc::Receiver<Message>,
        opened: oneshot::Sender<Result<(), Unopened>>,
        reply: oneshot::Sender<Result<Option<u64>, Error>>,
    },
    CopyOut {
        statement: CopyTo,
        rows: Option<Columns>,
        pieces: mpsc::Sender<Piece>,
        opened: oneshot::Sender<Result<(), Unopened>>,
    },
}

pub(super) struct Completion {
    pub(super) healthy: bool,
    reply: Option<Reply>,
    result: Result<Output, Error>,
}

impl Completion {
    fn discarded() -> Self {
        Self {
            healthy: false,
            reply: None,
            result: Err(Error::Custom("transaction connection discarded".into())),
        }
    }

    fn finished(reply: Option<Reply>) -> Self {
        Self {
            healthy: true,
            reply,
            result: Ok(Output::Finished),
        }
    }

    pub(super) fn respond(self) {
        if let Some(reply) = self.reply {
            let _ = reply.send(self.result);
        }
    }
}

/// How a frame's command loop ended short of discarding it: the error its
/// closure returns, so that pgorm rolls the frame back.
pub(super) enum Exit {
    /// Python rolled the frame back; the reply goes once the rollback has.
    RolledBack(Reply),
    /// The frame's handle was dropped or its command channel closed.
    Abandoned,
}

/// What ends a frame by dropping its transaction's future rather than letting
/// it finish, which rolls back without awaiting the server: `discard`, which
/// the frame cancels as it gives its connection up, and `ending`, which an
/// abandoned frame's closure cancels as it returns, leaving the rollback five
/// seconds before the connection is discarded instead.
#[derive(Clone, Default)]
pub(super) struct Halt {
    discard: CancellationToken,
    ending: CancellationToken,
}

impl Halt {
    pub(super) async fn run<T>(&self, transaction: impl Future<Output = T>) -> Option<T> {
        tokio::select! {
            _ = self.discard.cancelled() => None,
            _ = async {
                self.ending.cancelled().await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            } => None,
            ended = transaction => Some(ended),
        }
    }

    /// Give the frame's connection up. The closure never returns, so pgorm
    /// awaits no rollback on a connection in an unknown state, and
    /// [`Halt::run`] drops the transaction instead.
    async fn discard<T>(&self) -> T {
        self.discard.cancel();
        std::future::pending().await
    }
}

/// How a frame's transaction ended, for the code that opened it.
pub(super) enum Ended {
    /// `BEGIN` or `SAVEPOINT` was refused, the closure never ran, and the
    /// wait for the frame to open is still to be answered.
    Refused(Opened, Error),
    Ran(Completion),
}

/// One frame: its commands, and what outlives the closure serving them.
pub(super) struct Frame {
    state: Arc<State>,
    commands: mpsc::UnboundedReceiver<Command>,
    /// Answered from inside the closure as it starts, so a refused opening
    /// finds it unanswered.
    opened: Option<Opened>,
    /// A commit's reply, held outside the closure because the `COMMIT` runs
    /// once the closure has returned and the reply carries its failure.
    committed: Option<Reply>,
    /// Set as the frame opens and dropped with the frame, once its `COMMIT`
    /// or `ROLLBACK` has finished too.
    finished: Option<Finished>,
    pub(super) halt: Halt,
}

impl Frame {
    pub(super) fn new(
        state: Arc<State>,
        commands: mpsc::UnboundedReceiver<Command>,
        opened: Opened,
    ) -> Self {
        Self {
            state,
            commands,
            opened: Some(opened),
            committed: None,
            finished: None,
            halt: Halt::default(),
        }
    }

    /// Serve the frame's commands inside its transaction, a savepoint's frame
    /// nested inside while it is open: `Ok` once Python commits, an `Exit`
    /// once it rolls back or the handle is abandoned.
    // [spec:pgorm:req:python.transactions+2]
    pub(super) fn serve<'a>(
        &'a mut self,
        tx: &'a mut DatabaseTransaction<'_>,
    ) -> BoxFuture<'a, Result<(), Exit>> {
        Box::pin(async move {
            self.finished = Some(Finished(self.state.finished.clone()));
            if self
                .opened
                .take()
                .is_some_and(|opened| opened.send(Ok(())).is_err())
            {
                return self.halt.discard().await;
            }
            loop {
                let command = tokio::select! {
                    _ = self.state.abandoned.cancelled() => None,
                    command = self.commands.recv() => command,
                };
                let Some(command) = command else {
                    self.halt.ending.cancel();
                    return Err(Exit::Abandoned);
                };
                match command {
                    Command::Run { work, reply } => {
                        let result = tokio::select! {
                            _ = self.state.abandoned.cancelled() => return self.halt.discard().await,
                            result = work.run(Database::Transaction(&*tx)) => result,
                        };
                        if reply.send(result).is_err() {
                            return self.halt.discard().await;
                        }
                    }
                    // [spec:pgorm:req:python.copy+2]
                    Command::CopyIn {
                        statement,
                        rows,
                        mut messages,
                        opened,
                        reply,
                    } => {
                        let ran = tokio::select! {
                            _ = self.state.abandoned.cancelled() => return self.halt.discard().await,
                            ran = super::copy::run_in(&mut *tx, &statement, rows.as_ref(), &mut messages, opened) => ran,
                        };
                        match ran {
                            super::copy::RanIn::Lost => return self.halt.discard().await,
                            super::copy::RanIn::Undescribed => {}
                            super::copy::RanIn::Ran(result) => {
                                if reply.send(result).is_err() {
                                    return self.halt.discard().await;
                                }
                            }
                        }
                    }
                    Command::CopyOut {
                        statement,
                        rows,
                        pieces,
                        opened,
                    } => {
                        let known = tokio::select! {
                            _ = self.state.abandoned.cancelled() => return self.halt.discard().await,
                            known = super::copy::run_out(&mut *tx, &statement, rows.as_ref(), &pieces, opened) => known,
                        };
                        if !known {
                            return self.halt.discard().await;
                        }
                    }
                    Command::Finish {
                        commit: true,
                        reply,
                    } => {
                        self.committed = Some(reply);
                        return Ok(());
                    }
                    Command::Finish {
                        commit: false,
                        reply,
                    } => return Err(Exit::RolledBack(reply)),
                    Command::Begin {
                        child,
                        receiver,
                        opened,
                        parent_lock,
                    } => {
                        let mut frame = Frame::new(child.clone(), receiver, opened);
                        let halt = frame.halt.clone();
                        let ended = halt
                            .run(tx.transaction(async |savepoint| frame.serve(savepoint).await))
                            .await;
                        let completion = match frame.end(ended) {
                            Ended::Ran(completion) => completion,
                            Ended::Refused(opened, error) => {
                                let secrets = &self.state.connection.pool.secrets;
                                if opened.send(Err(database_error(error, secrets))).is_err() {
                                    return self.halt.discard().await;
                                }
                                continue;
                            }
                        };
                        drop(frame);
                        let healthy = completion.healthy;
                        drop(parent_lock);
                        completion.respond();
                        child.released.cancel();
                        if !healthy {
                            return self.halt.discard().await;
                        }
                    }
                }
            }
        })
    }

    /// What the frame's transaction ended in: `None` is a future [`Halt`]
    /// dropped. A frame whose `COMMIT` or `ROLLBACK` failed has left its
    /// connection in a state nobody knows, so it is not healthy, and the reply
    /// carries the failure.
    pub(super) fn end(&mut self, ended: Option<Result<(), TransactionError<Exit>>>) -> Ended {
        let Some(ended) = ended else {
            return Ended::Ran(Completion::discarded());
        };
        Ended::Ran(match ended {
            Ok(()) => Completion::finished(self.committed.take()),
            Err(TransactionError::Connection(error)) => {
                if let Some(opened) = self.opened.take() {
                    return Ended::Refused(opened, error);
                }
                Completion {
                    healthy: false,
                    reply: self.committed.take(),
                    result: Err(error),
                }
            }
            Err(TransactionError::Transaction(Exit::RolledBack(reply))) => {
                Completion::finished(Some(reply))
            }
            Err(TransactionError::Transaction(Exit::Abandoned)) => Completion::finished(None),
            Err(TransactionError::RollbackFailed { cause, rollback }) => Completion {
                healthy: false,
                reply: match cause {
                    Exit::RolledBack(reply) => Some(reply),
                    Exit::Abandoned => None,
                },
                result: Err(rollback),
            },
            // A failure with no arm here leaves the connection in a state
            // nobody knows, as a failed ROLLBACK does.
            Err(_) => Completion {
                healthy: false,
                reply: self.committed.take(),
                result: Err(Error::Custom(
                    "the transaction failed in a way pgorm-python does not represent".to_owned(),
                )),
            },
        })
    }
}
