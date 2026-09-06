use crate::{
    EntityTrait, QuerySelect, Related, RelationDef, Select, join_tbl_on_condition, unpack_table_ref,
};
use pgorm_query::{Condition, Iden, IntoIden, JoinType, SharedIden};
use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};

/// The name a linked join binds one hop's table to.
///
/// A [`Linked`] chain aliases its hops positionally — `r0`, `r1`, … — because
/// the same table may appear at more than one hop and an unaliased self-join
/// is ambiguous. Those names are generated, but they are visible: a caller
/// ordering or grouping by a column of a joined table has to name it. So the
/// name is handed out as a value, derived by [`Linked::last_hop_alias`] from
/// the same chain the join walks, instead of being retyped at the call site —
/// where a chain that gained a hop would silently rebind the string to a
/// different table.
///
/// It is an ordinary [`Iden`], so it stands in any identifier position:
///
/// ```
/// use pgorm::{Linked, QueryOrder, QueryTrait, RelatedLink};
/// use pgorm::tests_cfg::{cake, fruit};
/// use pgorm_query::Expr;
///
/// let link = RelatedLink::<cake::Entity, fruit::Entity>::new();
/// let sql = link
///     .find_linked()
///     .order_by_asc(Expr::col((link.last_hop_alias(), cake::Column::Id)))
///     .as_query()
///     .to_string();
///
/// assert!(sql.ends_with(r#"ORDER BY "r0"."id" ASC"#), "{sql}");
/// ```
// [spec:pgorm:req:entity.relation.linked+3]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkedAlias(usize);

impl LinkedAlias {
    /// The alias bound to the `i`-th hop of a link, counting from zero.
    pub const fn hop(i: usize) -> Self {
        Self(i)
    }
}

// [spec:pgorm:req:entity.relation.linked+3]
impl Iden for LinkedAlias {
    fn unquoted(&self, s: &mut dyn fmt::Write) {
        let _ = write!(s, "r{}", self.0);
    }
}

impl fmt::Display for LinkedAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r{}", self.0)
    }
}

/// A Trait for links between Entities
// [spec:pgorm:req:entity.relation.linked+3]
pub trait Linked {
    #[allow(missing_docs)]
    type FromEntity: EntityTrait;

    #[allow(missing_docs)]
    type ToEntity: EntityTrait;

    /// Link for an Entity
    fn link(&self) -> Vec<RelationDef>;

    /// The alias bound while the chain's last relation is joined.
    ///
    /// [`find_linked`] walks the ladder backwards from `ToEntity`, so the last
    /// rung is the *source* table — which is why it is what scopes
    /// [`ModelTrait::find_linked`](crate::ModelTrait::find_linked) to one
    /// model, and what a caller ordering or filtering on a `FromEntity` column
    /// has to name.
    ///
    /// The builder that walks the chain derives the name here, and so should a
    /// caller, rather than writing `r3` and hoping the chain never grows.
    ///
    /// [`find_linked`]: Linked::find_linked
    fn last_hop_alias(&self) -> LinkedAlias {
        LinkedAlias::hop(self.link().len().saturating_sub(1))
    }

    /// Find all the Entities that are linked to the Entity
    fn find_linked(&self) -> Select<Self::ToEntity> {
        let mut select = Select::new();
        for (i, mut rel) in self.link().into_iter().rev().enumerate() {
            let from_tbl = LinkedAlias::hop(i).into_iden();
            let to_tbl = if i > 0 {
                LinkedAlias::hop(i - 1).into_iden()
            } else {
                unpack_table_ref(&rel.to_tbl)
            };
            let table_ref = rel.from_tbl;

            let mut condition = Condition::all().add(join_tbl_on_condition(
                SharedIden::clone(&from_tbl),
                SharedIden::clone(&to_tbl),
                rel.columns,
            ));
            if let Some(f) = rel.on_condition.take() {
                condition =
                    condition.add(f(SharedIden::clone(&from_tbl), SharedIden::clone(&to_tbl)));
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
/// [`ModelTrait::find_linked`](crate::ModelTrait::find_linked) instead.
///
/// The source entity is inferred from the position it is used in; only the
/// target is named:
///
/// ```
/// use pgorm::{ModelTrait, QueryTrait, RelatedLink, tests_cfg::{cake, fruit}};
///
/// let cake = cake::Model { id: 12, name: "Cheesecake".to_owned() };
///
/// assert_eq!(
///     cake.find_linked(RelatedLink::to(fruit::Entity))
///         .as_query()
///         .to_string(),
///     [
///         r#"SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id""#,
///         r#"FROM "fruit""#,
///         r#"INNER JOIN "cake" AS "r0" ON "r0"."id" = "fruit"."cake_id""#,
///         r#"WHERE "r0"."id" = 12"#,
///     ]
///     .join(" ")
/// );
/// ```
///
/// The linked form aliases the joined table, so it is also what a self-relation
/// needs: `RelatedLink::to(Entity)` on an entity related to itself joins the
/// table a second time under `r0` rather than emitting an ambiguous self-join.
// [spec:pgorm:req:entity.relation.linked+3]
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

// [spec:pgorm:req:entity.relation.linked+3]
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
