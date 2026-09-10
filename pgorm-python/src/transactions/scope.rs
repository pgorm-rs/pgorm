//! A recursive task frame owns each real borrowed transaction/savepoint.

use super::{
    state::{Finished, State},
    work::{Output, Work},
};
use crate::execution::Database;
use futures_util::future::BoxFuture;
use pgorm::{DatabaseTransaction, Error, TransactionTrait};
use std::{sync::Arc, time::Duration};
use tokio::sync::{OwnedMutexGuard, mpsc, oneshot};

type Reply = oneshot::Sender<Result<Output, Error>>;

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
        opened: oneshot::Sender<Result<(), Error>>,
        parent_lock: OwnedMutexGuard<()>,
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
    pub(super) fn respond(self) {
        if let Some(reply) = self.reply {
            let _ = reply.send(self.result);
        }
    }
}

// [spec:pgorm:req:python.transactions]
pub(super) fn serve<'a>(
    mut tx: DatabaseTransaction<'a>,
    state: Arc<State>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) -> BoxFuture<'a, Completion> {
    Box::pin(async move {
        let _finished = Finished(state.finished.clone());
        loop {
            let command = tokio::select! {
                _ = state.abandoned.cancelled() => None,
                command = commands.recv() => command,
            };
            let Some(command) = command else {
                let result = tokio::time::timeout(Duration::from_secs(5), tx.rollback()).await;
                return Completion {
                    healthy: matches!(result, Ok(Ok(()))),
                    reply: None,
                    result: Ok(Output::Finished),
                };
            };
            match command {
                Command::Run { work, reply } => {
                    let result = tokio::select! {
                        _ = state.abandoned.cancelled() => return Completion::discarded(),
                        result = work.run(Database::Transaction(&tx)) => result,
                    };
                    if reply.send(result).is_err() {
                        return Completion::discarded();
                    }
                }
                Command::Finish { commit, reply } => {
                    let result = if commit {
                        tx.commit().await
                    } else {
                        tx.rollback().await
                    };
                    return Completion {
                        healthy: result.is_ok(),
                        reply: Some(reply),
                        result: result.map(|()| Output::Finished),
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
                            if opened.send(Err(error)).is_err() {
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
