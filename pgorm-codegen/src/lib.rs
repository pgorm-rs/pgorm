#![doc = include_str!("../README.md")]

mod entity;
mod error;
mod sql_schema;
mod util;

pub use entity::*;
pub use error::*;
pub use sql_schema::{entities_from_sql, parse_schema};

#[cfg(test)]
mod tests_cfg;
