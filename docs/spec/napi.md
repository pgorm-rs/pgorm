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

> [spec:pgorm:def:napi.api+1]
> The JavaScript API is an ES module, `pgorm-napi/lib/index.js`, backed by a
> native Node-API addon over pgorm's Rust query construction, execution and
> decoding APIs. The module loads the addon, hands it the error and value
> classes it rejects with and builds values from, exports them, and is what an
> application imports; the addon is not imported directly.

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

> [spec:pgorm:req:napi.errors+1]
> Failures MUST reject with instances of the module's error classes, all
> derived from `PgormError` and `Error`: `DatabaseError` when PostgreSQL
> reports an error, carrying its SQLSTATE as `sqlstate` with its severity and
> optional diagnostic fields (`detail`, `hint`, `schema`, `table`, `column`,
> `constraint`, each a string or `null`); `ConnectionError` when the server
> cannot be reached or the connection breaks; `ConstructionError` for an
> argument that cannot become the value pgorm sends; `DecodeError` for a result
> that cannot become the JavaScript value asked for; `LifecycleError` for a
> pool, connection, transaction or stream used after it closed or while
> another operation holds it; `TimeoutError` when no connection becomes free
> within a pool's acquire budget; `InternalError` when pgorm
> or the binding fails in a way no input should cause, a panic on the runtime
> above all, which MUST reject its promise rather than leave it pending or be
> classified as an expected input rejection. An error from PostgreSQL
> anywhere in a failure's cause chain makes it a `DatabaseError`, so a refusal
> during connection carries its SQLSTATE too. Credentials MUST NOT appear in
> an error, nested causes included. A failure is captured as data on the
> runtime thread and becomes an error object on the JavaScript thread, built
> by a factory the module registers with its instance; an instance with none
> registered rejects with an `Error` of the same name and fields.

## Values

Every kind of value pgorm's `Value` holds crosses into and out of JavaScript
with one declared JavaScript type, and the one PostgreSQL type pgorm has no
variant for, `interval`, crosses as well. Precision is the first rule: a value
reaches JavaScript exactly or not at all.

| Kind | PostgreSQL | JavaScript |
| --- | --- | --- |
| `bool` | `boolean` | `boolean` |
| `i8`, `i16`, `i32`, `u32` | `"char"`, `int2`, `int4`, `oid` | `number` |
| `i64`, `u64` | `int8` | `bigint` |
| `f32`, `f64` | `float4`, `float8` | `number` |
| `text`, `char` | `text`, `varchar`, `bpchar`, `name` | `string` |
| `bytes` | `bytea` | `Uint8Array` |
| `json` | `json`, `jsonb` | `null`, boolean, number, `bigint`, string, array, plain object |
| `decimal` | `numeric` | the module's `Decimal` |
| `uuid` | `uuid` | the module's `Uuid` |
| `date`, `time`, `datetime`, `datetime_utc` | `date`, `time`, `timestamp`, `timestamptz` | `Temporal.PlainDate`, `PlainTime`, `PlainDateTime`, `Instant` |
| `interval` | `interval` | the module's `Interval` |
| `ipnetwork`, `mac_address` | `inet`/`cidr`, `macaddr` | `string`, six-byte `Uint8Array` |
| `vector` | pgvector's `vector` | `Float32Array` |
| a built-in range or multirange | `int4range` … `tstzmultirange` | the module's `Range`, `Multirange` |
| an enum | its type | `string` |
| a created range | its type | `Range`, its kind a `CreatedRange` |

