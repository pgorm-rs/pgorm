//! The data flow between a native COPY handle and the task that owns the
//! connection or transaction the sink or stream borrows.

use futures_util::{SinkExt, StreamExt};
use pgorm::{Bytes, CopyInSink, CopyOutStream, CopyTrait, Error};
use tokio::sync::mpsc;

use super::{CopyFrom, CopyTo};
use crate::runtime::Operation;

/// Chunks queued between Python and the owning task, in either direction.
/// The bound is what holds a fast source, or an unread server, back.
pub(crate) const QUEUED: usize = 4;

/// What a COPY FROM handle sends its owner. Every sender gone without a
/// `Finish` aborts the COPY.
#[derive(Debug)]
pub(crate) enum Message {
    Data(Bytes),
    Finish,
}

/// What a COPY TO owner sends its handle. A Connection's `End` carries its
/// lease, so the connection stays reserved until the reader reaches the end,
/// and is discarded if the reader closes first.
pub(crate) enum Piece {
    Chunk(Bytes),
    End(Box<(Result<(), Error>, Option<Operation>)>),
}

pub(crate) async fn open_in<'a, C>(
    db: &'a mut C,
    statement: &CopyFrom,
) -> Result<CopyInSink<'a>, Error>
where
    C: CopyTrait + Send,
{
    match statement {
        CopyFrom::Text(statement) => db.copy_in(statement).await,
        CopyFrom::Csv(statement) => db.copy_in(statement).await,
        CopyFrom::Binary(statement) => db.copy_in(statement).await,
    }
}

pub(crate) async fn open_out<'a, C>(
    db: &'a mut C,
    statement: &CopyTo,
) -> Result<CopyOutStream<'a>, Error>
where
    C: CopyTrait + Send,
{
    match statement {
        CopyTo::Text(statement) => db.copy_out(statement).await,
        CopyTo::Csv(statement) => db.copy_out(statement).await,
        CopyTo::Binary(statement) => db.copy_out(statement).await,
    }
}

/// Write each chunk into `sink` until `Finish` completes the COPY with its
/// row count. Every sender gone first answers `None`, having dropped the
/// sink, which aborts the COPY.
// [spec:pgorm:req:python.copy+2]
pub(crate) async fn drive_in(
    mut sink: CopyInSink<'_>,
    messages: &mut mpsc::Receiver<Message>,
) -> Result<Option<u64>, Error> {
    while let Some(message) = messages.recv().await {
        match message {
            Message::Data(chunk) => {
                sink.feed(chunk).await?;
                // Flushing only when nothing else is queued lets a burst of
                // chunks share the sink's buffering.
                if messages.is_empty() {
                    sink.flush().await?;
                }
            }
            Message::Finish => return sink.finish().await.map(Some),
        }
    }
    Ok(None)
}

/// Forward each chunk of `stream` until it ends, answering how; `None` once
/// nobody reads, with the stream dropped part way.
// [spec:pgorm:req:python.copy+2]
pub(crate) async fn drive_out(
    mut stream: CopyOutStream<'_>,
    pieces: &mpsc::Sender<Piece>,
) -> Option<Result<(), Error>> {
    loop {
        // Reserving before reading leaves an unread stream unpolled, so the
        // server is held back rather than buffered.
        let permit = pieces.reserve().await.ok()?;
        let next = tokio::select! {
            _ = pieces.closed() => return None,
            next = stream.next() => next,
        };
        match next {
            Some(Ok(chunk)) => permit.send(Piece::Chunk(chunk)),
            Some(Err(error)) => return Some(Err(error)),
            None => return Some(Ok(())),
        }
    }
}
