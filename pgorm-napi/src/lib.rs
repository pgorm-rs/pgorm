//! pgorm from Node.js and Deno, through Node-API.
//!
//! The addon is a Neon cdylib that both runtimes load as a `.node` file;
//! `lib/index.js` loads it, defines the error and value classes, and is what
//! JavaScript imports. pgorm's asynchronous work runs on one tokio runtime per
//! process ([`runtime`]), and each result crosses back to JavaScript by
//! settling a promise on the instance's JavaScript thread.

mod codec;
mod connect;
mod decode;
mod errors;
mod params;
#[cfg(debug_assertions)]
mod probes;
mod rows;
mod runtime;
mod settle;
mod values;

use neon::prelude::*;

// [spec:pgorm:def:napi.api+1]
// [spec:pgorm:req:napi.optional]
// [spec:pgorm:req:napi.loading]
#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    runtime::runtime(&mut cx)?;
    cx.export_function("setErrorFactory", errors::set_error_factory)?;
    cx.export_function("setCodec", codec::set_codec)?;
    connect::export(&mut cx)?;
    values::export(&mut cx)?;
    #[cfg(debug_assertions)]
    probes::export(&mut cx)?;
    let version = cx.string(env!("CARGO_PKG_VERSION"));
    cx.export_value("version", version)?;
    Ok(())
}