> [spec:pgorm:req:napi.values]
> Every kind of pgorm `Value`, and `interval`, MUST cross into and out of
> JavaScript as the one JavaScript type the table above declares for it, and
> SQL NULL as `null`, both ways. A value MUST reach JavaScript exactly or not
> at all: `int8` decodes as a `bigint`, never a number; `numeric` as a
> `Decimal`, whose text keeps the value's scale, within pgorm's range of a
> 96-bit coefficient and 28 fractional digits; `float4` widened exactly; a
> JSON number exactly, an integer literal past `Number.MAX_SAFE_INTEGER` as a
> `bigint` and a number serde_json would read as another refused. A value
> with no exact JavaScript form — a `numeric` past pgorm's range, `NaN` or
> infinite; a date or timestamp outside the years -9999 to 9999 pgorm holds,
> or infinite; `24:00`; an array of more than one dimension or not starting
> at 1; a type the table does not list — MUST be a `DecodeError`, never a
> rounded value, a string, `null` or a missing column. A domain decodes as
> the type it is built over.
>
> In the other direction a value MUST become the pgorm value it declares or
> be refused with a `ConstructionError`: an integer outside its kind's range,
> a number past 2^53 - 1 declared as an integer, an `f32` that narrowing
> would change, a string holding a lone surrogate, a `Decimal` outside pgorm's
> range or in exponent notation, a `Decimal` made from a number, which may
> already be rounded, and statement text holding NUL. A `Decimal` MUST NOT
> become a number implicitly. Parameters are bound, never interpolated,
> through pgorm's `ValueHolder`, which writes each against the type
> PostgreSQL inferred for its placeholder and refuses a value that type
> cannot receive; its coercions — an integer to any numeric type, a float
> to `float4` rounding as PostgreSQL's own cast does — are pgorm's, and a
> refusal names the parameter and the codec's reason.

> [spec:pgorm:req:napi.temporal]
> Dates and times MUST be Temporal's: `date` a `Temporal.PlainDate`, `time` a
> `Temporal.PlainTime`, `timestamp` a `Temporal.PlainDateTime` and
> `timestamptz` a `Temporal.Instant`, a `PlainDate` or `PlainDateTime` in
> another calendar binding as the same day. A `timestamptz` is an instant:
> PostgreSQL keeps no zone with it and sends it in binary as microseconds from
> 2000-01-01 UTC, so the instant read MUST NOT depend on the session's
> `TimeZone`, which governs only its text rendering and the reading of text
> without an offset. A `ZonedDateTime` would invent a zone on the way out and
> drop one on the way in, so it is refused as a parameter, as a `Date` is.
> A `PlainDateTime` MUST NOT bind to `timestamptz`, nor an `Instant` to
> `timestamp`, in an array or a range bound included: pgorm writes both as
> microseconds and would read either as UTC, where PostgreSQL's own cast
> between the two reads the wall clock in the session's zone.
>
> PostgreSQL keeps microseconds and Temporal nanoseconds. Reading MUST be
> exact, and a value written with a nonzero digit below the microsecond MUST
> be a `ConstructionError`, never truncated or rounded by the binding.
>
> `interval` MUST keep PostgreSQL's three independently signed fields —
> months, days and microseconds — as the module's `Interval`, because
> `Temporal.Duration` requires every field to share one sign and so cannot
> hold `1 mon -2 days`. `Interval.from` takes a `Duration`, years and months
> becoming months, weeks and days days and the rest microseconds, and a
> `Duration` parameter binds as one; `toDuration` gives one back where the
> signs agree and throws a `RangeError` where they do not. `timetz` MUST be a
> `DecodeError`: no Temporal type holds a time of day at a fixed offset.

