#![allow(unused_imports, dead_code)]

// These fixtures are byte-exact golden inputs for the transformer tests:
// the token spacing and trailing commas must match generator output, so
// rustfmt must never normalize them.
#[rustfmt::skip]
pub mod duplicated_many_to_many_paths;
#[rustfmt::skip]
pub mod many_to_many;
#[rustfmt::skip]
pub mod many_to_many_multiple;
#[rustfmt::skip]
pub mod self_referencing;
