# JavaScript API (Node-API)

This chapter specifies pgorm's interface to JavaScript, implemented by the
optional `pgorm-napi` companion crate: a native addon built with the Neon
framework against Node-API, which both Node.js and Deno load. The WBS root is
`napi`. It mirrors [python.md](python.md)'s structure in JavaScript
conventions — camelCase, a Promise for anything asynchronous, typed errors —
and these rules grow with it. What exists today is the foundation the rest of
the binding is built on: the loading model, the runtime that runs pgorm's
asynchronous work, the contract by which that work settles a Promise, and a
process's clean exit in both runtimes.

## Package boundary

> [spec:pgorm:def:napi.api]
> The JavaScript API is an ES module, `pgorm-napi/lib/index.js`, backed by a
> native Node-API addon over pgorm's Rust query construction, execution and
> decoding APIs. The module loads the addon, defines the error classes, and is
> what an application imports; the addon is not imported directly.

> [spec:pgorm:req:napi.optional]
> JavaScript support MUST live in an opt-in companion crate. Ordinary pgorm
> builds, default workspace builds and Rust library users MUST NOT require
> Node.js, Deno, Neon or a Node-API linker configuration. The companion crate
> is its own Cargo workspace, and the rule `python.optional` states
> for pinned dependency revisions holds for it unchanged: a revision pgorm
> pins MUST reach the addon through pgorm's own dependency declarations, never
> a `patch` table, and the crate's committed lockfile MUST resolve it to the
> pinned revision.

## Targeting and loading

> [spec:pgorm:req:napi.loading]
> The addon MUST target Node-API version 6 — the lowest that carries
> per-instance data, which the shared channel, instance-local values and the
> dropped-handle queue rest on — and MUST NOT link against a particular
> runtime's executable: its Node-API symbols are resolved from the host process
> when it loads, so the one build serves Node.js and Deno.
>
> The built library is copied to `lib/pgorm_napi.node`, beside the module that
> loads it, and kept out of version control. `lib/index.js` loads it through a
> CommonJS `require` made with `node:module`'s `createRequire`, the one loader
> both runtimes give a `.node` file. Node.js needs no flag to load it. Deno
> MUST be granted `--allow-ffi`, to open a native library, and `--allow-read`,
> to resolve its path; `--allow-net` is not needed and does not confine the
> addon. Native code runs outside Deno's permission checks, so granting
> `--allow-ffi` to a program that loads the addon trusts it with everything
> the process can do, the network included.

## Runtime and threading

> [spec:pgorm:req:napi.runtime]
> pgorm's asynchronous work MUST run on one multi-threaded Tokio runtime per
> process, built when the first instance of the addon loads and shared by
> every instance after it — the main thread's and each worker thread's. A
> runtime MUST NOT be created per call or per query. A JavaScript thread MUST
> NOT block on the runtime. The runtime is never shut down: its idle threads
> hold nothing a JavaScript event loop waits on, the process's exit ends them,
> and shutting it down with an instance would cancel the work of the instances
> still running.

> [spec:pgorm:req:napi.promises]
> Every operation that does I/O MUST return a Promise and MUST NOT throw
> synchronously: an argument that cannot be used rejects the Promise like any
> other failure. The work runs on the runtime, and its outcome MUST settle the
> Promise on the JavaScript thread of the instance that started it, through
> that instance's shared channel. Each operation in flight MUST hold a
> reference that keeps its event loop alive, and MUST release it once its
> settlement is queued, so that concurrent operations each settle with their
> own outcome and a loop with nothing in flight is free to finish. An
> operation MUST NOT leave its promise to Neon's drop queue, which rejects a
> promise whose `Deferred` was dropped unsettled: the queue holds no event
> loop open, and Node.js finishes a loop with its deliveries still pending
> where Deno delivers them first, so a promise left to it may never settle.

> [spec:pgorm:req:napi.errors]
> Failures MUST reject with instances of the module's error classes, all
> derived from `PgormError` and `Error`: `DatabaseError` when PostgreSQL
> reports an error, carrying its SQLSTATE as `sqlstate` with its severity and
> optional diagnostic fields (`detail`, `hint`, `schema`, `table`, `column`,
> `constraint`, each a string or `null`); `ConnectionError` when the server
> cannot be reached or the connection breaks; `ConstructionError` for an
> argument that cannot become the value pgorm sends; `DecodeError` for a result
> that cannot become the JavaScript value asked for; `InternalError` when pgorm
> or the binding fails in a way no input should cause, a panic on the runtime
> above all, which MUST reject its promise rather than leave it pending or be
> classified as an expected input rejection. An error from PostgreSQL
> anywhere in a failure's cause chain makes it a `DatabaseError`, so a refusal
> during connection carries its SQLSTATE too. Credentials MUST NOT appear in
> an error, nested causes included. A failure is captured as data on the
> runtime thread and becomes an error object on the JavaScript thread, built
> by a factory the module registers with its instance; an instance with none
> registered rejects with an `Error` of the same name and fields.

## Clean exit

> [spec:pgorm:req:napi.exit]
> Loading the addon MUST NOT keep a process alive, nor MUST anything it leaves
> behind once its operations have settled: a process that has finished its
> work exits on its own, in Node.js and in Deno, with its ordinary status and
> nothing written to stderr. An operation still in flight when a script's last
> statement runs MUST keep the process up until it settles. A process that
> exits explicitly while operations are running MUST neither wait for them nor
> crash, and an outcome that arrives after its instance has been torn down — the
> process exiting, its worker terminated — MUST be dropped without a panic or a
> write to a closed environment. The shared channel and the drop queue MUST
> NOT hold a process open once nothing is in flight, whether or not the drop
> queue has deliveries pending. An unhandled rejection MUST end the
> process the way the runtime ends one for any other unhandled rejection.

## Types and tests

> [spec:pgorm:req:napi.typing]
> The module MUST ship TypeScript declarations for every export, error classes
> and their fields included, beside it as `lib/index.d.ts` and named by the
> module's `@ts-self-types` directive so Deno finds them as Node.js tooling
> does. `deno check` MUST type-check the declarations, the module and the test
> suite, which uses the public API through them.

> [spec:pgorm:req:napi.runtimes]
> One test suite MUST run unchanged under `node --test` and under `deno test`,
> written against `node:test` and `node:assert`, which both runners provide,
> with every runtime difference — how a fixture process is started, and the
> Deno permissions it is granted — kept in one support module. The exit
> behaviour `napi.exit` requires MUST be tested in fresh processes of
> the runtime under test, each held to exiting by itself before a deadline. CI
> MUST run the suite in both runtimes against PostgreSQL 18 on Linux and
> macOS.
