use pgorm_query::{tests_cfg::*, *};

mod case;
mod comment;
mod extension;
mod foreign_key;
mod frame;
mod func;
mod grouping;
mod index;
mod oracle;
mod oracle_pins;
mod oracle_sweep;
mod query;
mod render;
mod schema;
mod subscript;
mod table;
mod token;
mod type_vocab;
mod types;
mod value;
mod window;
mod write_relations;

#[path = "../common.rs"]
mod common;
use common::*;
