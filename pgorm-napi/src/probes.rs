//! Exports that exist only in debug builds, for the suite to drive the two
//! paths no query reaches on its own.
//!
//! `probePanic` panics inside work on the runtime, which has to reject its
//! promise with an `InternalError`. `probeDropQueue` drops an unsettled
//! promise's `Deferred` on a runtime thread, which Neon hands to the
//! instance's drop queue — a threadsafe function the event loop does not wait
//! on — to reject on the JavaScript thread. Every handle a later binding keeps
//! across threads is released through that queue, so the suite proves it
//! delivers in both runtimes without holding a finished process open.

use neon::prelude::*;

use crate::{errors::Failure, runtime::runtime, settle};

pub(crate) fn export(cx: &mut ModuleContext) -> NeonResult<()> {
    cx.export_function("probePanic", probe_panic)?;
    cx.export_function("probeDropQueue", probe_drop_queue)?;
    Ok(())
}

async fn panics() -> Result<(), Failure> {
    panic!("the probe's deliberate panic")
}

fn probe_panic(mut cx: FunctionContext) -> JsResult<JsPromise> {
    settle::promise(&mut cx, panics(), |cx, ()| Ok(cx.undefined().upcast()))
}

fn probe_drop_queue(mut cx: FunctionContext) -> JsResult<JsPromise> {
    let runtime = runtime(&mut cx)?;
    let (deferred, promise) = cx.promise();
    runtime.spawn(async move { drop(deferred) });
    Ok(promise)
}
