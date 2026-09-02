// The `pass` fixtures are compiled and run, covering the default / prefixed /
// suffixed generated enum names and the `table_name` override.
// [spec:pgorm:sem:macros.derive.enum-def/test]
#[test]
fn build_tests() {
    let t = trybuild::TestCases::new();
    //t.compile_fail("./tests/compile-fail/*.rs");

    // all of these are exactly the same as the examples in `examples/derive.rs`
    t.pass("./tests/pass/*.rs");
}
