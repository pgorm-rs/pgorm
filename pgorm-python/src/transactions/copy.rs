//! COPY inside a transaction: the scope task owning the borrowed transaction
//! drives the COPY, while the native handle holds the transaction's
//! reservation until the COPY ends.

use super::{State, scope::Command, state::Pending};
use crate::{
    copy::{CopyFrom, CopyTo, Message, Piece, drive_in, drive_out, open_in, open_out},
    errors::{LifecycleError, database_error},
};
use pgorm::{DatabaseTransaction, Error};
use pyo3::prelude::*;
use std::sync::Arc;
use tokio::sync::{OwnedMutexGuard, mpsc, oneshot};

/// A transaction reserved by a COPY in progress. Dropped without
/// `complete`, the COPY's outcome is unknown, and the whole transaction is
/// discarded.
pub(crate) struct Lease {
    _busy: OwnedMutexGuard<()>,
    pending: Pending,
}

impl Lease {
    pub(crate) fn complete(mut self) {
        self.pending.complete();
    }
}

impl State {
    /// Reserve the transaction and have its owner start `command`, answering
    /// the reservation once the server has accepted the COPY.
    async fn copy(
        self: &Arc<Self>,
        command: impl FnOnce(oneshot::Sender<Result<(), Error>>) -> Command,
        refusal_closes: bool,
    ) -> PyResult<Lease> {
        self.ensure_open()?;
        let busy = self.busy.clone().try_lock_owned().map_err(|_| {
            LifecycleError::new_err("transaction has an active operation or nested savepoint")
        })?;
        let (opened, open) = oneshot::channel();
        let mut pending = Pending::new(self.root.aborted.clone());
        self.commands
            .send(command(opened))
            .map_err(|_| LifecycleError::new_err("transaction owner has closed"))?;
        let result = open
            .await
            .map_err(|_| LifecycleError::new_err("transaction owner has closed"))?;
        match result {
            Ok(()) => Ok(Lease {
                _busy: busy,
                pending,
            }),
            Err(error) => {
                // A refused COPY FROM has closed the connection under the
                // transaction, so `pending` is left to abort it.
                if !refusal_closes {
                    pending.complete();
                }
                Err(database_error(error, &self.connection.pool.secrets))
            }
        }
    }

    /// Wait until the root transaction's owner has let go of its connection.
    pub(crate) async fn released(&self) {
        self.root.finished.cancelled().await;
    }

    // [spec:pgorm:req:python.copy+2]
    pub(crate) async fn copy_in(
        self: &Arc<Self>,
        statement: CopyFrom,
        messages: mpsc::Receiver<Message>,
    ) -> PyResult<(Lease, oneshot::Receiver<Result<Option<u64>, Error>>)> {
        let (reply, settled) = oneshot::channel();
        let lease = self
            .copy(
                |opened| Command::CopyIn {
                    statement,
                    messages,
                    opened,
                    reply,
                },
                true,
            )
            .await?;
        Ok((lease, settled))
    }

    // [spec:pgorm:req:python.copy+2]
    pub(crate) async fn copy_out(
        self: &Arc<Self>,
        statement: CopyTo,
        pieces: mpsc::Sender<Piece>,
    ) -> PyResult<Lease> {
        self.copy(
            |opened| Command::CopyOut {
                statement,
                pieces,
                opened,
            },
            false,
        )
        .await
    }
}

/// Run a COPY FROM on the owner's transaction: `None` when it never
/// started, refused — which closes the connection — or abandoned.
pub(super) async fn run_in(
    tx: &mut DatabaseTransaction<'_>,
    statement: &CopyFrom,
    messages: &mut mpsc::Receiver<Message>,
    opened: oneshot::Sender<Result<(), Error>>,
) -> Option<Result<Option<u64>, Error>> {
    match open_in(tx, statement).await {
        Err(error) => {
            let _ = opened.send(Err(error));
            None
        }
        Ok(sink) => {
            opened.send(Ok(())).ok()?;
            Some(drive_in(sink, messages).await)
        }
    }
}

/// Run a COPY TO on the owner's transaction: whether the transaction is
/// still in a known state. A refusal aborts it as any failed statement does;
/// a reader gone part way leaves the server sending.
pub(super) async fn run_out(
    tx: &mut DatabaseTransaction<'_>,
    statement: &CopyTo,
    pieces: &mpsc::Sender<Piece>,
    opened: oneshot::Sender<Result<(), Error>>,
) -> bool {
    match open_out(tx, statement).await {
        Err(error) => opened.send(Err(error)).is_ok(),
        Ok(stream) => {
            if opened.send(Ok(())).is_err() {
                return false;
            }
            match drive_out(stream, pieces).await {
                Some(result) => pieces
                    .send(Piece::End(Box::new((result, None))))
                    .await
                    .is_ok(),
                None => false,
            }
        }
    }
}
