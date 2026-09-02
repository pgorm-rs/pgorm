//! Compile-failure verification for the derives whose contract is a *refusal*.
//!
//! Each fixture in `tests/compile-fail/` is paired with a `.stderr` snapshot
//! that trybuild asserts against.

// [spec:pgorm:req:macros.derive.entity-model.reject/test]    struct must be `Model`; the entity must have a primary key
// [spec:pgorm:syn:macros.derive.active-enum/test]    `rs_type` / `db_type` are mandatory
// [spec:pgorm:syn:macros.derive.relation/test]    `belongs_to` without `from` is rejected
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("./tests/compile-fail/*.rs");
}