> [spec:pgorm:req:napi.value-tags]
> The module's `Value` MUST declare what plain JavaScript cannot:
> `new Value(data, kind)` takes a kind named as pgorm-python names it, a
> `TypeName` for an enum, or a `CreatedRange` or `CreatedMultirange`;
> `Value.null(kind)` is SQL NULL of a kind; `Value.json(data)` is a JSON
> document, so JSON's `null` stays distinct from SQL NULL; and
> `Value.array(kind, items)` is an array that names its element kind when it
> is empty or NULL. Construction does no I/O and MUST convert at once,
> throwing a `ConstructionError` for data the kind cannot hold exactly. A
> `Value` is immutable and can be bound any number of times, and the plain
> value it hands out is independently owned.
>
> A result read with `{ tagged: true }` MUST give each column as a `Value`
> carrying its kind: an integer's width, an enum's schema-qualified type, an
> array's element kind, and SQL NULL apart from JSON's null.
>
> A created range's kind MUST carry the type's name, schema-qualifiable, and
> its subtype, one of the twelve pgorm's `RangeSubtype` admits, which
> converts each bound. Its value is the text form a `DeriveCreatedRange`
> newtype converts into, bound as text for the statement to cast,
> `CAST($1::text AS name)`, as `Expr::as_range` writes it. A column of one
> MUST decode, through its subtype's binary codec, to a `Range` whose kind is
> named as the column's type is. Arrays of a created range are refused both
> ways.

> [spec:pgorm:req:napi.inference]
> A plain JavaScript parameter MUST be bound as the one kind its type leaves:
> `null` as SQL NULL of no declared kind; a boolean as `bool`; a number as
> `i64` when it is a safe integer other than negative zero, so that it binds
> to an integer column exactly, and as `f64` otherwise; a `bigint` as `i64`; a
> string as `text`; a `Uint8Array` as `bytes`; a `Decimal`, `Uuid`,
> `Interval`, `Temporal.Duration` or Temporal date, time, date-time or
> instant as its kind; a plain object as `json`; and an array as an array of
> the one kind its non-null items infer as, numbers being `i64` items only
> when every one is a safe integer. `undefined`, an empty or all-null array,
> a range, an array of mixed kinds or nested arrays, and any other object
> MUST be refused with a `ConstructionError` that names the explicit form,
> never guessed at or turned into a string.

> [spec:pgorm:req:napi.rows]
> A result's rows MUST be decoded on the runtime thread and reach JavaScript
> as plain objects, one per row, keyed by column name in column order, each
> key an own data property, so that a column named `__proto__` is a property
> rather than the row's prototype. Two columns of one name MUST be a
> `DecodeError`, not one key for both. A statement that returns no rows
> resolves with an empty array.

## Connections

A `Pool` holds the connections, a `Connection` is one of them checked out, a
`Transaction` is a transaction or savepoint on one, and a `RowStream` pulls a
statement's rows over one. Each is a JavaScript object over a boxed native
handle, and each is released by being closed.

> [spec:pgorm:req:napi.connections]
> A `Pool` MUST be built from a connection string and options — TLS mode, CA,
> size, connect and acquire timeouts, statement-cache size and recycling — by
> a constructor that sends nothing and throws a `ConstructionError` for an
> unusable string or option; `connect` resolves with a pool whose server
> answered. TLS MUST default to `verify-full`, verifying the server's
> certificate and host name against the CA given or the platform's trust
> store, with no silent fallback to plaintext; only `sslmode=disable` or
> `tls: "disable"` connects in plaintext. A pool lends a `Connection` with
> `acquire`, and runs a statement of its own on a connection lent to it alone.
>
> Each handle — pool, connection, transaction, stream — MUST be closable
> explicitly, by `close()` or `await using`, and what it holds MUST be
> released by that close, never left to garbage collection or Neon's drop
> queue: closing a connection returns it to its pool at once, and closing a
> pool refuses new work, cancels what runs and resolves once every connection
> is released. A handle collected unclosed releases only what is idle — an
> idle transaction rolled back, an idle connection returned to its pool —
> never cutting short work still running on it, and a transaction keeps the
> connection or transaction it borrows reachable. A connection runs one
> operation at a time: a second, or any while a transaction or stream holds
> it, MUST be refused with a `LifecycleError` rather than queued or raced, as
> is any use after close. Acquiring past `acquireTimeout` MUST reject with a
> `TimeoutError`. Connections live on the runtime and hold nothing a
> JavaScript event loop waits on, so a process that ends holding open
> handles still exits.

