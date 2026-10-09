# pgorm-napi

pgorm from Node.js and Deno: a Node-API addon built with
[Neon](https://neon-bindings.com), and the ES module that loads it. It is the
JavaScript counterpart of `pgorm-python`, and like it lives in its own Cargo
workspace so that ordinary pgorm builds never meet it. The rules it follows
are [docs/spec/napi.md](../docs/spec/napi.md).

This is the binding's foundation: the addon loads in both runtimes, runs
pgorm's asynchronous work on a tokio runtime of its own, settles a Promise
with the outcome, and lets the process exit when its work is done. The one
operation it exports today, `queryInt`, exists to prove that path end to end.

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
import { DatabaseError, queryInt } from "./pgorm-napi/lib/index.js";

try {
  console.log(await queryInt("postgres://postgres@localhost/postgres", "SELECT $1::int + 1", [41]));
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
node --test --experimental-strip-types --disable-warning=ExperimentalWarning "tests/*.test.ts"
deno test --allow-ffi --allow-read --allow-env --allow-run tests/
deno check
```

`tests/exit.test.ts` runs each of `tests/fixtures/` in a fresh process of the
runtime under test, which is why Deno's suite needs `--allow-run`, and holds it
to exiting by itself. `tests/runtime.test.ts` drives a panic on the runtime and
Neon's drop queue through two exports only debug builds carry, and is skipped
against a release build. `deno.json` keeps Deno on its global npm cache, so type
checking `node:` imports needs no `node_modules`.
