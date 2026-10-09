// The window family's parity cases — calls over inline and named windows,
// frames and the window functions — and the range operators', held to
// `windows.json` as `select.ts`'s are to theirs.
// [spec:pgorm:req:napi.windows/test]
// [spec:pgorm:req:napi.ranges/test]

import {
  bind,
  type Builder,
  call,
  col,
  CreatedRange,
  FrameType,
  jsonArrayAgg,
  jsonObjectAgg,
  Multirange,
  Range,
  select,
  Table,
  Value,
  Window,
  windowFunction,
} from "../../lib/index.js";

const reading = new Table("reading", { alias: "r" });

export const cases: Record<string, () => Builder> = {
  "window-inline-and-named": () => {
    const running = new Window().partitionBy(reading.col("kind")).orderBy(reading.col("at").asc());
    return select(
      reading.col("id"),
      call("sum", reading.col("weight")).over(running).as("running"),
      windowFunction("row_number").over("by_kind").as("n"),
      call("avg", reading.col("weight")).over(new Window()),
    ).from(reading).window("by_kind", new Window().partitionBy(reading.col("kind")));
  },
  "window-frames": () => {
    const ordered = new Window().orderBy(reading.col("id").asc());
    return select(
      call("sum", col("w")).over(ordered.frame(FrameType.rows.preceding(1).andFollowing(1).exclude("currentRow"))),
      call("sum", col("w")).over(ordered.frame(FrameType.rows.unboundedPreceding())),
      call("sum", col("w")).over(ordered.frame(FrameType.groups.currentRow().andUnboundedFollowing())),
      call("sum", col("w")).over(ordered.frame(FrameType.range.preceding(bind("1 day").cast("interval")).andCurrentRow())),
      call("sum", col("w")).over(ordered.frame(FrameType.rows.following(1).andFollowing(3))),
      call("sum", col("w")).over(ordered.frame(FrameType.rows.preceding(3).andPreceding(1))),
      call("sum", col("w")).over(ordered.frame(FrameType.rows.currentRow().exclude("ties"))),
      call("sum", col("w")).over(ordered.frame(FrameType.groups.unboundedPreceding().andUnboundedFollowing().exclude("noOthers"))),
    ).from(reading);
  },
  "window-functions": () => {
    const ordered = new Window().orderBy(col("id").asc());
    return select(
      windowFunction("rank").over(ordered),
      windowFunction("dense_rank").over(ordered),
      windowFunction("percent_rank").over(ordered),
      windowFunction("cume_dist").over(ordered),
      windowFunction("ntile", bind(new Value(4, "i32"))).over(ordered),
      windowFunction("first_value", col("w")).over(ordered),
      windowFunction("last_value", col("w")).over(ordered),
      windowFunction("nth_value", col("w"), bind(new Value(2, "i32"))).over(ordered),
      windowFunction("lag", col("w")).over(ordered),
      windowFunction("lead", col("w"), bind(new Value(1, "i32")), 0).over(ordered).as("next"),
    ).from(new Table("t"));
  },
  "window-json-aggregates": () =>
    select(
      jsonArrayAgg(col("v"), { orderBy: [col("v").asc()] }).over(new Window().partitionBy(col("k"))),
      jsonObjectAgg(col("k"), col("v")).over("w").as("pairs"),
    ).from(new Table("t")).window("w", new Window().orderBy(col("k").asc())),
  "ranges": () =>
    select().from(new Table("booking")).where(
      col("during").contains(bind(Temporal.Instant.from("2026-10-09T12:00:00Z")).cast("timestamptz"))
        .and(col("during").overlaps(new Value(new Range(Temporal.Instant.from("2026-10-09T00:00:00Z"), null), "tstzrange")))
        .and(col("seats").containedBy(new Value(new Range(1, 10, "[]"), "int4range")))
        .and(col("spans").overlaps(new Value(new Multirange([new Range(1, 3), new Range(5, 8)]), "int4multirange")))
        .and(col("tags").contains(["a", "b"]))
        .and(col("weights").overlaps(new Value(new Range(0.5, 1.5), new CreatedRange("floatrange", "f64", { schema: "app" })))),
    ),
};
