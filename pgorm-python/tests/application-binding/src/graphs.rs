use crate::{account, note};
use pgorm::pgorm_query::Alias;
use pgorm::{EntityTrait, Opt, RelationDef, Req, SelectGraph};
use pyo3::prelude::*;

pub fn relation() -> RelationDef {
    account::Entity::has_many(note::Entity).into()
}

impl pgorm::Related<account::Entity> for note::Entity {
    fn to() -> RelationDef {
        note::Entity::belongs_to(account::Entity)
            .columns(note::Column::AccountId, account::Column::Id)
            .into()
    }
}

pub fn optional(aliases: &[String]) -> SelectGraph<account::Entity, (Opt<note::Entity>,)> {
    account::Entity::graph().join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[0]))
}

pub fn required(aliases: &[String]) -> SelectGraph<account::Entity, (Req<note::Entity>,)> {
    account::Entity::graph().join_one_as::<note::Entity>(relation(), Alias::new(&aliases[0]))
}

// [spec:pgorm:req:python.graph/test]
pub fn register(registry: &mut pgorm_python::entities::Registry) -> PyResult<()> {
    registry.graph::<account::Entity, (), _>("app.AccountOnly", |_| account::Entity::graph())?;
    registry.graph("app.AccountNotes", optional)?;
    registry.graph("app.RequiredNotes", required)?;
    registry.graph("app.MixedNotes", |aliases| {
        required(&aliases[..1]).join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[1]))
    })?;
    registry.graph("app.FourSources", |aliases| {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[2]))
    })?;
    registry.graph("app.FiveSources", |aliases| {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[2]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[3]))
    })?;
    registry.graph("app.SixSources", |aliases| {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[2]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[3]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[4]))
    })?;
    registry.graph("app.SevenSources", |aliases| {
        optional(&aliases[..1])
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[1]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[2]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[3]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[4]))
            .join_maybe_as::<note::Entity>(relation(), Alias::new(&aliases[5]))
    })?;
    Ok(())
}
