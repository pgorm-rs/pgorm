use pgorm_query::{tests_cfg::*, *};

mod comment;
mod extension;
mod foreign_key;
mod index;
mod oracle;
mod oracle_pins;
mod oracle_sweep;
mod query;
mod render;
mod schema;
mod table;
mod token;
mod type_vocab;
mod types;
mod value;
mod window;

#[path = "../common.rs"]
mod common;
use common::*;
