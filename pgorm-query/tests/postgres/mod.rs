use pgorm_query::{tests_cfg::*, *};

mod case;
mod collate;
mod comment;
mod composite;
mod conflict_constraint;
mod deferrability;
mod extension;
mod foreign_key;
mod frame;
mod func;
mod grouping;
mod index;
mod merge;
mod oracle;
mod oracle_pins;
mod oracle_sweep;
mod query;
mod range;
mod render;
mod schema;
mod sequence;
mod subscript;
mod table;
mod table_constraint;
mod token;
mod type_vocab;
mod types;
mod value;
mod window;
mod write_relations;

#[path = "../common.rs"]
mod common;
use common::*;
