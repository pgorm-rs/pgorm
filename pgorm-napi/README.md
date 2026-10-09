# pgorm-napi

pgorm from Node.js and Deno: a Node-API addon built with
[Neon](https://neon-bindings.com), and the ES module that loads it. It is the
JavaScript counterpart of `pgorm-python`, and like it lives in its own Cargo
workspace so that ordinary pgorm builds never meet it. The rules it follows
are [docs/spec/napi.md](../docs/spec/napi.md).

The addon loads in both runtimes, runs pgorm's asynchronous work on a tokio
runtime of its own, settles a Promise with the outcome, and lets the process
exit when its work is done. Every value pgorm holds crosses into and out of
JavaScript with a declared type ([Values](#values)). It needs a runtime with
`Temporal` as a global: Node.js 26 or later, or Deno 2.9.5 or later.

## Building

```sh
node pgorm-napi/scripts/build.mjs            # debug build
node pgorm-napi/scripts/build.mjs --release  # optimised build
```

The script runs Cargo (into the repository's `target/` unless
`CARGO_TARGET_DIR` says otherwise) and copies the library it produces to
`pgorm-napi/lib/pgorm_napi.node`, which git ignores. The addon targets
Node-API 6 and resolves its Node-API symbols from whichever runtime loads it,
so one build serves both.

## Loading

Import the module, never the `.node` file:

```js
import { DatabaseError, query } from "./pgorm-napi/lib/index.js";

try {
  const [row] = await query("postgres://postgres@localhost/postgres", "SELECT $1::int8 + 1 AS n", [41]);
  console.log(row.n); // 42n
} catch (error) {
  if (error instanceof DatabaseError) console.error(error.sqlstate, error.message);
  else throw error;
}
```

- **Node.js** loads it as is: `node app.js`.
- **Deno** needs `--allow-ffi`, to open a native library, and `--allow-read`,
  to resolve its path: `deno run --allow-ffi --allow-read app.ts`. Native code
  runs outside Deno's permission checks, so `--allow-net` is neither needed
  nor a limit on what the addon reaches; granting `--allow-ffi` to a program
  that loads it trusts it with everything the process can do.

`lib/index.d.ts` declares every export, and Deno finds it through the module's
`@ts-self-types` directive.

## Values

A parameter is bound, never interpolated, and each value has one JavaScript
type, both ways. Precision comes first: a value reaches JavaScript exactly or
not at all, and one that cannot is a `DecodeError` or `ConstructionError`,
never a rounded number, a truncated time or a string.

| PostgreSQL | JavaScript |
| --- | --- |
| `boolean` | `boolean` |
| `int2`, `int4`, `oid`, `"char"` | `number` |
| `int8` | `bigint`, never a number |
| `float4`, `float8` | `number` |
| `text`, `varchar`, `bpchar`, `name`, an enum | `string` |
| `bytea` | `Uint8Array` |
| `numeric` | `Decimal`, its text keeping its scale (`"19.9900"`) |
| `uuid` | `Uuid` |
| `json`, `jsonb` | JSON values; an integer past 2^53 − 1 as a `bigint` |
| `date`, `time`, `timestamp` | `Temporal.PlainDate`, `PlainTime`, `PlainDateTime` |
| `timestamptz` | `Temporal.Instant` |
| `interval` | `Interval`: months, days and microseconds, each signed |
| `inet`, `cidr`; `macaddr` | `string`; six-byte `Uint8Array` |
| arrays | arrays, `null` for a NULL item |
| ranges and multiranges | `Range`, `Multirange` |
| NULL | `null` |

Why these:

- **`numeric`** is a `Decimal`, not a number, because a float cannot hold
  `0.1` or `19.9900`. It is exact within pgorm's range (a 96-bit coefficient,
  28 fractional digits), is made from a string or a `bigint`, and refuses to
  become a number implicitly.
- **Dates and times** are Temporal's, not `Date`: a `Date` holds milliseconds
  and one meaning, an instant, where PostgreSQL has microseconds and four
  types. A `timestamptz` is an instant — PostgreSQL stores no zone with it, and
  the binary protocol sends UTC microseconds — so it reads as a
  `Temporal.Instant`, the same whatever the session's `TimeZone`, which only
  shapes its text. A `ZonedDateTime` or a `Date` parameter is refused rather
  than have its zone dropped or its meaning guessed, and a `PlainDateTime`
  binds only to `timestamp` and an `Instant` only to `timestamptz`. A value
  with a digit below the microsecond is refused; round it first, e.g.
  `instant.round({ smallestUnit: "microsecond" })`.
- **`interval`** is the module's `Interval` rather than a `Temporal.Duration`,
  whose fields must share one sign and so cannot hold `1 mon -2 days`.
  `Interval.from(duration)` and `interval.toDuration()` convert where they can.
- **`timetz`** is refused: no Temporal type is a time of day at a fixed offset.
  Select it cast to `time` or `text`.

A plain parameter's kind is inferred where its JavaScript type leaves one
answer: a safe-integer `number` binds as an integer, any other number as a
`float8`, a `bigint` as `int8`, a plain object as JSON, an array as an array of
its items' one kind. For anything else, declare the kind with `Value`, using
pgorm-python's kind names:

```js
import { Range, TypeName, Value } from "./pgorm-napi/lib/index.js";

new Value(5, "i16");                                  // an int2
Value.null("uuid");                                   // a typed NULL
Value.json(null);                                     // JSON's null, not SQL NULL
Value.array("i64", []);                               // an empty int8[]
new Value(new Range(1, 5), "int4range");              // a range's kind is never inferred
new Value("calm", new TypeName("mood", { schema: "app" }));
```

`query(dsn, sql, params, { tagged: true })` gives each column as a `Value`
carrying its kind, which tells an `int2` from an `int8` and SQL NULL from JSON's
`null`. A range type a schema created is a `CreatedRange` kind; its value
travels as text for the statement to cast, `CAST($1::text AS app.floatrange)`,
as pgorm's own `DeriveCreatedRange` does.

## Errors

Every failure rejects with a subclass of `PgormError`: `DatabaseError` when
PostgreSQL refuses a statement, with its `sqlstate`, `severity` and optional
diagnostic fields; `ConnectionError` when the server cannot be reached;
`ConstructionError` for an argument pgorm cannot send; `DecodeError` for a
result that does not decode. Credentials never appear in a message.

## Tests

One suite, written against `node:test` and `node:assert`, runs unchanged in
both runtimes against a live server. It reads `PGORM_TEST_DSN`, or else the
server `DATABASE_URL` names (from the environment or the checkout's
`.env.local`/`.env`, as the Rust suite does) and its `postgres` database.

```sh
cd pgorm-napi
node --test "tests/*.test.ts"
deno test --allow-ffi --allow-read --allow-env --allow-run tests/
deno check
```

`tests/exit.test.ts` runs each of `tests/fixtures/` in a fresh process of the
runtime under test, which is why Deno's suite needs `--allow-run`, and holds it
to exiting by itself. `tests/runtime.test.ts` drives a panic on the runtime and
Neon's drop queue through two exports only debug builds carry, skipped against
a release build. `tests/values.test.ts` makes a database of its own, named for
the runtime and process, and drops it when it ends. `deno.json` keeps Deno on its global npm cache, so type
checking `node:` imports needs no `node_modules`.
