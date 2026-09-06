#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_debug_implementations)]

//! # pgorm-query
//!
//! A dynamic, PostgreSQL-only SQL query builder: expressions, queries and
//! schema statements built as abstract syntax trees through an ergonomic
//! API, rendered against the real PostgreSQL grammar.
//!
//! pgorm-query is the query layer of [pgorm](https://github.com/pgorm-rs/pgorm),
//! an async PostgreSQL ORM for Rust, and lives in the same repository. It is
//! a hard fork of [SeaQuery](https://github.com/SeaQL/sea-query) by SeaQL,
//! rebuilt around a single database: everything non-Postgres is deleted, and
//! statements the server would reject are unrepresentable where the type
//! system can make them so.
//!
//! ## Install
//!
//! ```toml
//! # Cargo.toml
//! [dependencies]
//! pgorm-query = "0"
//! ```
//!
//! ### Feature flags
//!
//! Macro: `derive` `attr`
//!
//! Type support is unconditional: `chrono`, `serde_json`, `rust_decimal`, `uuid`,
//! `ipnetwork`, `mac_address`, `pgvector`, Postgres arrays and intervals.
//!
//! ## Usage
//!
//! Table of Content
//!
//! 1. Basics
//!
//!     1. [Iden](#iden)
//!     1. [Expression](#expression)
//!     1. [Condition](#condition)
//!     1. [Statement Builders](#statement-builders)
//!
//! 1. Query Statement
//!
//!     1. [Query Select](#query-select)
//!     1. [Query Insert](#query-insert)
//!     1. [Query Update](#query-update)
//!     1. [Query Delete](#query-delete)
//!
//! 1. Advanced
//!     1. [Aggregate Functions](#aggregate-functions)
//!     1. [Casting](#casting)
//!     1. [Custom Function](#custom-function)
//!
//! 1. Schema Statement
//!
//!     1. [Table Create](#table-create)
//!     1. [Table Alter](#table-alter)
//!     1. [Table Drop](#table-drop)
//!     1. [Table Rename](#table-rename)
//!     1. [Table Truncate](#table-truncate)
//!     1. [Foreign Key Create](#foreign-key-create)
//!     1. [Foreign Key Drop](#foreign-key-drop)
//!     1. [Index Create](#index-create)
//!     1. [Index Drop](#index-drop)
//!
//! ### Motivation
//!
//! Why would you want to use a dynamic query builder?
//!
//! 1. Parameter bindings
//!
//! One of the headaches when using raw SQL is parameter binding. With pgorm-query you can:
//!
//! ```
//! # use pgorm_query::{*, tests_cfg::*};
//! assert_eq!(
//!     Query::select()
//!         .column(Character::Character)
//!         .from(Character::Table)
//!         .and_where(Expr::col(Character::Character).like("A"))
//!         .and_where(Expr::col(Character::Id).is_in([1, 2, 3]))
//!         .build(),
//!     (
//!         r#"SELECT "character" FROM "character" WHERE "character" LIKE $1 AND "id" IN ($2, $3, $4)"#
//!             .to_owned(),
//!         Values(vec![
//!             Value::String(Some(Box::new("A".to_owned()))),
//!             Value::Int(Some(1)),
//!             Value::Int(Some(2)),
//!             Value::Int(Some(3))
//!         ])
//!     )
//! );
//! ```
//!
//! 2. Dynamic query
//!
//! You can construct the query at runtime based on user inputs:
//!
//! ```
//! # use pgorm_query::{*, tests_cfg::*};
//! Query::select()
//!     .column(Character::Character)
//!     .from(Character::Table)
//!     .conditions(
//!         // some runtime condition
//!         true,
//!         // if condition is true then add the following condition
//!         |q| {
//!             q.and_where(Expr::col(Character::Id).eq(1));
//!         },
//!         // otherwise leave it as is
//!         |q| {},
//!     );
//! ```
//!
//! ### Iden
//!
//! `Iden` is a trait for identifiers used in any query statement.
//!
//! Commonly implemented by Enum where each Enum represents a table found in a database,
//! and its variants include table name and column name.
//!
//! [`Iden::unquoted()`] must be implemented to provide a mapping between Enum variants and its
//! corresponding string value.
//!
//! ```rust
//! use pgorm_query::*;
//!
//! // For example Character table with column id, character, font_size...
//! pub enum Character {
//!     Table,
//!     Id,
//!     FontId,
//!     FontSize,
//! }
//!
//! // Mapping between Enum variant and its corresponding string value
//! impl Iden for Character {
//!     fn unquoted(&self, s: &mut dyn std::fmt::Write) {
//!         write!(
//!             s,
//!             "{}",
//!             match self {
//!                 Self::Table => "character",
//!                 Self::Id => "id",
//!                 Self::FontId => "font_id",
//!                 Self::FontSize => "font_size",
//!             }
//!         )
//!         .unwrap();
//!     }
//! }
//! ```
//!
//! If you're okay with running another procedural macro, you can activate
//! the `derive` or `attr` feature on the crate to save you some boilerplate.
//! For more usage information, look at
//! [the derive examples](https://github.com/pgorm-rs/pgorm/tree/main/pgorm-query/pgorm-query-derive/tests/pass)
//! or [the attribute examples](https://github.com/pgorm-rs/pgorm/tree/main/pgorm-query/pgorm-query-attr/tests/pass).
//!
//! ```rust
//! #[cfg(feature = "derive")]
//! use pgorm_query::Iden;
//!
//! // This will implement Iden exactly as shown above
//! #[derive(Iden)]
//! enum Character {
//!     Table,
//! }
//! assert_eq!(Character::Table.to_string(), "character");
//!
//! // You can also derive a unit struct
//! #[derive(Iden)]
//! struct Glyph;
//! assert_eq!(Glyph.to_string(), "glyph");
//! ```
//!
//! ```rust
//! #[cfg(feature = "attr")]
//! # fn test() {
//! use pgorm_query::{enum_def, Iden};
//!
//! #[enum_def]
//! struct Character {
//!     pub foo: u64,
//! }
//!
//! // It generates the following along with Iden impl
//! # let not_real = || {
//! enum CharacterIden {
//!     Table,
//!     Foo,
//! }
//! # };
//!
//! assert_eq!(CharacterIden::Table.to_string(), "character");
//! assert_eq!(CharacterIden::Foo.to_string(), "foo");
//! # }
//! # #[cfg(feature = "attr")]
//! # test();
//! ```
//!
//!
//! ### Expression
//!
//! Use [`Expr`] to construct select, join, where and having expression in query.
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! # fn main() -> pgorm_query::error::Result<()> {
//! assert_eq!(
//!     Query::select()
//!         .column(Character::Character)
//!         .from(Character::Table)
//!         .and_where(
//!             Expr::expr(Expr::col(Character::FontSize).add(1))
//!                 .mul(2)
//!                 .eq(Expr::expr(Expr::col(Character::FontSize).div(2)).sub(1))
//!         )
//!         .and_where(
//!             Expr::col(Character::FontSize).in_subquery(
//!                 Query::select()
//!                     .expr(Expr::cust_with_values("ln($1 ^ $2)", [2.4, 1.2])?)
//!                     .take()
//!             )
//!         )
//!         .and_where(
//!             Expr::col(Character::Character)
//!                 .like("D")
//!                 .and(Expr::col(Character::Character).like("E"))
//!         )
//!         .to_string(),
//!     [
//!         r#"SELECT "character" FROM "character""#,
//!         r#"WHERE ("font_size" + 1) * 2 = ("font_size" / 2) - 1"#,
//!         r#"AND "font_size" IN (SELECT ln(2.4 ^ 1.2))"#,
//!         r#"AND ("character" LIKE 'D' AND "character" LIKE 'E')"#,
//!     ]
//!     .join(" ")
//! );
//! # Ok(())
//! # }
//! ```
//!
//! ### Condition
//!
//! If you have complex conditions to express, you can use the [`Condition`] builder,
//! usable for [`ConditionalStatement::cond_where`] and [`SelectStatement::cond_having`].
//!
//! ```
//! # use pgorm_query::{*, tests_cfg::*};
//! assert_eq!(
//!     Query::select()
//!         .column(Character::Id)
//!         .from(Character::Table)
//!         .cond_where(
//!             Cond::any()
//!                 .add(
//!                     Cond::all()
//!                         .add(Expr::col(Character::FontSize).is_null())
//!                         .add(Expr::col(Character::Character).is_null())
//!                 )
//!                 .add(
//!                     Cond::all()
//!                         .add(Expr::col(Character::FontSize).is_in([3, 4]))
//!                         .add(Expr::col(Character::Character).like("A%"))
//!                 )
//!         )
//!         .to_string(),
//!     [
//!         r#"SELECT "id" FROM "character""#,
//!         r#"WHERE"#,
//!         r#"("font_size" IS NULL AND "character" IS NULL)"#,
//!         r#"OR"#,
//!         r#"("font_size" IN (3, 4) AND "character" LIKE 'A%')"#,
//!     ]
//!     .join(" ")
//! );
//! ```
//!
//! There is also the [`any!`] and [`all!`] macro at your convenience:
//!
//! ```
//! # use pgorm_query::{*, tests_cfg::*};
//! Query::select().cond_where(any![
//!     Expr::col(Character::FontSize).is_in([3, 4]),
//!     all![
//!         Expr::col(Character::FontSize).is_null(),
//!         Expr::col(Character::Character).like("A%")
//!     ]
//! ]);
//! ```
//!
//! ### Statement Builders
//!
//! Statements are divided into 2 categories: Query and Schema.
//!
//! A schema statement carries no bind parameters, so it has exactly one rendering — its
//! [`Display`](std::fmt::Display):
//!
//! ```rust
//! # use pgorm_query::*;
//! # trait ExampleSchemaBuilder {
//! fn to_string(&self) -> String;
//! # }
//! ```
//!
//! A query statement is serialized through [`QueryStatementBuilder`], which offers both:
//!
//! ```rust
//! # use pgorm_query::*;
//! # trait ExampleQueryBuilder {
//! fn build(&self) -> (String, Values);
//!
//! fn to_string(&self) -> String;
//! # }
//! ```
//!
//! `build` builds a SQL statement as string and parameters to be passed to the database driver
//! through the binary protocol. This is the preferred way as it has less overhead and is more secure.
//!
//! `to_string` builds a SQL statement as string with parameters injected. This is good for testing
//! and debugging.
//!
//! ### Query Select
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let query = Query::select()
//!     .column(Character::Character)
//!     .column((Font::Table, Font::Name))
//!     .from(Character::Table)
//!     .left_join(Font::Table, Expr::col((Character::Table, Character::FontId)).equals((Font::Table, Font::Id)))
//!     .and_where(Expr::col(Character::FontSize).is_in([3, 4]))
//!     .and_where(Expr::col(Character::Character).like("A%"))
//!     .to_owned();
//!
//! assert_eq!(
//!     query.to_string(),
//!     r#"SELECT "character", "font"."name" FROM "character" LEFT JOIN "font" ON "character"."font_id" = "font"."id" WHERE "font_size" IN (3, 4) AND "character" LIKE 'A%'"#
//! );
//! ```
//!
//! ### Query Insert
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let query = Query::insert()
//!     .into_table(Character::Table)
//!     .columns([Character::Id, Character::Character])
//!     .values_panic([1.into(), "A".into()])
//!     .to_owned();
//!
//! assert_eq!(
//!     query.to_string(),
//!     r#"INSERT INTO "character" ("id", "character") VALUES (1, 'A')"#
//! );
//! ```
//!
//! ### Query Update
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let query = Query::update()
//!     .table(Character::Table)
//!     .values([(Character::Character, "A".into())])
//!     .and_where(Expr::col(Character::Id).eq(1))
//!     .to_owned();
//!
//! assert_eq!(
//!     query.to_string(),
//!     r#"UPDATE "character" SET "character" = 'A' WHERE "id" = 1"#
//! );
//! ```
//!
//! ### Query Delete
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let query = Query::delete()
//!     .from_table(Character::Table)
//!     .cond_where(
//!         Cond::any()
//!             .add(Expr::col(Character::Id).lt(1))
//!             .add(Expr::col(Character::Id).gt(10)),
//!     )
//!     .to_owned();
//!
//! assert_eq!(
//!     query.to_string(),
//!     r#"DELETE FROM "character" WHERE "id" < 1 OR "id" > 10"#
//! );
//! ```
//!
//! ### Aggregate Functions
//!
//! `max`, `min`, `sum`, `avg`, `count` etc
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let query = Query::select()
//!     .expr(Func::sum(Expr::col(Character::Id)))
//!     .from(Character::Table)
//!     .to_owned();
//!
//! assert_eq!(
//!     query.to_string(),
//!     r#"SELECT SUM("id") FROM "character""#
//! );
//! ```
//!
//! ### Casting
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let query = Query::select()
//!     .expr(Func::cast_as("hello", Alias::new("MyType")))
//!     .to_owned();
//!
//! assert_eq!(
//!     query.to_string(),
//!     r#"SELECT CAST('hello' AS MyType)"#
//! );
//! ```
//!
//! ### Custom Function
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! struct MyFunction;
//!
//! impl Iden for MyFunction {
//!     fn unquoted(&self, s: &mut dyn Write) {
//!         write!(s, "MY_FUNCTION").unwrap();
//!     }
//! }
//!
//! let query = Query::select()
//!     .expr(Func::cust(MyFunction).arg(Expr::val("hello")))
//!     .to_owned();
//!
//! assert_eq!(
//!     query.to_string(),
//!     r#"SELECT MY_FUNCTION('hello')"#
//! );
//! ```
//!
//! ### Table Create
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let table = Table::create(Character::Table)
//!     .col(ColumnDef::new(Character::Id).integer().not_null())
//!     .col(ColumnDef::new(Character::Character).string().not_null())
//!     .to_owned();
//!
//! assert_eq!(
//!     table.to_string(),
//!     r#"CREATE TABLE "character" ( "id" integer NOT NULL, "character" varchar NOT NULL )"#
//! );
//! ```
//!
//! ### Table Alter
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let table = Table::alter(Character::Table).add_column(
//!     ColumnDef::new(Alias::new("new_col"))
//!         .integer()
//!         .not_null()
//!         .default(100),
//! );
//!
//! assert_eq!(
//!     table.to_string(),
//!     r#"ALTER TABLE "character" ADD COLUMN "new_col" integer NOT NULL DEFAULT 100"#
//! );
//! ```
//!
//! ### Table Drop
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let table = Table::drop(Character::Table);
//!
//! assert_eq!(
//!     table.to_string(),
//!     r#"DROP TABLE "character""#
//! );
//! ```
//!
//! ### Table Rename
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let table = Table::rename(Character::Table, Alias::new("character_new"));
//!
//! assert_eq!(
//!     table.to_string(),
//!     r#"ALTER TABLE "character" RENAME TO "character_new""#
//! );
//! ```
//!
//! ### Table Truncate
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let table = Table::truncate(Character::Table);
//!
//! assert_eq!(
//!     table.to_string(),
//!     r#"TRUNCATE TABLE "character""#
//! );
//! ```
//!
//! ### Foreign Key Create
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let foreign_key = ForeignKey::create(
//!     Character::Table,
//!     Character::Id,
//!     Character::Table,
//!     Character::Id,
//! )
//! .name("FK_character_id")
//! .to_owned();
//!
//! assert_eq!(
//!     foreign_key.to_string(),
//!     r#"ALTER TABLE "character" ADD CONSTRAINT "FK_character_id" FOREIGN KEY ("id") REFERENCES "character" ("id")"#
//! );
//! ```
//!
//! ### Foreign Key Drop
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let foreign_key = ForeignKey::drop(Character::Table, "FK_character_id");
//!
//! assert_eq!(
//!     foreign_key.to_string(),
//!     r#"ALTER TABLE "character" DROP CONSTRAINT "FK_character_id""#
//! );
//! ```
//!
//! ### Index Create
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let index = Index::create(Character::Table, Character::Id)
//!     .name("idx-character-id")
//!     .to_owned();
//!
//! assert_eq!(
//!     index.to_string(),
//!     r#"CREATE INDEX "idx-character-id" ON "character" ("id")"#
//! );
//! ```
//!
//! ### Index Drop
//!
//! ```rust
//! # use pgorm_query::{*, tests_cfg::*};
//! let index = Index::drop("idx-character-id")
//!     .table(Character::Table)
//!     .to_owned();
//!
//! assert_eq!(
//!     index.to_string(),
//!     r#"DROP INDEX "idx-character-id""#
//! );
//! ```
//!
//! ## License
//!
//! Licensed under either of
//!
//! -   Apache License, Version 2.0
//!     ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
//! -   MIT license
//!     ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
//!
//! at your option.
//!
//! ## Contribution
//!
//! Unless you explicitly state otherwise, any contribution intentionally submitted
//! for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
//! dual licensed as above, without any additional terms or conditions.
//!
//! ## Provenance
//!
//! This crate began as [SeaQuery](https://github.com/SeaQL/sea-query) by SeaQL
//! and its contributors; the fork keeps their license and gratefully builds on
//! their work.

pub mod backend;
pub mod comment;
pub mod error;
pub mod expr;
pub mod extension;
pub mod foreign_key;
pub mod func;
pub mod index;
pub mod prepare;
pub mod query;
pub mod schema;
pub mod table;
mod template;
pub use template::CustomExpr;
pub mod token;
pub mod types;
pub mod value;
mod value_identity;

#[doc(hidden)]
#[cfg(feature = "tests-cfg")]
pub mod tests_cfg;

pub use backend::*;
pub use comment::*;
pub use expr::*;
pub use foreign_key::*;
pub use func::*;
pub use index::*;
pub use prepare::*;
pub use query::*;
pub use schema::*;
pub use table::*;
pub use token::*;
pub use types::*;
pub use value::*;

#[cfg(feature = "derive")]
pub use pgorm_query_derive::{Iden, IdenStatic};

#[cfg(feature = "attr")]
pub use pgorm_query_attr::enum_def;
