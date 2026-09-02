//! Compile-time verification of the macro's contract.
//!
//! The `compile-fail` fixtures are the refusals — malformed grammar and inputs
//! that are not a lone string literal — each paired with a `.stderr` snapshot
//! trybuild asserts against. The `pass` fixtures are compiled *and run*, so the
//! expansion is checked in expression and const position and their assertions
//! are part of this test.

// [spec:pgorm:req:macros.sql.reject/test]    grammar rejections and non-literal input
// [spec:pgorm:def:macros.sql+1/test]    valid SQL compiles and keeps its text
// [spec:pgorm:req:macros.sql.ceiling/test]    unknown tables pass; the span is the whole literal
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("./tests/compile-fail/*.rs");
    t.pass("./tests/pass/*.rs");
}
