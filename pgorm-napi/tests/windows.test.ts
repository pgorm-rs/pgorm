// Windows and range operators built from JavaScript against a live server, in
// either runtime: calls over inline and named windows, frames and their
// typestate, the window-only functions, and containment and overlap over
// ranges, multiranges and arrays.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import {
  bind,
  call,
  col,
  ConstructionError,
  CreatedRange,
  DatabaseError,
  FrameType,
  jsonArrayAgg,
  jsonValue,
  Multirange,
  Pool,
  Range,
  select,
  Table,
  Value,
  Window,
  windowFunction,
} from "../lib/index.js";
import { scratchDatabase } from "./support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

const reading = new Table("reading", { alias: "r" });
const booking = new Table("booking", { schema: "app" });

before(async () => {
  database = await scratchDatabase("pgorm_napi_windows");
  shared = new Pool(database.dsn, { maxSize: 4 });
  for (
    const sql of [
      "CREATE TABLE reading (id int8 PRIMARY KEY, kind text NOT NULL, weight int4 NOT NULL, at timestamptz NOT NULL)",
      `INSERT INTO reading VALUES
         (1, 'a', 10, '2026-10-01 00:00Z'), (2, 'a', 20, '2026-10-02 00:00Z'), (3, 'a', 30, '2026-10-04 00:00Z'),
         (4, 'b', 1, '2026-10-01 00:00Z'), (5, 'b', 2, '2026-10-01 12:00Z')`,
      "CREATE SCHEMA app",
      "CREATE TYPE app.floatrange AS RANGE (subtype = float8)",
      `CREATE TABLE app.booking (
         id int8 PRIMARY KEY, during tstzrange, seats int4range, spans int4multirange,
         tags text[], weights app.floatrange
       )`,
      `INSERT INTO app.booking VALUES
         (1, '[2026-10-09 10:00Z, 2026-10-09 14:00Z)', '[2,4]', '{[1,3),[10,12)}', '{a,b,c}', '[0.5,1.0)'),
         (2, '[2026-10-10 10:00Z, 2026-10-10 14:00Z)', '[8,20)', '{[5,6)}', '{c}', '[2.0,3.0]')`,
    ]
  ) {
    await pool().execute(sql);
  }
});

after(async () => {
  await shared?.close();
  await database?.drop();
});

function refused(pattern: RegExp): (error: unknown) => true {
  return (error: unknown) => {
    assert.ok(error instanceof ConstructionError, `expected a ConstructionError, got ${error}`);
    assert.match(error.message, pattern);
    return true;
  };
}

const byId = reading.col("id").asc();

// [spec:pgorm:req:napi.windows/test]
test("a call runs over an inline window and over one the statement names", async () => {
  const running = new Window().partitionBy(reading.col("kind")).orderBy(byId);
  const rows = await pool().query(
    select(
      reading.col("id"),
      call("sum", reading.col("weight")).over(running).as("running"),
      windowFunction("row_number").over("by_kind").as("n"),
      call("count", reading.col("id")).over(new Window()).as("all"),
    ).from(reading).window("by_kind", new Window().partitionBy(reading.col("kind")).orderBy(byId)).orderBy(byId),
  );
  assert.deepStrictEqual(rows.map((row) => [row.id, row.running, row.n, row.all]), [
    [1n, 10n, 1n, 5n],
    [2n, 30n, 2n, 5n],
    [3n, 60n, 3n, 5n],
    [4n, 1n, 1n, 5n],
    [5n, 3n, 2n, 5n],
  ]);
});

// [spec:pgorm:req:napi.windows/test]
test("a frame's mode, start, end and exclusion decide which rows a call reads", async () => {
  const ordered = new Window().partitionBy(reading.col("kind")).orderBy(byId);
  const sum = (frame: Parameters<Window["frame"]>[0]) => call("sum", reading.col("weight")).over(ordered.frame(frame));
  const rows = await pool().query(
    select(
      reading.col("id"),
      sum(FrameType.rows.preceding(1).andFollowing(1).exclude("currentRow")).as("neighbours"),
      sum(FrameType.rows.unboundedPreceding()).as("upto"),
      sum(FrameType.rows.currentRow().andUnboundedFollowing()).as("rest"),
      sum(FrameType.rows.following(1).andFollowing(5)).as("after"),
      call("sum", reading.col("weight")).over(
        new Window().partitionBy(reading.col("kind")).orderBy(reading.col("at").asc())
          .frame(FrameType.range.preceding(bind("1 day").cast("interval")).andCurrentRow()),
      ).as("day"),
    ).from(reading).orderBy(byId),
  );
  assert.deepStrictEqual(rows.map((row) => [row.id, row.neighbours, row.upto, row.rest, row.after, row.day]), [
    [1n, 20n, 10n, 60n, 50n, 10n],
    [2n, 40n, 30n, 50n, 30n, 30n],
    [3n, 20n, 60n, 30n, null, 30n],
    [4n, 2n, 1n, 3n, 2n, 1n],
    [5n, 1n, 3n, 2n, null, 3n],
  ]);
});

