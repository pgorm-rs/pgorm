use crate::{
    EntityTrait, QuerySelect, Related, RelationDef, Select, join_tbl_on_condition, unpack_table_ref,
};
use pgorm_query::{Alias, Condition, IntoIden, JoinType, SeaRc};
use std::{fmt::Debug, marker::PhantomData};

/// A Trait for links between Entities
// [spec:pgorm:req:entity.relation.linked+1]
pub trait Linked {
    #[allow(missing_docs)]
    type FromEntity: EntityTrait;

    #[allow(missing_docs)]
    type ToEntity: EntityTrait;

    /// Link for an Entity
    fn link(&self) -> Vec<RelationDef>;

    /// Find all the Entities that are linked to the Entity
    fn find_linked(&self) -> Select<Self::ToEntity> {
        let mut select = Select::new();
        for (i, mut rel) in self.link().into_iter().rev().enumerate() {
            let from_tbl = Alias::new(format!("r{i}")).into_iden();
            let to_tbl = if i > 0 {
                Alias::new(format!("r{}", i - 1)).into_iden()
            } else {
                unpack_table_ref(&rel.to_tbl)
            };
            let table_ref = rel.from_tbl;

            let mut condition = Condition::all().add(join_tbl_on_condition(
                SeaRc::clone(&from_tbl),
                SeaRc::clone(&to_tbl),
                rel.columns,
            ));
            if let Some(f) = rel.on_condition.take() {
                condition = condition.add(f(SeaRc::clone(&from_tbl), SeaRc::clone(&to_tbl)));
            }

            select
                .query()
                .join_as(JoinType::InnerJoin, table_ref, from_tbl, condition);
        }
        select
    }
}

/// The [`Linked`] chain a [`Related`] implementation already describes.
///
/// A one-hop link is the relation itself, and a junction-mediated one is
/// `[via, to]` — exactly what `Related` carries. Writing a unit struct plus an
/// eleven-line `Linked` impl to restate that is duplication, so hand this to
/// [`find_also_linked`](crate::Select::find_also_linked),
/// [`find_with_linked`](crate::Select::find_with_linked) or
/// [`ModelTrait::find_linked`](crate::ModelTrait::find_linked) instead.
///
/// The source entity is inferred from the position it is used in; only the
/// target is named:
///
/// ```
/// use pgorm::{EntityTrait, QueryTrait, RelatedLink, tests_cfg::{cake, fruit}};
///
/// assert_eq!(
///     cake::Entity::find()
///         .find_also_linked(RelatedLink::to(fruit::Entity))
///         .as_query()
///         .to_string(),
///     [
///         r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
///         r#""r0"."id" AS "B_id", "r0"."name" AS "B_name", "r0"."cake_id" AS "B_cake_id""#,
///         r#"FROM "cake""#,
///         r#"LEFT JOIN "fruit" AS "r0" ON "cake"."id" = "r0"."cake_id""#,
///     ]
///     .join(" ")
/// );
/// ```
///
/// Unlike [`find_also_related`](crate::Select::find_also_related), the linked
/// form aliases the joined table, so it is also what a self-relation needs:
/// `RelatedLink::to(Entity)` on an entity related to itself joins the table a
/// second time under `r0` rather than emitting an ambiguous self-join.
// [spec:pgorm:req:entity.relation.linked+1]
pub struct RelatedLink<E, R>(PhantomData<fn() -> (E, R)>);

impl<E, R> RelatedLink<E, R> {
    /// The link to the given entity, through whichever [`Related`]
    /// implementation connects it to the entity being selected from.
    pub fn to(_: R) -> Self {
        Self(PhantomData)
    }

    /// The link, with the target entity named as a type rather than a value.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<E, R> Default for RelatedLink<E, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E, R> Clone for RelatedLink<E, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E, R> Copy for RelatedLink<E, R> {}

impl<E, R> Debug for RelatedLink<E, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RelatedLink")
    }
}

// [spec:pgorm:req:entity.relation.linked+1]
impl<E, R> Linked for RelatedLink<E, R>
where
    E: EntityTrait + Related<R>,
    R: EntityTrait,
{
    type FromEntity = E;

    type ToEntity = R;

    fn link(&self) -> Vec<RelationDef> {
        match <E as Related<R>>::via() {
            Some(via) => vec![via, <E as Related<R>>::to()],
            None => vec![<E as Related<R>>::to()],
        }
    }
}
