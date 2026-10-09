//! The tokio runtime pgorm's asynchronous work runs on.

use std::sync::LazyLock;

use neon::prelude::*;
use tokio::runtime::{Builder, Runtime};

/// One multi-threaded runtime per process, shared by every instance of the
/// addon — the main thread's and each worker thread's — and built when the
/// first instance loads.
///
/// It is never shut down. Its threads park when idle and hold nothing the
/// JavaScript event loop waits on, so they cannot keep a process alive, and
/// the process's exit ends them. Shutting it down when an instance unloads
/// would cancel the queries of the instances still running, and dropping it on
/// a JavaScript thread blocks that thread until every worker stops.
// [spec:pgorm:req:napi.runtime]
// [spec:pgorm:req:napi.exit]
static RUNTIME: LazyLock<std::io::Result<Runtime>> = LazyLock::new(|| {
    Builder::new_multi_thread()
        .enable_all()
        .thread_name("pgorm-napi")
        .build()
});

/// The process's runtime, or a JavaScript `Error` saying why it could not be
/// built.
pub(crate) fn runtime<'cx>(cx: &mut impl Context<'cx>) -> NeonResult<&'static Runtime> {
    match &*RUNTIME {
        Ok(runtime) => Ok(runtime),
        Err(error) => cx.throw_error(format!("pgorm could not start its runtime: {error}")),
    }
}
