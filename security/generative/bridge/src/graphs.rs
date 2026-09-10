use crate::{account, note};
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
    account::Entity::graph().join_one_as::<note::Entity>(relation().rev(), Alias::new(&aliases[0]))
}

pub fn self_join(aliases: &[String]) -> SelectGraph<account::Entity, (Opt<account::Entity>,)> {
    let relation = account::Entity::belongs_to(account::Entity)
        .columns(account::Column::Rank, account::Column::Id)
        .into();
    account::Entity::graph().join_maybe_as::<account::Entity>(relation, Alias::new(&aliases[0]))
}

pub type Notes2 = (Opt<note::Entity>, Opt<note::Entity>);

pub fn arity3(aliases: &[String]) -> SelectGraph<account::Entity, Notes2> {
    optional(&aliases[..1]).join_maybe_as::<note::Entity>(relation().rev(), Alias::new(&aliases[1]))
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
