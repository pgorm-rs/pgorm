// The MERGE family's parity cases, held to `merge.json` as `select.ts`'s are
// to theirs.
// [spec:pgorm:req:napi.merge/test]

import { bind, type Builder, col, merge, MergeAction, ReturningRow, select, Table, Value, With } from "../../lib/index.js";

const target = new Table("account", { schema: "app", alias: "t" });
const source = new Table("staged", { alias: "s" });
const on = target.col("id").eq(source.col("id"));

export const cases: Record<string, () => Builder> = {
  "merge-every-arm": () =>
    merge(target, source, on)
      .whenMatched(MergeAction.update("name", source.col("name")).set("visits", target.col("visits").add(1)))
      .whenMatched(MergeAction.delete(), { condition: source.col("name").isNull() })
      .whenNotMatched(MergeAction.insert("id", source.col("id")).set("name", source.col("name")))
      .whenNotMatchedBySource(MergeAction.delete())
      .returningAction()
      .returning([target.col("id"), ReturningRow.old.col("name").as("was")]),
  "merge-unconditional-last": () =>
    merge(target, source, on)
      .whenMatched(MergeAction.doNothing())
      .whenMatched(MergeAction.update("name", "x"), { condition: source.col("kind").eq("rename") })
      .whenMatched(MergeAction.delete())
      .whenNotMatched(MergeAction.doNothing(), { condition: source.col("kind").eq("skip") })
      .whenNotMatched(MergeAction.insertDefaults()),
  "merge-overriding-only": () =>
    merge(target, source, on)
      .whenNotMatched(MergeAction.insert("id", bind(new Value(7, "i32"))).overriding("systemValue"))
      .whenNotMatchedBySource(MergeAction.update("active", false), { condition: target.col("active").eq(true) })
      .only(),
  "merge-with-source": () =>
    merge(target, new Table("fresh"), target.col("id").eq(col("id", { table: "fresh" })))
      .whenNotMatched(MergeAction.insert("id", col("id", { table: "fresh" })))
      .with(new With("fresh", select(col("id")).from(new Table("staged")).where(col("id").gt(10))))
      .returning(),
  "merge-subquery-source": () =>
    merge(target, select(col("id"), col("name")).from(new Table("staged")).as("s"), on)
      .whenMatched(MergeAction.update("name", source.col("name"))),
  "merge-as-cte": () =>
    select(col("merge_action"), col("id")).from(new Table("changed")).with(
      new With(
        "changed",
        merge(target, source, on)
          .whenMatched(MergeAction.update("name", source.col("name")))
          .whenNotMatched(MergeAction.insert("id", source.col("id")).set("name", source.col("name")))
          .returningAction()
          .returning([target.col("id")]),
      ),
    ),
};