// [spec:pgorm:req:napi.windows/test]
test("the window-only functions and the JSON aggregates run over a window", async () => {
  const ordered = new Window().orderBy(reading.col("weight").desc());
  const rows = await pool().query(
    select(
      reading.col("id"),
      windowFunction("rank").over(ordered).as("rank"),
      windowFunction("lag", reading.col("weight"), bind(new Value(1, "i32")), bind(new Value(0, "i32"))).over(ordered).as("prev"),
      windowFunction("ntile", bind(new Value(2, "i32"))).over(ordered).as("half"),
      windowFunction("nth_value", reading.col("id"), bind(new Value(2, "i32"))).over(ordered).as("second"),
      jsonArrayAgg(reading.col("id")).over(new Window().partitionBy(reading.col("kind")).orderBy(byId)
        .frame(FrameType.rows.unboundedPreceding().andUnboundedFollowing())).as("ids"),
    ).from(reading).orderBy(reading.col("weight").desc()),
  );
  assert.deepStrictEqual(rows.map((row) => [row.id, row.rank, row.prev, row.half, row.second, row.ids]), [
    [3n, 1n, 0, 1, null, [1, 2, 3]],
    [2n, 2n, 30, 1, 2n, [1, 2, 3]],
    [1n, 3n, 20, 1, 2n, [1, 2, 3]],
    [5n, 4n, 10, 2, 2n, [4, 5]],
    [4n, 5n, 2, 2, 2n, [4, 5]],
  ]);
  await assert.rejects(
    pool().query(select(jsonArrayAgg(reading.col("id"), { orderBy: [byId] }).over(new Window())).from(reading)),
    (error: unknown) => error instanceof DatabaseError && error.sqlstate === "0A000",
  );
});

// [spec:pgorm:req:napi.windows/test]
test("OVER follows only a call, a frame only ends after its start, and a window-only function needs its window", () => {
  const ordered = new Window().orderBy(byId);
  assert.throws(() => col("x").over(ordered), refused(/OVER follows only a function call/));
  assert.throws(() => col("x").add(1).over(ordered), refused(/OVER follows only/));
  assert.throws(() => call("lower", col("x")).cast("text").over(ordered), refused(/OVER follows only/));
  assert.throws(() => jsonValue(col("doc"), "$.a").over(ordered), refused(/OVER follows only/));
  assert.throws(() => call("sum", col("x")).over(col("w") as never), refused(/Window or a window's name, not an expression/));
  assert.throws(() => call("sum", col("x")).over(""), refused(/identifier/));
  assert.throws(() => ordered.frame(FrameType.rows.following(1) as never), refused(/following start needs an end/));
  assert.throws(() => ordered.frame(col("x") as never), refused(/frame is a Frame/));
  const following = FrameType.rows.following(1) as unknown as { andCurrentRow?: unknown; andPreceding?: unknown; exclude?: unknown };
  assert.equal(following.andCurrentRow, undefined);
  assert.equal(following.andPreceding, undefined);
  assert.equal(following.exclude, undefined);
  assert.equal((FrameType.rows.currentRow() as unknown as { andPreceding?: unknown }).andPreceding, undefined);
  assert.throws(() => select(windowFunction("rank") as never), refused(/not a window function with no window/));
  assert.throws(() => windowFunction("rank", 1), refused(/no "rank" taking 1 argument/));
  assert.throws(() => windowFunction("lag"), refused(/no "lag" taking 0/));
  assert.throws(() => windowFunction("median" as never, col("x")), refused(/no "median"/));
  assert.throws(() => select().window("w", col("x") as never), refused(/named window is a Window/));
  assert.throws(() => FrameType.rows.currentRow().exclude("others" as never), refused(/exclusion/));
});

// [spec:pgorm:req:napi.ranges/test]
test("contains, containedBy and overlaps compare ranges, multiranges and arrays", async () => {
  const ids = async (predicate: ReturnType<typeof col>) =>
    (await pool().query(select(booking.col("id")).from(booking).where(predicate).orderBy(booking.col("id").asc())))
      .map((row) => row.id);
  const noon = Temporal.Instant.from("2026-10-09T12:00:00Z");
  assert.deepStrictEqual(await ids(booking.col("during").contains(bind(noon).cast("timestamptz"))), [1n]);
  assert.deepStrictEqual(
    await ids(booking.col("during").overlaps(new Value(new Range(Temporal.Instant.from("2026-10-10T13:00:00Z"), null), "tstzrange"))),
    [2n],
  );
  assert.deepStrictEqual(await ids(booking.col("seats").containedBy(new Value(new Range(1, 10, "[]"), "int4range"))), [1n]);
  assert.deepStrictEqual(await ids(booking.col("seats").contains(new Value(Range.empty(), "int4range"))), [1n, 2n]);
  assert.deepStrictEqual(
    await ids(booking.col("spans").overlaps(new Value(new Multirange([new Range(2, 3), new Range(11, 15)]), "int4multirange"))),
    [1n],
  );
  assert.deepStrictEqual(await ids(booking.col("tags").contains(["a", "c"])), [1n]);
  assert.deepStrictEqual(await ids(booking.col("tags").containedBy(["c", "d"])), [2n]);
  assert.deepStrictEqual(await ids(booking.col("tags").overlaps(["c"])), [1n, 2n]);
  const floats = new CreatedRange("floatrange", "f64", { schema: "app" });
  assert.deepStrictEqual(await ids(booking.col("weights").overlaps(new Value(new Range(0.9, 2.5), floats))), [1n, 2n]);
  assert.deepStrictEqual(await ids(booking.col("weights").containedBy(new Value(new Range(0.0, 1.0, "[]"), floats))), [1n]);
});

// [spec:pgorm:req:napi.ranges/test]
test("a range is bound only as a Value that declares its kind", () => {
  assert.throws(() => col("seats").contains(new Range(1, 5) as never), refused(/Value/));
  assert.throws(() => col("seats").contains(new Multirange([new Range(1, 5)]) as never), refused(/Value/));
  assert.equal(
    col("seats").overlaps(new Value(new Range(1, 5), "int4range")).inspect().sql,
    'SELECT "seats" && $1',
  );
});
