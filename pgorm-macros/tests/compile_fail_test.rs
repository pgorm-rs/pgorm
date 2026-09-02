//! Compile-failure verification for the derives whose contract is a *refusal*.
//!
//! Each fixture in `tests/compile-fail/` is paired with a `.stderr` snapshot
//! that trybuild asserts against.

// [spec:pgorm:req:macros.derive.entity-model.reject+1/test]    struct must be `Model`; the entity must have a primary key
// [spec:pgorm:sem:macros.derive.entity-model.casing+1/test]    field names deriving no identifier, and an `enum_name` that spells none
// [spec:pgorm:sem:macros.derive.value-type+1/test]    non-tuple input and an unread `#[pgorm(...)]` key
// [spec:pgorm:sem:macros.derive.entity-model.column-def+2/test]    a lifetime-bearing type reaches the `ValueType` fallback intact
// [spec:pgorm:syn:macros.derive.active-enum/test]    `rs_type` / `db_type` are mandatory
// [spec:pgorm:syn:macros.derive.relation/test]    `belongs_to` without `from` is rejected
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("./tests/compile-fail/*.rs");
}
