// The entity family's parity cases, built through the binding from the
// application's registrations and held to `parity.json`, which the
// application crate's Rust test builds with its entities directly.
// [spec:pgorm:req:napi.entity-reads/test]

import { type Compiled, entity, graph } from "../../../lib/index.js";

export function cases(): Record<string, () => Compiled> {
  const Account = entity("app.Account");
  const Membership = entity("app.Membership");
  const notes = graph("app.AccountNotes").find({ aliases: ["n"] });
  return {
    "find": () => Account.find().inspect(),
    "find-filtered": () =>
      Account.find().where(Account.col("id").gte(10)).orderBy(Account.col("display name").desc()).limit(20).inspect(),
    "find-enum": () => Account.find().where(Account.col("mood").eq("busy")).inspect(),
    "find-one": () => Account.find().inspect("one"),
    "find-composite": () =>
      Membership.find().where(Membership.col("account_id").eq(1)).where(Membership.col("team").eq("red")).inspect(),
    "graph-notes": () => notes.inspect(),
    "graph-mixed-default-aliases": () => graph("app.MixedNotes").find().inspect(),
    "graph-filtered": () => notes.where(notes.col(1, "body").eq("x")).orderBy(notes.col(0, "id").asc()).inspect(),
  };
}
