# pgorm-napi

pgorm from Node.js and Deno: a Node-API addon built with
[Neon](https://neon-bindings.com), and the ES module that loads it. It is the
JavaScript counterpart of `pgorm-python`, and like it lives in its own Cargo
workspace so that ordinary pgorm builds never meet it. The rules it follows
are [docs/spec/napi.md](../docs/spec/napi.md).

The addon loads in both runtimes, runs pgorm's asynchronous work on a tokio
runtime of its own, settles a Promise with the outcome, and lets the process
exit when its work is done. JavaScript connects through pools, runs bound SQL,
scopes transactions and savepoints and streams rows
([Connections](#connections)), builds pgorm-query's statements
([Statements](#statements)), DDL ([Schema](#schema)) and pipelines
([Pipelines](#pipelines)), and every value pgorm holds crosses into and out of
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
import { DatabaseError, Pool } from "./pgorm-napi/lib/index.js";

await using pool = new Pool("postgres://postgres@localhost/postgres?sslmode=disable");
try {
  const row = await pool.one("SELECT $1::int8 + 1 AS n", [41]);
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

## Connections

```js
import { connect } from "./pgorm-napi/lib/index.js";

await using pool = await connect(process.env.DATABASE_URL, { maxSize: 10 });

await pool.execute("UPDATE accounts SET visits = visits + 1 WHERE id = $1", [7]);  // affected rows
const rows = await pool.query("SELECT id, name FROM accounts WHERE id = ANY($1)", [[1, 2, 3]]);
const row = await pool.one("SELECT now() AS at");               // exactly one row
const maybe = await pool.optional("SELECT 1 WHERE false");       // null

await pool.transaction(async (tx) => {                           // commits, or rolls back on throw
  await tx.execute("INSERT INTO audit VALUES ($1)", ["visit"]);
  await tx.transaction(async (savepoint) => { /* rolled back alone if it throws */ });
});

for await (const account of pool.stream("SELECT * FROM accounts")) {
  if (account.id === 100) break;                                // releases the connection
}

await pool.execute("SELECT pg_sleep(10)", [], { signal: AbortSignal.timeout(1000) });  // rejects after 1 s
```

- **`Pool`** builds from a connection string and options (`tls`, `ca`,
  `maxSize`, `connectTimeout`, `acquireTimeout`, `statementCacheSize`,
  `recycle`; durations in milliseconds) without sending anything; `connect`
  also pings. TLS is `verify-full` — certificate and host name checked against
  `ca` or the platform's trust store — unless the string says
  `sslmode=disable` or `tls` is `"disable"`, and never falls back to plaintext.
  A statement run on a pool takes a connection for itself alone.
- **`Connection`** (from `pool.acquire()`) is the caller's until closed, and
  runs one operation at a time; another meanwhile is a `LifecycleError`, not a
  queue.
- **`Transaction`** comes from `begin()` (then `commit()` or `rollback()`) or
  `transaction(fn)`, which commits when `fn` resolves and rolls back when it
  throws, rejecting with `fn`'s own error. On a transaction, `begin` and
  `transaction` open savepoints. A transaction borrows its parent exclusively,
  as in Rust: while a savepoint is open its transaction refuses statements,
  commits and other savepoints, and while a transaction is open its connection
  refuses other work — refused with a `LifecycleError`, never raced. `mode`
  (`"readWrite"`, `"readOnly"`, `"deferrable"`) and `isolation` choose how it
  opens.
- **`execute`, `query`, `one`, `optional`** are on all three; `one` and
  `optional` reject a row count they do not admit with a `DecodeError`.
  `{ tagged: true }` gives `Value`s.
- **`stream`** on a pool or connection is an async iterator pulling one row
  per `next()`, so an unread result holds the server back. Its connection is
  freed at the last row; leaving the loop early, `return()` or `close()`
  discards it, because rows were left unread on it.
- **`signal`**: acquiring, running a statement, opening a transaction or
  savepoint and pulling a stream's rows each take an `AbortSignal`. Aborting
  rejects with the signal's reason and discards the connection the operation
  ran on — its outcome is unknown — ending any transaction on it.
- **Releasing**: every handle has `close()` and works with `await using`.
  Closing a connection returns it to its pool at once; closing a pool cancels
  what runs and waits until every connection is released. Nothing waits on
  garbage collection: a handle collected unclosed releases only what is idle,
  rolling back an idle transaction and returning an idle connection, and never
  cuts short work still running.

`TimeoutError` is a pool's `acquireTimeout` running out; an `AbortSignal`
timeout rejects with the signal's own `TimeoutError` `DOMException`.

## Statements

pgorm-query's statements and expressions are built from JavaScript as
pgorm-python builds them, and run through the same terminals as SQL text:

```js
import { Condition, call, col, select, Table, With } from "./pgorm-napi/lib/index.js";

const account = new Table("account", { schema: "app", alias: "a" });
const event = new Table("event", { alias: "e" });
const clicks = call("count", event.col("id"));

const query = select(account.col("name"), clicks.as("clicks"))
  .from(account)
  .join(event, account.col("id").eq(event.col("account_id")), { kind: "left" })
  .where(Condition.any(account.col("active").eq(true), account.col("mood").isNull()))
  .groupBy(account.col("name"))
  .having(clicks.gte(1))
  .orderBy(clicks.desc({ nulls: "last" }))
  .limit(10);

query.inspect();               // { sql: 'SELECT "a"."name", COUNT("e"."id") AS ..', values: [Value, ..] }
await pool.query(query);       // built as inspect() builds it; options come second
for await (const row of pool.stream(query, { tagged: true })) { /* .. */ }
```

- **Builders never change.** Each method returns a new builder from a copy of
  its receiver's pgorm-query state, so a base query can be extended in two
  directions. Builders come from the module's functions (`select`, `col`,
  `bind`, `call`, `caseWhen`, `caseOf`, `exists`, `scalar`, `tuple`) and the
  `Table` and `With` constructors.
- **Values are bound, identifiers quoted.** An operand that is not an
  expression is a parameter, inferred as one is or declared with `Value` — an
  enum's label is cast to its type. `null` is refused (test with `isNull()`, or
  bind `Value.null(kind)`), as is an interval, which pgorm's statement values do
  not hold: `bind(interval.toString()).cast("interval")`. A name is 1–63 bytes,
  always an identifier, its dots included.
- **Expressions**: comparisons and arithmetic (`eq` .. `gte`, `add` .. `mod`,
  `concat`), `and`/`or`/`not` and `Condition.all`/`any`, `isNull`, `isIn` a list
  or a `Select`, `between` (`{ symmetric }`), `like`/`ilike` with an `escape`,
  `startsWith`/`endsWith`/`containsText` (text, not patterns), `cast`,
  `collate`, `at`/`slice`, `caseWhen(..).when(..).else(..)`,
  `caseOf(x).when(..)`, `call` for the functions pgorm-query constructs
  (`uuidv7` and its shift among them), `exists` and `scalar`.
- **SELECT**: `select(..)`, `from`, `join` (`kind`, `lateral`), `crossJoin`,
  `where`, `groupBy`, `having`, `orderBy`, `limit`/`offset` (`null` removes),
  `distinct`, `union` .. `exceptAll`, `lock("update", { of, wait })`, `with`, and
  `as(alias)` to read a query as a FROM item. `new With(name, query, { columns,
  materialized }).cte(..)` and `With.recursive(name, query, { search, cycle })`
  are its common table expressions.
- **Refusals happen as a builder is called**: an argument that cannot be built
  — an unknown function, a negative limit, a LIKE escape of two characters, an
  empty tuple — throws a `ConstructionError` there, so a statement that cannot
  be built never exists. A statement binding more than 65,535 values is refused
  before anything is sent.

### Writes

```js
import { col, Conflict, deleteFrom, insert, ReturningRow, Table, update } from "./pgorm-napi/lib/index.js";

const account = new Table("account", { schema: "app" });

await pool.query(
  insert(account).columns("id", "name").values(1, "Alice").values(2, "Bob")
    .onConflict(Conflict.on("id").update("name"))
    .returning([col("id"), ReturningRow.old.col("id").isNull().as("inserted")]),
);
await pool.execute(update(account).set("visits", col("visits").add(1)).where(col("id").eq(1)));
await pool.query(deleteFrom(account).where(col("active").eq(false)).returning([ReturningRow.old.star()]));
```

- **INSERT** names its distinct columns once, then takes rows: `values(..)` per
  row, checked against the columns' count, `select(query)`, or
  `defaultValues()` with no columns. `overriding("systemValue" | "userValue")`
  is for identity columns.
- **Conflicts** are an arbiter and an action: `Conflict.doNothing()` for any
  conflict, or `Conflict.on(column | expression, ..)` — with `.where(..)` for a
  partial index — or `Conflict.onConstraint(name)`, followed by `.doNothing()`,
  `.update(column, ..)` (from `EXCLUDED`) or `.set(column, value)`, an update
  taking a `.where(..)` of its own. A target without its action is refused.
- **UPDATE** assigns each column once with `set`; **DELETE** comes from
  `deleteFrom`. Each needs `where(..)` or an explicit `allRows()` before it runs,
  and `from(item)` / `using(item)` add the items it reads.
- **RETURNING**: `returning(items, { oldAs, newAs })`, every column when
  `items` is empty. `ReturningRow.old` and `ReturningRow.new` read the row
  before and after the write; a renamed version answers only to its new name,
  `col(name, { table: newName })`.
- A write with a RETURNING list is a common table expression's body, and every
  write takes `with(..)`.
- An INSERT with no rows, an UPDATE with no assignment and an UPDATE or DELETE
  with neither `where` nor `allRows()` are refused, by `inspect()` and the
  terminals alike, before anything is sent.

### MERGE

```js
import { merge, MergeAction, ReturningRow, Table } from "./pgorm-napi/lib/index.js";

const target = new Table("account", { alias: "t" });
const source = new Table("staged", { alias: "s" });

await pool.query(
  merge(target, source, target.col("id").eq(source.col("id")))
    .whenMatched(MergeAction.update("name", source.col("name")).set("visits", target.col("visits").add(1)))
    .whenMatched(MergeAction.delete(), { condition: source.col("name").isNull() })
    .whenNotMatched(MergeAction.insert("id", source.col("id")).set("name", source.col("name")))
    .whenNotMatchedBySource(MergeAction.delete())
    .returningAction()
    .returning([target.col("id"), ReturningRow.old.col("name").as("was")]),
);
```

`merge(target, source, on)` gives a `PendingMerge`, which has no `inspect()`
and which every terminal refuses: PostgreSQL refuses a MERGE with no WHEN arm.
Its first arm gives the `Merge`. An arm takes only what its kind of row can
take — a target row (`whenMatched`, `whenNotMatchedBySource`) is updated,
deleted or left alone, a source row (`whenNotMatched`) is inserted or skipped —
and anything else is a `ConstructionError`. `MergeAction.update` and `.insert`
take their first column at once, so neither is ever empty. Within a kind of
row, a row takes the first conditional arm whose condition holds, otherwise the
one unconditional arm, which renders last; `returningAction()` puts
`merge_action()` first in the RETURNING list, `only()` writes `ONLY`, and a
MERGE takes a plain WITH clause and is one's body when it returns rows. The
source may be a `Table` or a `FromItem`, a subquery among them.

### SQL/JSON, windows and ranges

```js
import {
  call, col, FrameType, jsonDefault, jsonExists, jsonTable, JsonTableColumn as C, jsonValue,
  Range, select, Table, Value, Window, windowFunction,
} from "./pgorm-napi/lib/index.js";

const docs = new Table("docs", { alias: "d" });
const size = jsonValue(docs.col("doc"), "$.size", { returning: "integer", onEmpty: jsonDefault(0), onError: "error" });
const blue = jsonExists(docs.col("doc"), "$.tags[*] ? (@ == $Tag)", { passing: { Tag: "blue" } });
const items = jsonTable(docs.col("doc"), "$.items[*]", [C.ordinality("i"), C.value("n", "integer")], { alias: "jt" });
await pool.query(select(docs.col("id"), size.as("size"), items.col("n")).from(docs).from(items).where(blue));

const running = new Window().partitionBy(col("kind")).orderBy(col("at").asc())
  .frame(FrameType.rows.preceding(1).andFollowing(1).exclude("currentRow"));
await pool.query(select(call("sum", col("weight")).over(running).as("around"), windowFunction("rank").over("w"))
  .from(new Table("reading")).window("w", new Window().orderBy(col("weight").desc())));

await pool.query(select().from(new Table("booking"))
  .where(col("seats").overlaps(new Value(new Range(1, 10, "[]"), "int4range"))));
```

- **SQL/JSON**: `jsonExists`, `jsonValue`, `jsonQuery`, `jsonObject`,
  `jsonArray`, `jsonArrayQuery`, `jsonObjectAgg`, `jsonArrayAgg`, `jsonParse`
  (`JSON(..)`), `jsonScalar`, `jsonSerialize`, `formatJson` and
  `isJson`/`isNotJson`. A path is bound as `jsonpath`, PASSING takes an object
  of names to values, and each function takes only its own behaviours —
  `jsonExists` `"true" | "false" | "unknown" | "error"`, `jsonValue` `"null" |
  "error" | jsonDefault(v)`, `jsonQuery` those and `"emptyArray" |
  "emptyObject"`, with one `shaping`. `jsonValue` refuses to return `json` or
  `jsonb`; `jsonQuery` reads JSON out instead. RETURNING and JSON_TABLE's
  columns take a `DataType` — `new DataType("numeric", { precision, scale })`,
  `.array()` — or a built-in type's name.
- **JSON_TABLE** is a FROM item: `jsonTable(context, path, columns, { alias,
  passing, pathName, onError })`, its columns `JsonTableColumn.ordinality`,
  `.value`, `.query`, `.exists` and `.nested`, each with its own options. It
  needs a column and an alias, and is implicitly LATERAL, so `join(items,
  bind(true), { kind: "left" })` keeps a row whose document yields nothing.
- **Windows**: `expr.over(window | name)` on a function call or a JSON
  aggregate; `new Window().partitionBy(..).orderBy(..).frame(..)`; frames from
  `FrameType.rows`, `.range` or `.groups` offer only the ends that may follow
  their start; `windowFunction(name, ..)` for `row_number`, `rank`, `lag` and
  the others, usable only with `over`.
- **Ranges**: `contains` (`@>`), `containedBy` (`<@`) and `overlaps` (`&&`)
  over ranges, multiranges and arrays. A range binds as a `Value` naming its
  kind — a built-in one or a `CreatedRange` — never inferred.

## Schema

DDL is built from JavaScript as the statements are, through pgorm-query's DDL
builders, and runs through `execute`:

```js
import {
  alterTable, col, ColumnDef, createIndex, createSequence, createTable, createType, DataType,
  Table, TypeName, Value,
} from "./pgorm-napi/lib/index.js";

const mood = new TypeName("mood", { schema: "app" });
const booking = new Table("booking", { schema: "app" });

await pool.execute(createType(mood).values(["calm", "tense"]));
await pool.execute(
  createTable(booking)
    .column(new ColumnDef("id", "bigint").identity("always", { startWith: 1000 }))
    .column(new ColumnDef("room", "integer").notNull({ name: "room_present" }))
    .column(new ColumnDef("during", "tstzrange").notNull())
    .column(new ColumnDef("mood", mood).default(new Value("calm", mood)))
    .column(new ColumnDef("price", new DataType("numeric", { precision: 8, scale: 2 })).check(col("price").gte(0)))
    .primaryKey("id")
    .unique("room", { withoutOverlaps: "during", name: "no_double_booking" })
    .foreignKey("room", new Table("room", { schema: "app" }), "id", { onDelete: "cascade" }),
);
await pool.execute(alterTable(booking).addCheck(col("room").lt(500), { name: "small", notValid: true }));
await pool.execute(createIndex(booking, { on: "during", order: "desc" }).using("gist").where(col("room").gt(0)));
await pool.execute(createSequence("ticket").options({ incrementBy: 10, maxValue: null }));
```

- **A statement renders as it runs.** PostgreSQL takes no parameters in DDL,
  so a value a statement carries — a `DEFAULT`, a `CHECK`'s operands, an enum
  label, a comment — is written as pgorm-query's escaped literal, and
  `inspect()` gives `{ sql, values: [] }`. Names are identifiers, quoted.
  Nothing runs until a terminal runs it.
- **Tables**: `createTable(table)` with `column`, `primaryKey` (one; a later
  call replaces it), `unique`, `foreignKey` and `check`. A key takes `name`,
  `include`, `deferrability` and PostgreSQL 18's `withoutOverlaps` (its period
  column, written last); a unique key `nullsNotDistinct`. A foreign key pairs
  its columns with as many referenced ones and takes `onDelete`, `onUpdate`,
  `deferrability`, `enforcement` and PostgreSQL 18's `period: [column,
  referenced]`. A `ColumnDef` takes `notNull({ name, noInherit })`, `null`,
  `default`, `check`, `generated(expr, "stored" | "virtual")`,
  `identity("always" | "byDefault", sequenceOptions)`, `autoIncrement` and
  `collate`. `dropTable`, `renameTable`, `renameColumn`, `renameConstraint`
  and `truncateTable`; `commentOnTable` and `commentOnColumn`.
- **Alterations**: `alterTable(table)` has no action and so nothing to run;
  its first of `addColumn`, `modifyColumn`, `dropColumn`, `addPrimaryKey`,
  `addUnique`, `addForeignKey`, `addCheck`, `addNotNull`, `dropConstraint`,
  `validateConstraint`, `alterConstraint`, `setExpression` and
  `dropExpression` gives the statement, which takes more. `notValid` adds a
  foreign key, `CHECK` or `NOT NULL` without checking the rows already there,
  which `validateConstraint` checks later. A `modifyColumn` refuses what no
  `ALTER COLUMN` writes, a generated expression among it.
- **Indexes**: `createIndex(table, entry, { name })` over columns, expressions
  and `{ on, order, operatorClass }`, with `unique`, `nullsNotDistinct`,
  `ifNotExists`, `using(method)`, `include` and `where`; `dropIndex(table,
  name)`.
- **Types**: `createType(name)` — `.values(labels)` for an enumeration,
  `.attribute(name, type)` for a composite, `.asRange(subtype, { .. })` for a
  range — and `alterType(name)` with `addValue`, `renameTo`, `renameValue`,
  `renameAttribute` and a composite's `addAttribute`, `dropAttribute` and
  `alterAttribute`; `dropType`.
- **Sequences and extensions**: `createSequence(name)` and
  `alterSequence(name)` with `asType`, `options({ incrementBy, minValue,
  maxValue, startWith, cache, cycle })` — the options an identity column
  takes, `null` a bound's `NO` form — `ownedBy` and, altering, `restart`;
  `dropSequence` and `renameSequence`; `createExtension` and `dropExtension`.
- An options object holding a key its builder does not know is a
  `TypeError`, so a misspelt option is never left out.

## Pipelines

pgorm's PRQL-shaped pipeline is the module's `pipeline` namespace:
relation-to-relation stages over sources, compiled through prqlc to
PostgreSQL SQL and run as any statement is.

```js
import { pipeline as pl, Table } from "./pgorm-napi/lib/index.js";

const items = new Table("items", { schema: "app" });
const category = pl.col("items", "category");
const amount = pl.col("items", "amount");
const total = pl.alias("total");

const minimum = 10;
const top = pl.from(items)
  .filterWith((binder) => amount.gt(binder.bind(minimum)))   // $1, minted by this stage's binder
  .group(category)
  .aggregate(pl.sum(amount).as(total))
  .filter(total.gt(100))                                      // $2: a value is bound, not written
  .sort(total.desc())
  .take(5);                                                   // LIMIT 5: a count is no expression

top.inspect();               // { sql: "SELECT category, COALESCE(SUM(amount), 0) AS total ..", values: [..] }
await pool.query(top);
```

- **Stages**: `from(source)`, then `filter`, `derive`, `select`, `group(..)`
  and `aggregate(..)`, `window(over, ..)`, `sort`, `take`, `takeRange`,
  `join(source, on, { kind })`, `append`, `intersect`, `remove` and
  `distinct`, each returning a new pipeline. A source is a `Table` (read under
  its alias if it has one), a table's name, another pipeline — embedded whole,
  its bound values with it — or `pl.source(relation).named(name)`, which is
  how a relation meets itself.
- **Expressions** are the pipeline's own, not the statement builders':
  `pl.col(table, column)` (prqlc has no catalog, so columns are qualified),
  `pl.alias(name)` for a name a stage introduces, `pl.thisColumn` and
  `pl.thatColumn` for a join's sides, the comparison, logical and arithmetic
  methods, `coalesce`, `isNull`, `inArray`, `cast`, `as`, `asc`/`desc`,
  `pl.caseWhen`, the aggregates (`sum`, `min`, `max`, `average`, `stddev`,
  `count`, `countDistinct`, `countRows`) and the window functions
  (`rowNumber`, `rank`, `rankDense`, `lag`, `lead`, `first`, `last`) over
  `pl.over().by(..).sortBy(..).rows(start, end)`.
- **Values**: `pl.literal(v)` writes a value into the SQL; any other value an
  operand is given is bound, by the stage that takes it.
- **Binders**: each stage has a `With` form whose function is called once
  with a `Binder`; `bind(v)` mints one placeholder, reusable in the
  expressions the function returns. Like pgorm's lifetime-branded binder, it
  binds only while its function runs, and a placeholder anywhere but its own
  stage — a plain stage, another binder's, another pipeline, a window's keys —
  is a `LifecycleError`.
- **Terminals** are the pool's, connection's and transaction's, with their
  cardinality: `one` wants exactly one row, so ask for it with `.take(1)`.
  What prqlc cannot compile — an alias it reserves, a name it cannot write —
  is a `ConstructionError` before anything is sent.

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

`{ tagged: true }` gives each column as a `Value`
carrying its kind, which tells an `int2` from an `int8` and SQL NULL from JSON's
`null`. A range type a schema created is a `CreatedRange` kind; its value
travels as text for the statement to cast, `CAST($1::text AS app.floatrange)`,
as pgorm's own `DeriveCreatedRange` does.

## Errors

Every failure rejects with a subclass of `PgormError`: `DatabaseError` when
PostgreSQL refuses a statement, with its `sqlstate`, `severity` and optional
diagnostic fields; `ConnectionError` when the server cannot be reached;
`ConstructionError` for an argument pgorm cannot send; `DecodeError` for a
result that does not decode; `LifecycleError` for a handle used after it
closed or while another operation holds it; `TimeoutError` when no connection
frees up in time. Credentials never appear in a message.

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
a release build. `tests/values.test.ts`, `tests/connections.test.ts`, `tests/statements.test.ts`
and the other live files each make a database of their own, named for the
runtime and process, and drop it when they end.
`tests/statements-parity.test.ts` holds each statement family's cases under
`tests/parity/` to the golden file beside them, which the addon's Rust unit
tests build the same statements to with pgorm-query directly. The suite
connects in plaintext unless its connection string names an `sslmode`; with
`PGORM_TEST_CA` naming a PEM CA whose certificate for `localhost` the server
presents, it also holds verified TLS to that CA, as CI does. `deno.json` keeps
Deno on its global npm cache, so type checking `node:` imports needs no
`node_modules`.
