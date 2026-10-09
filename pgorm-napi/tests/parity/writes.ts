// The INSERT, UPDATE and DELETE family's parity cases, held to `writes.json`
// as `select.ts`'s are to theirs.
// [spec:pgorm:req:napi.writes/test]

import {
  bind,
  type Builder,
  call,
  col,
  Conflict,
  deleteFrom,
  insert,
  ReturningRow,
  select,
  Table,
  TypeName,
  update,
  Value,
  With,
} from "../../lib/index.js";

const account = new Table("account", { schema: "app" });

export const cases: Record<string, () => Builder> = {
  "insert-values-upsert": () =>
    insert(account)
      .columns("id", "name")
      .values(1, "Alice")
      .values(2, "O'Brien")
      .onConflict(Conflict.on("id").update("name"))
      .returning([col("id"), ReturningRow.old.col("id").isNull().as("inserted")]),
  "insert-select-do-nothing": () =>
    insert(account).columns("id").select(select(col("id")).from(new Table("staged"))).onConflict(Conflict.doNothing()),
  "insert-defaults": () => insert(account).defaultValues().returning(),
  "insert-constraint-set": () =>
    insert(account)
      .columns("id", "name")
      .values(1, "x")
      .onConflict(Conflict.onConstraint("account_pkey").set("name", bind("y")).update("note").where(col("name").ne("z"))),
  "insert-expression-target": () =>
    insert(account)
      .columns("id")
      .values(1)
      .overriding("systemValue")
      .onConflict(Conflict.on("id", call("lower", col("name"))).where(col("active").eq(true)).doNothing()),
  "insert-typed-values": () =>
    insert(account)
      .columns("small", "missing", "mood")
      .values(new Value(5, "i16"), Value.null("text"), new Value("calm", new TypeName("mood", { schema: "app" }))),
  "insert-with": () =>
    insert(account)
      .with(new With("src", select(col("id")).from(new Table("staged"))))
      .columns("id")
      .select(select(col("id")).from(new Table("src"))),
  "update-versions-renamed": () =>
    update(account)
      .set("name", "Bob")
      .set("visits", col("visits").add(1))
      .where(col("id").eq(2))
      .returning([col("name", { table: "before" }).as("was"), col("name", { table: "after" }).as("now")], {
        oldAs: "before",
        newAs: "after",
      }),
  "update-from-all-rows": () =>
    update(account).set("name", col("other", { table: "s" })).from(new Table("staged", { alias: "s" })).allRows(),
  "update-where-twice": () =>
    update(account).set("a", 1).where(col("b").eq(2)).where(col("c").eq(3)).allRows().returning([ReturningRow.new.star()]),
  "delete-using": () =>
    deleteFrom(account)
      .using(new Table("gone"))
      .where(col("id", { table: "gone" }).eq(col("id", { table: "account", schema: "app" })))
      .returning([ReturningRow.old.star()]),
  "delete-all-rows": () => deleteFrom(new Table("t")).allRows(),
  "delete-as-cte": () =>
    select().from(new Table("moved")).with(
      new With("moved", deleteFrom(account).where(col("id").gt(3)).returning([col("id")])).cte(
        "kept",
        update(new Table("log")).set("seen", true).allRows().returning([col("id")]),
      ),
    ),
};
