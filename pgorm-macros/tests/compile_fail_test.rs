//! Compile-failure verification for the derives whose contract is a *refusal*.
//!
//! Each fixture in `tests/compile-fail/` is paired with a `.stderr` snapshot
//! that trybuild asserts against. Only refusals the derives *word themselves*
//! belong here: a snapshot of a downstream `E0277` re-renders whenever rustc
//! or trybuild changes how it lays a diagnostic out, which says nothing about
//! the macro. Contracts of that shape are pinned on the generated tokens in
//! `sql_type_match`'s unit tests instead.

// [spec:pgorm:req:macros.derive.entity-model.reject+1/test]    struct must be `Model`; the entity must have a primary key
// [spec:pgorm:sem:macros.derive.entity-model.casing+1/test]    field names deriving no identifier, and an `enum_name` that spells none
// [spec:pgorm:syn:macros.derive.entity-model.attrs+1/test]    an unknown key at struct level, at field level, and in each of the three derives reading a subset of the same vocabulary
// [spec:pgorm:sem:macros.derive.from-query-result+2/test]    an unknown field key
// [spec:pgorm:sem:macros.derive.partial-model+3/test]    an unknown field key, and the both-keys conflict the accumulating parser makes reachable
// [spec:pgorm:sem:macros.derive.value-type+2/test]    non-tuple input and an unread `#[pgorm(...)]` key
// [spec:pgorm:syn:macros.derive.active-enum/test]    `rs_type` / `db_type` are mandatory
// [spec:pgorm:syn:macros.derive.relation+1/test]    `belongs_to` without `from` is rejected, and `from` / `to` of unequal arity are rejected while the arity is still known
// [spec:pgorm:req:entity.relation.builder+1/test]    a builder given no columns has no conversion into a `RelationDef`
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("./tests/compile-fail/*.rs");
}