> [spec:pgorm:req:napi.results]
> A pool, a connection and a transaction MUST run bound SQL through pgorm's
> cached `ConnectionTrait` path with the same terminals: `execute` resolves
> with the affected-row count as an exact number; `query` with every row;
> `one` with exactly one row; and `optional` with at most one, or `null`. Any
> other count is a `DecodeError`, decided after the query and never in place
> of a database or decode failure. `{ tagged: true }` gives each column as a
> `Value`, as `napi.value-tags` describes.

> [spec:pgorm:req:napi.transactions]
> Transactions MUST run through pgorm's `DatabaseTransaction` and its
> savepoints, a task on the runtime owning each borrowed scope and JavaScript
> sending it owned commands. `begin` opens one explicitly, its `commit` or
> `rollback` usable once; `transaction(fn)` on a pool, a connection or a
> transaction commits when `fn` resolves and rolls back when it throws,
> rejecting with `fn`'s own error whatever the rollback does, and a commit
> that fails ends the transaction and discards its connection. On a
> transaction, `begin` and `transaction` open a savepoint. pgorm's rule that a
> transaction borrows its parent exclusively MUST hold: while a transaction is
> open its connection refuses other work; while a savepoint is open its
> transaction refuses statements, commits, rollbacks and other savepoints;
> and a statement started while another runs on the same transaction is
> refused — each with a `LifecycleError`, never queued or raced. A callback
> that settles with a savepoint still open fails, and commits nothing.
> `mode` and `isolation` select pgorm's `TransactionMode`, the combinations
> PostgreSQL acts on; a savepoint takes neither. Nothing is retried.

> [spec:pgorm:req:napi.cancellation]
> Acquiring a connection, running a statement, opening a transaction or a
> savepoint, and pulling a stream's rows MUST each take an `AbortSignal`. An
> aborted operation rejects with the signal's reason. Its outcome is unknown — a write may or may not
> have happened — so the connection it ran on MUST be discarded rather than
> returned to its pool, and a transaction it ran in ends with nothing
> committed; an operation whose signal fired before it started sends nothing.
> Closing a pool or a connection MUST likewise interrupt what runs on it,
> idle transactions and streams included, without waiting for them.

> [spec:pgorm:req:napi.streams]
> `stream` on a pool or a connection MUST return an async iterator over a
> bound statement's rows through pgorm's `query_raw`, pulling one row per
> `next()`, so that the driver's bounded buffer and the socket hold the server
> back while nothing pulls, and refusing a second pull while one is pending.
> The stream holds its connection until its last row, which frees it, or
> until `return()` — a `for await` loop leaving early — or `close()`, which
> discard it if rows remained; a pool's stream returns or discards a
> connection of its own. A connection's stream reserves that connection, which
> refuses other work meanwhile. A transaction offers no stream, as
> pgorm-python's does not.

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

> [spec:pgorm:req:napi.runtimes+1]
> The binding targets runtimes that carry `Temporal` as a global, which its
> date and time values are: Node.js 26 or later, and Deno 2.9.5 or later, the
> oldest release tested. Loading the module in a runtime without `Temporal`
> MUST throw an error naming the requirement rather than fail later on a
> value. Node.js 26 strips TypeScript types without a flag, so the suite and
> its fixtures run under plain `node`.
>
> One test suite MUST run unchanged under `node --test` and under `deno test`,
> written against `node:test` and `node:assert`, which both runners provide,
> with every runtime difference — how a fixture process is started, and the
> Deno permissions it is granted — kept in one support module. The exit
> behaviour `napi.exit` requires MUST be tested in fresh processes of
> the runtime under test, each held to exiting by itself before a deadline. CI
> MUST run the suite in both runtimes, Node.js 26 and Deno, against PostgreSQL
> 18 on Linux and macOS.
