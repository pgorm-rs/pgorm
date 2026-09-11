//! The campaign's entity, model and graph declarations.
//!
//! A copy of `security/generative/bridge/src/{account,note,graphs}.rs`, which
//! is already PyO3-free — only the module paths differ. The hostile enum name
//! and the `before_save` hook are part of what the campaign tests, so they are
//! reproduced exactly rather than tidied: a replay whose entities are milder
//! than the binding's is replaying a different program.

/// The accounts table: every scalar kind the campaign exercises, a
/// schema-qualified enum whose name carries a quote and a non-ASCII character,
/// and a save hook that rewrites on insert and on update alike.
pub mod account {
    use pgorm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
    #[pgorm(
        rs_type = "String",
        db_type = "Enum",
        enum_name = "State\" 雪",
        schema_name = "fixture"
    )]
    pub enum State {
        #[pgorm(string_value = "calm")]
        Calm,
        #[pgorm(string_value = "O'Brien 雪")]
        Busy,
    }

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[pgorm(table_name = "accounts", schema_name = "fixture")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub tenant: i32,
        pub name: String,
        pub note: Option<String>,
        pub score: Option<i32>,
        pub rank: i32,
        pub active: bool,
        pub balance: Decimal,
        pub payload: Json,
        pub uuid: Uuid,
        pub created_at: DateTime,
        pub occurred_at: DateTimeUtc,
        pub event_date: Date,
        pub event_time: Time,
        pub state: State,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    #[async_trait::async_trait]
    impl ActiveModelBehavior for ActiveModel {
        async fn before_save<C>(mut self, _: &C, insert: bool) -> Result<Self, pgorm::Error>
        where
            C: pgorm::ConnectionTrait,
        {
            if insert {
                if let Some(name) = self.name.try_as_ref() {
                    self.name = pgorm::set(format!("{name}|hook"));
                }
            } else if let Some(rank) = self.rank.try_as_ref() {
                self.rank = pgorm::set(rank + 1);
            }
            Ok(self)
        }
    }
}

/// The notes table: the many side of the join, with a tenant column of its own
/// so a graph can leak across tenants without the relation noticing.
pub mod note {
    use pgorm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[pgorm(table_name = "notes", schema_name = "fixture")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub account_id: i32,
        pub tenant: i32,
        pub body: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The registered graph shapes, by arity, each aliasing its joined sources with
/// caller-supplied names so hostile aliases reach the quoting path.
pub mod graphs {
    use super::{account, note};
    use pgorm::pgorm_query::Alias;
    use pgorm::{EntityTrait, Opt, RelationDef, Req, SelectGraph};

    pub fn relation() -> RelationDef {
        note::Entity::belongs_to(account::Entity)
            .columns(note::Column::AccountId, account::Column::Id)
            .into()
    }

    pub fn optional(aliases: &[String]) -> SelectGraph<account::Entity, (Opt<note::Entity>,)> {
        account::Entity::graph()
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[0]))
    }

    pub fn required(aliases: &[String]) -> SelectGraph<account::Entity, (Req<note::Entity>,)> {
        account::Entity::graph()
            .join_one_as::<note::Entity>(relation().rev(), Alias::new(&aliases[0]))
    }

    pub fn self_join(aliases: &[String]) -> SelectGraph<account::Entity, (Opt<account::Entity>,)> {
        let relation = account::Entity::belongs_to(account::Entity)
            .columns(account::Column::Rank, account::Column::Id)
            .into();
        account::Entity::graph().join_maybe_as::<account::Entity>(relation, Alias::new(&aliases[0]))
    }

    pub type Notes2 = (Opt<note::Entity>, Opt<note::Entity>);

    pub fn arity3(aliases: &[String]) -> SelectGraph<account::Entity, Notes2> {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[1]))
    }

    pub type Notes3 = (Opt<note::Entity>, Opt<note::Entity>, Opt<note::Entity>);

    pub fn arity4(aliases: &[String]) -> SelectGraph<account::Entity, Notes3> {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[2]))
    }

    pub type Notes4 = (
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
    );

    pub fn arity5(aliases: &[String]) -> SelectGraph<account::Entity, Notes4> {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[2]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[3]))
    }

    pub type Notes5 = (
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
    );

    pub fn arity6(aliases: &[String]) -> SelectGraph<account::Entity, Notes5> {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[2]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[3]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[4]))
    }

    pub type Notes6 = (
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
        Opt<note::Entity>,
    );

    pub fn arity7(aliases: &[String]) -> SelectGraph<account::Entity, Notes6> {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[2]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[3]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[4]))
            .join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[5]))
    }
}
