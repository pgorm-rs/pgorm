#[path = "../../application-binding/src/account.rs"]
pub mod account;
#[path = "../../application-binding/src/note.rs"]
pub mod note;

use pgorm::pgorm_query::Alias;
use pgorm::{EntityTrait, Opt, RelationDef, Req, SelectGraph};

impl pgorm::Related<account::Entity> for note::Entity {
    fn to() -> RelationDef {
        note::Entity::belongs_to(account::Entity)
            .columns(note::Column::AccountId, account::Column::Id)
            .into()
    }
}

pub fn account_only(_: &[String]) -> SelectGraph<account::Entity, ()> {
    account::Entity::graph()
}

pub fn optional(aliases: &[String]) -> SelectGraph<account::Entity, (Opt<note::Entity>,)> {
    account::Entity::graph().join_maybe_as::<note::Entity>(
        account::Entity::has_many(note::Entity).into(),
        Alias::new(&aliases[0]),
    )
}

pub fn required(aliases: &[String]) -> SelectGraph<account::Entity, (Req<note::Entity>,)> {
    account::Entity::graph().join_one_as::<note::Entity>(
        account::Entity::has_many(note::Entity).into(),
        Alias::new(&aliases[0]),
    )
}
