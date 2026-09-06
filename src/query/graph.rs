//! The N-ary relational read: a root entity plus a typed list of joined,
//! decoded sources.
//!
//! [`SelectGraph<E, S>`] is one shape at every arity where the pair surface
//! had a bespoke type per pair. The declaration is the whole contract:
//!
//! - the *slot kind is the join type is the decode shape* — [`Opt<F>`] is a
//!   `LEFT JOIN` decoded through the absence witness as `Option<F::Model>`,
//!   [`Req<F>`] is an `INNER JOIN` decoded as a bare `F::Model` — so
//!   "LEFT-joined but decoded as required" is unrepresentable;
//! - the projection is *generated only*, by the one writer
//!   ([`project_source`], prefix `s{i}_`, the `select_as` enum-cast
//!   discipline included), and the builder implements no [`QuerySelect`], so
//!   a hand-edited projection that the decode does not expect cannot be
//!   written;
//! - the decode target is `(E::Model, S1::Out, …, Sn::Out)`, computed from
//!   the declared slot tuple, so a mismatch is a type error rather than a
//!   run-time surprise.
//!
//! [`QuerySelect`]: crate::QuerySelect

use core::marker::PhantomData;
use std::fmt;

use pgorm_query::{
    Alias, Condition, DynIden, Expr, IntoIden, JoinType, SelectExpr, SelectStatement, SharedIden,
};

use super::helper::join_condition;
use crate::{
    ColumnTrait, EntityTrait, Error, FromQueryResult, IdenStr, Identity, Iterable,
    PrimaryKeyToColumn, QueryFilter, QueryOrder, QueryResult, QueryTrait, Related, RelationDef,
};

/// The closure shape a call-site `ON` predicate takes: the join's left and
/// right identifiers in, a [`Condition`] out — the same shape
/// [`RelationDef::on_condition`] takes.
type OnCondition = Box<dyn Fn(DynIden, DynIden) -> Condition + Send + Sync>;

mod sealed {
    /// [`Slot`](super::Slot) is sealed: the join type, the decode and the
    /// output shape are one declaration, and an outside implementor could
    /// only make the three disagree.
    pub trait Sealed {}
}

/// A joined source that must match: `INNER JOIN`, decoded as a bare `Model`.
///
/// The type states that the join cannot miss, so there is no `Option` to
/// unwrap for a row the join guarantees.
// [spec:pgorm:sem:query.graph.slots]
pub struct Req<F>(PhantomData<F>);

/// A joined source that may be absent: `LEFT JOIN`, decoded through the
/// absence witness ([`FromQueryResult::from_query_result_optional`]) as
/// `Option<Model>`.
// [spec:pgorm:sem:query.graph.slots]
pub struct Opt<F>(PhantomData<F>);

impl<F> fmt::Debug for Req<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Req")
    }
}

impl<F> fmt::Debug for Opt<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Opt")
    }
}

impl<F: EntityTrait> sealed::Sealed for Req<F> {}
impl<F: EntityTrait> sealed::Sealed for Opt<F> {}

/// One declared source of a [`SelectGraph`]: its entity, its output shape,
/// the join type that shape implies, and the decode that reads it.
///
/// The trait is sealed — [`Req<F>`] and [`Opt<F>`] are the only slots — so
/// the four cannot be recombined into a pairing the graph does not mean.
// [spec:pgorm:sem:query.graph.slots]
pub trait Slot: sealed::Sealed {
    /// The entity behind the slot.
    type Entity: EntityTrait;

    /// What one row contributes for this slot: `F::Model` for [`Req`],
    /// `Option<F::Model>` for [`Opt`].
    type Out;

    /// The join type the slot's shape implies.
    const JOIN: JoinType;

    /// Decode this slot's columns out of `res`, under the prefix `pre` the
    /// writer aliased them with.
    fn decode(res: &QueryResult, pre: &str) -> Result<Self::Out, Error>;
}

// [spec:pgorm:sem:query.graph.slots]
impl<F: EntityTrait> Slot for Req<F> {
    type Entity = F;
    type Out = F::Model;
    const JOIN: JoinType = JoinType::InnerJoin;

    // [spec:pgorm:sem:query.graph.decode]
    fn decode(res: &QueryResult, pre: &str) -> Result<Self::Out, Error> {
        F::Model::from_query_result(res, pre)
    }
}

// [spec:pgorm:sem:query.graph.slots]
impl<F: EntityTrait> Slot for Opt<F> {
    type Entity = F;
    type Out = Option<F::Model>;
    const JOIN: JoinType = JoinType::LeftJoin;

    // [spec:pgorm:sem:query.graph.decode]
    fn decode(res: &QueryResult, pre: &str) -> Result<Self::Out, Error> {
        F::Model::from_query_result_optional(res, pre)
    }
}

/// The secondary order entries one decoded source contributes to a cursor's
/// sort key: one unary entry per primary-key column.
pub(crate) type Tiebreaks = Vec<(DynIden, Identity)>;

/// One decoded source's primary key as cursor tiebreaks, qualified with the
/// source's *effective* identifier — its bound alias when it has one — so a
/// tiebreak names the same table the projection and the `ON` clause do.
// [spec:pgorm:sem:query.graph.cursor]
pub(crate) fn qualified_pk_tiebreaks<F: EntityTrait>(qualifier: &DynIden) -> Tiebreaks {
    <F::PrimaryKey as Iterable>::iter()
        .map(|pk| {
            (
                SharedIden::clone(qualifier),
                Identity::Unary(SharedIden::new(pk.into_column())),
            )
        })
        .collect()
}

/// The declared slot tuple read as a list: what a cursor needs to install one
/// tiebreak per decoded slot without a call site naming a column.
///
/// The declaration that fixed the joins fixes the tiebreak set, so the trait
/// is sealed for the reason [`Slot`] is — an outside implementor could only
/// make the two disagree. It is implemented for `()` and for every slot tuple
/// the graph can declare.
// [spec:pgorm:sem:query.graph.cursor]
pub trait Slots: sealed::Sealed {
    /// The primary-key tiebreaks of every declared slot except `skip`, in
    /// declaration order.
    ///
    /// `qualifiers` holds the slots' effective identifiers in declaration
    /// order — the root's is not among them — and slots are numbered from 1
    /// as the projection prefixes are (`s1_` is the first slot), so
    /// `skip == 0` skips nothing.
    fn tiebreaks(qualifiers: &[DynIden], skip: usize) -> Tiebreaks;
}

/// The slot declared at position `I`, counted from 1 as the projection
/// prefixes are: `s1_` is the first slot, the root being source 0.
///
/// This is how [`cursor_by_on`](crate::SelectGraph::cursor_by_on) names a
/// slot — positionally, at compile time. A position no slot occupies has no
/// implementation, so asking for it is a compile error rather than a silently
/// mis-qualified column.
// [spec:pgorm:sem:query.graph.cursor]
pub trait SlotAt<const I: usize>: Slots {
    /// The slot at that position, whose entity types the order columns.
    type Slot: Slot;
}

/// A slotless graph declares no tiebreaks; its cursor is a single-table one.
// [spec:pgorm:sem:query.graph.cursor]
impl Slots for () {
    fn tiebreaks(_qualifiers: &[DynIden], _skip: usize) -> Tiebreaks {
        Tiebreaks::new()
    }
}

impl sealed::Sealed for () {}

/// Generate the `SlotAt` impls of one slot-tuple arity, one position per
/// recursion, so each impl sees the whole tuple and the single slot it picks.
macro_rules! slot_at {
    ( ( $( $all:ident ),+ ) ; ) => {};
    ( ( $( $all:ident ),+ ) ; $s:ident @ $i:literal $( , $rest:ident @ $ri:literal )* ) => {
        // [spec:pgorm:sem:query.graph.cursor]
        impl< $( $all: Slot ),+ > SlotAt<$i> for ( $( $all, )+ ) {
            type Slot = $s;
        }

        slot_at!( ( $( $all ),+ ) ; $( $rest @ $ri ),* );
    };
}

/// Generate the slot-list machinery for one slot-tuple arity: the seal, the
/// tiebreak list, and one `SlotAt` impl per declared position.
macro_rules! slots {
    ( $( $s:ident @ $i:literal ),+ ) => {
        impl< $( $s: Slot ),+ > sealed::Sealed for ( $( $s, )+ ) {}

        // [spec:pgorm:sem:query.graph.cursor]
        impl< $( $s: Slot ),+ > Slots for ( $( $s, )+ ) {
            fn tiebreaks(qualifiers: &[DynIden], skip: usize) -> Tiebreaks {
                let sources: &[fn(&DynIden) -> Tiebreaks] =
                    &[ $( qualified_pk_tiebreaks::<<$s as Slot>::Entity> ),+ ];

                sources
                    .iter()
                    .zip(qualifiers)
                    .enumerate()
                    .filter(|(index, _)| index + 1 != skip)
                    .flat_map(|(_, (tiebreaks, qualifier))| tiebreaks(qualifier))
                    .collect()
            }
        }

        slot_at!( ( $( $s ),+ ) ; $( $s @ $i ),+ );
    };
}

slots!(S1 @ 1);
slots!(S1 @ 1, S2 @ 2);
slots!(S1 @ 1, S2 @ 2, S3 @ 3);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4, S5 @ 5);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4, S5 @ 5, S6 @ 6);

/// THE prefix scheme: the alias the writer gives `column` of the `index`-th
/// decoded source, `s{index}_{column}`.
///
/// Both projection writers — [`project_source`] into a [`SelectStatement`]
/// and the pipeline's `select_sources` terminal into prqlc's PL AST — mint
/// their aliases here, and the per-prefix decode reads exactly these names,
/// so the scheme is one code path rather than a convention three sites
/// repeat.
// [spec:pgorm:sem:query.graph.writer]
pub(crate) fn source_column_alias(index: usize, column: &str) -> String {
    format!("s{index}_{column}")
}

/// THE read-cast discipline, stated once: the type a projected column is
/// `CAST` to on selection, or `None` to project it untouched.
///
/// This is the decision the default [`ColumnTrait::select_as`] renders on
/// the SQL side — an enum column reads back as `text`, an array of enums as
/// `text[]`, everything else as itself — restated as data so the writer
/// that cannot dispatch through `select_as` (the pipeline's, which emits
/// PRQL nodes rather than [`SimpleExpr`](pgorm_query::SimpleExpr)) casts by
/// the same rule. A column that *overrides* `select_as` is honoured only by
/// the [`SelectStatement`] writer, which still calls the method.
// [spec:pgorm:sem:query.graph.writer]
pub(crate) fn source_read_cast<C: ColumnTrait>(col: &C) -> Option<&'static str> {
    use crate::entity::ColumnTypeTrait;
    let def = col.def();
    def.get_enum_name()?;
    Some(match def.get_column_type() {
        pgorm_query::ColumnType::Array(_) => "text[]",
        _ => "text",
    })
}

/// THE one projection writer: every decoded source of a graph — and of the
/// pipeline's source-select terminal — passes through here.
///
/// Appends, for every variant of `F`'s `Column` in iteration order,
/// `col.select_as(Expr::col((qualifier, col)))` aliased as
/// `s{index}_{col}` ([`source_column_alias`]) — the same enum-cast
/// discipline as `Select<E>`'s default list, with the alias being the plain
/// SQL column name under the prefix, whatever the cast wrapped.
///
/// `qualifier` is the source's *effective* identifier: the bound alias when
/// the source was declared under one, otherwise its bare table — the same
/// identifier the `ON` clause constrains against, so the projection and the
/// join cannot name one source two ways.
// [spec:pgorm:sem:query.graph.writer]
pub(crate) fn project_source<F: EntityTrait>(
    query: &mut SelectStatement,
    qualifier: DynIden,
    index: usize,
) {
    for col in <F::Column as Iterable>::iter() {
        let alias = source_column_alias(index, col.as_str());
        let expr = Expr::col((SharedIden::clone(&qualifier), col.into_iden()));
        query.expr(SelectExpr::new_as(
            col.select_as(expr),
            SharedIden::new(Alias::new(alias)),
        ));
    }
}

/// A relational read declared as a root entity plus joined, decoded sources.
///
/// `S` is the tuple of declared slots, in join order; each edge method
/// appends its slot by returning `SelectGraph<E, (…, NewSlot)>`, so the
/// declared tuple, the emitted joins and the decoded row type are one fact
/// stated once. Construct with [`EntityTrait::graph`](crate::EntityTrait::graph),
/// [`SelectGraph::new`] or [`Default`]; the root's columns are projected
/// under `s0_` from the moment the value exists.
///
/// ```
/// use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
///
/// assert_eq!(
///     cake::Entity::graph()
///         .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
///         .as_query()
///         .to_string(),
///     [
///         r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
///         r#""fruit"."id" AS "s1_id", "fruit"."name" AS "s1_name", "fruit"."cake_id" AS "s1_cake_id""#,
///         r#"FROM "cake""#,
///         r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
///     ]
///     .join(" ")
/// );
/// ```
///
/// The projection is generated, never edited: the graph does not implement
/// [`QuerySelect`](crate::QuerySelect), so there is no `column`,
/// `column_as`, `expr`, `select` or `select_only` to disagree with the
/// decode.
///
/// ```compile_fail,E0599
/// use pgorm::{entity::*, query::*, tests_cfg::cake};
///
/// // The graph's projection is generated, never edited.
/// let _ = cake::Entity::graph().column(cake::Column::Name);
/// ```
///
/// Neither can the decode be told a shape the declaration does not produce:
/// the row type is computed from `S`.
///
/// ```compile_fail,E0308
/// use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit, vendor}};
/// use pgorm::query::{Opt, SelectGraph};
///
/// // The graph joined `fruit`, so its slot list is `(Opt<fruit::Entity>,)`;
/// // ascribing a vendor slot is a type error, not a runtime decode surprise.
/// let _: SelectGraph<cake::Entity, (Opt<vendor::Entity>,)> = cake::Entity::graph()
///     .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def());
/// ```
// [spec:pgorm:def:query.graph]
pub struct SelectGraph<E: EntityTrait, S = ()> {
    pub(crate) query: SelectStatement,
    /// The decoded sources' effective identifiers, in declaration order: the
    /// root at index 0, each slot at its own. It is the writer's prefix index
    /// and the cursor's tiebreak qualifier, held once so the two cannot
    /// disagree about what a source is called.
    pub(crate) qualifiers: Vec<DynIden>,
    pub(crate) marker: PhantomData<(E, S)>,
}

impl<E: EntityTrait, S> fmt::Debug for SelectGraph<E, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SelectGraph")
            .field("query", &self.query)
            .field("qualifiers", &self.qualifiers)
            .finish()
    }
}

impl<E: EntityTrait, S> Clone for SelectGraph<E, S> {
    fn clone(&self) -> Self {
        SelectGraph {
            query: self.query.clone(),
            qualifiers: self.qualifiers.clone(),
            marker: PhantomData,
        }
    }
}

// [spec:pgorm:def:query.graph]
impl<E: EntityTrait> SelectGraph<E, ()> {
    /// Start a graph at its root entity, selected `FROM` its table and
    /// projected under the `s0_` prefix.
    ///
    /// The same construction [`EntityTrait::graph`](crate::EntityTrait::graph)
    /// and [`Default`] perform.
    pub fn new() -> Self {
        let mut query = SelectStatement::new();
        query.from(E::default().table_ref());
        let mut graph = SelectGraph {
            query,
            qualifiers: Vec::new(),
            marker: PhantomData,
        };
        graph.project::<E>(SharedIden::new(E::default()));
        graph
    }
}

// [spec:pgorm:def:query.graph]
impl<E: EntityTrait> Default for SelectGraph<E, ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EntityTrait, S> SelectGraph<E, S> {
    /// Project one decoded source under the next prefix, and record the
    /// identifier that prefix belongs to.
    // [spec:pgorm:sem:query.graph.writer]
    pub(crate) fn project<F: EntityTrait>(&mut self, qualifier: DynIden) {
        project_source::<F>(
            &mut self.query,
            SharedIden::clone(&qualifier),
            self.qualifiers.len(),
        );
        self.qualifiers.push(qualifier);
    }

    /// The declared slots' effective identifiers, in declaration order — the
    /// root's excluded, so slot `n` sits at index `n - 1`.
    // [spec:pgorm:sem:query.graph.cursor]
    pub(crate) fn slot_qualifiers(&self) -> &[DynIden] {
        self.qualifiers.get(1..).unwrap_or_default()
    }

    /// The effective identifier of one decoded source: the root at 0, each
    /// slot at its declared position.
    // [spec:pgorm:sem:query.graph.cursor]
    pub(crate) fn qualifier(&self, index: usize) -> Option<DynIden> {
        self.qualifiers.get(index).cloned()
    }

    /// Join a hop that is never decoded — a junction table, a hop of a chain.
    ///
    /// `rel.to_tbl` is `LEFT JOIN`ed under the relation's own condition
    /// (`on_condition` and `condition_type` included), but the hop
    /// contributes nothing to the projection or the decode tuple and
    /// consumes no prefix index: joined because the path runs through it,
    /// invisible because nobody asked to read it.
    ///
    /// `LEFT` so that a missing middle cannot erase root rows by itself; a
    /// [`Req`] slot joined through it re-tightens the chain, because the
    /// `INNER` join's `ON` references the middle's columns and NULLs do not
    /// satisfy it.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::{cake, cake_filling, filling}};
    ///
    /// assert_eq!(
    ///     cake::Entity::graph()
    ///         .via(cake_filling::Relation::Cake.def().rev())
    ///         .join_maybe::<filling::Entity>(cake_filling::Relation::Filling.def())
    ///         .as_query()
    ///         .to_string()
    ///         .contains(r#"LEFT JOIN "cake_filling" ON "cake"."id" = "cake_filling"."cake_id""#),
    ///     true
    /// );
    /// ```
    // [spec:pgorm:def:query.graph]
    pub fn via(mut self, rel: RelationDef) -> Self {
        let to_tbl = rel.to_tbl.clone();
        let condition = join_condition(rel);
        self.query.join(JoinType::LeftJoin, to_tbl, condition);
        self
    }

    /// THE one edge walker: bind the joined side to an alias if asked, join
    /// it with the relation's own condition — plus any call-site `ON`
    /// predicate, ANDed in rather than replacing — then project the joined
    /// side under the next prefix.
    // [spec:pgorm:req:query.graph.aliases]
    fn slot_edge<F: EntityTrait>(
        &mut self,
        join: JoinType,
        mut rel: RelationDef,
        alias: Option<DynIden>,
        extra: Option<OnCondition>,
    ) {
        if let Some(alias) = alias {
            rel.to_tbl = rel.to_tbl.alias(alias);
        }
        let left = SharedIden::clone(rel.from_tbl.qualifier());
        let qualifier = SharedIden::clone(rel.to_tbl.qualifier());
        let to_tbl = rel.to_tbl.clone();
        let mut condition = join_condition(rel);
        if let Some(extra) = extra {
            condition = Condition::all()
                .add(condition)
                .add(extra(left, SharedIden::clone(&qualifier)));
        }
        self.query.join(join, to_tbl, condition);
        self.project::<F>(qualifier);
    }

    /// Re-type the graph after an edge has been written into the statement.
    fn retype<S2>(self) -> SelectGraph<E, S2> {
        SelectGraph {
            query: self.query,
            qualifiers: self.qualifiers,
            marker: PhantomData,
        }
    }
}

/// Generate the edge methods for one slot-tuple arity.
///
/// Each invocation takes the *existing* slot list and produces the methods
/// that append one more slot to it. The bound on decoded sources is the set
/// of arities generated here — a seventh has no receiver impl, and raising
/// the ceiling is one more entry in the invocation below.
macro_rules! grow {
    ( $( ( $( $prev:ident ),* ) ),+ $(,)? ) => {$(
        // [spec:pgorm:sem:query.graph.slots]
        impl<E: EntityTrait $(, $prev: Slot )*> SelectGraph<E, ( $( $prev, )* )> {
            /// `LEFT JOIN` a source that may be absent, decoded as
            /// `Option<F::Model>` through the absence witness.
            ///
            /// `rel.to_tbl` is joined under the relation's own condition,
            /// `on_condition` and `condition_type` included.
            // [spec:pgorm:sem:query.graph.slots]
            pub fn join_maybe<F: EntityTrait>(
                mut self,
                rel: RelationDef,
            ) -> SelectGraph<E, ( $( $prev, )* Opt<F>, )> {
                self.slot_edge::<F>(<Opt<F> as Slot>::JOIN, rel, None, None);
                self.retype()
            }

            /// [`join_maybe`](Self::join_maybe) with the joined side re-bound
            /// to a caller-supplied alias, which is how the same table enters
            /// a graph twice.
            ///
            /// The alias is then the slot's one identifier everywhere: the
            /// `ON` condition's right side, the projection qualifier, a
            /// cursor tiebreak's qualifier. Distinctness is not checked
            /// client-side — joining a table the graph already names,
            /// unaliased, renders SQL PostgreSQL refuses.
            // [spec:pgorm:req:query.graph.aliases]
            pub fn join_maybe_as<F: EntityTrait>(
                mut self,
                rel: RelationDef,
                alias: impl IntoIden,
            ) -> SelectGraph<E, ( $( $prev, )* Opt<F>, )> {
                self.slot_edge::<F>(<Opt<F> as Slot>::JOIN, rel, Some(alias.into_iden()), None);
                self.retype()
            }

            /// [`join_maybe`](Self::join_maybe) with an extra `ON` predicate
            /// ANDed in *in addition to* whatever `on_condition` the relation
            /// already carries.
            ///
            /// Where [`RelationDef::on_condition`] replaces, this composes, so
            /// a call-site narrowing cannot silently drop an authored
            /// predicate. `ON` versus `WHERE` is the point of its existence:
            /// under a `LEFT JOIN` a predicate in `ON` narrows which rows
            /// *match* — unmatched roots survive, decoding `None` — while the
            /// same predicate through [`filter`](crate::QueryFilter::filter)
            /// lands in `WHERE`, where an unmatched row's NULLs fail it and
            /// the join silently tightens to `INNER`.
            ///
            /// The closure receives the join's left and right identifiers;
            /// qualify from them rather than hardcoding a table name, or an
            /// aliased slot's predicate will name a table that is not in the
            /// query. There is no `join_one_filtered`: under an `INNER JOIN`
            /// the two placements select the same rows, and `filter` already
            /// spells it.
            // [spec:pgorm:req:query.graph.aliases]
            pub fn join_maybe_filtered<F, C>(
                mut self,
                rel: RelationDef,
                f: C,
            ) -> SelectGraph<E, ( $( $prev, )* Opt<F>, )>
            where
                F: EntityTrait,
                C: Fn(DynIden, DynIden) -> Condition + Send + Sync + 'static,
            {
                self.slot_edge::<F>(<Opt<F> as Slot>::JOIN, rel, None, Some(Box::new(f)));
                self.retype()
            }

            /// `INNER JOIN` a source that must match, decoded as a bare
            /// `F::Model` — no `Option` to unwrap for a join that cannot miss.
            // [spec:pgorm:sem:query.graph.slots]
            pub fn join_one<F: EntityTrait>(
                mut self,
                rel: RelationDef,
            ) -> SelectGraph<E, ( $( $prev, )* Req<F>, )> {
                self.slot_edge::<F>(<Req<F> as Slot>::JOIN, rel, None, None);
                self.retype()
            }

            /// [`join_one`](Self::join_one) with the joined side re-bound to a
            /// caller-supplied alias, on the terms
            /// [`join_maybe_as`](Self::join_maybe_as) states.
            // [spec:pgorm:req:query.graph.aliases]
            pub fn join_one_as<F: EntityTrait>(
                mut self,
                rel: RelationDef,
                alias: impl IntoIden,
            ) -> SelectGraph<E, ( $( $prev, )* Req<F>, )> {
                self.slot_edge::<F>(<Req<F> as Slot>::JOIN, rel, Some(alias.into_iden()), None);
                self.retype()
            }

            /// Fold the path [`Related<F>`] already describes into the graph
            /// as an optional source.
            ///
            /// When [`Related::via`] is `Some`, that junction relation is
            /// [`via`](Self::via)ed first and then [`Related::to`] is
            /// [`join_maybe`](Self::join_maybe)d — one call for the shape
            /// `find_also_related` used to spell, junction hop included.
            // [spec:pgorm:sem:query.graph.slots]
            pub fn related_maybe<F: EntityTrait>(
                self,
            ) -> SelectGraph<E, ( $( $prev, )* Opt<F>, )>
            where
                E: Related<F>,
            {
                let graph = match <E as Related<F>>::via() {
                    Some(via) => self.via(via),
                    None => self,
                };
                graph.join_maybe::<F>(<E as Related<F>>::to())
            }
        }
    )+};
}

grow!(
    (),
    (S1),
    (S1, S2),
    (S1, S2, S3),
    (S1, S2, S3, S4),
    (S1, S2, S3, S4, S5),
);

// [spec:pgorm:def:query.graph]
impl<E: EntityTrait, S> QueryFilter for SelectGraph<E, S> {
    type QueryStatement = SelectStatement;

    fn query(&mut self) -> &mut SelectStatement {
        &mut self.query
    }
}

// [spec:pgorm:def:query.graph]
impl<E: EntityTrait, S> QueryOrder for SelectGraph<E, S> {
    type QueryStatement = SelectStatement;

    fn query(&mut self) -> &mut SelectStatement {
        &mut self.query
    }
}

// [spec:pgorm:def:query.graph]
impl<E: EntityTrait, S> QueryTrait for SelectGraph<E, S> {
    type QueryStatement = SelectStatement;

    fn query(&mut self) -> &mut SelectStatement {
        &mut self.query
    }

    fn as_query(&self) -> &SelectStatement {
        &self.query
    }

    fn into_query(self) -> SelectStatement {
        self.query
    }
}

// [spec:pgorm:def:query.graph/test]    construction projects the root under
// `s0_` and selects from its table, `via` joins without consuming a prefix,
// and the filter / order / query traits reach the same statement
// [spec:pgorm:sem:query.graph.slots/test]    the slot kind fixes the join
// type, and the declared tuple grows to the generated ceiling
// [spec:pgorm:sem:query.graph.writer/test]    one prefixed block per decoded
// source, in declaration order, under the source's effective identifier
// [spec:pgorm:req:query.graph.aliases/test]    an `_as` slot is named by its
// alias everywhere, and `join_maybe_filtered` composes with the relation's
// own `on_condition` instead of replacing it
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_cfg::{cake, cake_filling, filling, fruit, lunch_set, vendor};
    use crate::{ColumnTrait, QueryFilter, RelationTrait};
    use pgorm_query::{Expr, IntoCondition, alias};
    use pretty_assertions::assert_eq;

    #[track_caller]
    fn assert_renders(actual: String, parts: &[&str]) {
        assert_eq!(actual, parts.join(" "));
    }

    #[test]
    fn root_is_projected_at_construction() {
        assert_eq!(
            cake::Entity::graph().as_query().to_string(),
            r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name" FROM "cake""#
        );
        assert_eq!(
            SelectGraph::<cake::Entity>::default()
                .as_query()
                .to_string(),
            SelectGraph::<cake::Entity>::new().as_query().to_string()
        );
    }

    #[test]
    fn enum_columns_keep_cast_and_plain_alias() {
        assert_renders(
            lunch_set::Entity::graph().as_query().to_string(),
            &[
                r#"SELECT "lunch_set"."id" AS "s0_id", "lunch_set"."name" AS "s0_name","#,
                r#"CAST("lunch_set"."tea" AS text) AS "s0_tea" FROM "lunch_set""#,
            ],
        );
    }

    #[test]
    fn three_sources_one_writer() {
        assert_renders(
            cake::Entity::graph()
                .via(cake_filling::Relation::Cake.def().rev())
                .join_maybe::<filling::Entity>(cake_filling::Relation::Filling.def())
                .join_maybe::<vendor::Entity>(filling::Relation::Vendor.def())
                .as_query()
                .to_string(),
            &[
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
                r#""filling"."id" AS "s1_id", "filling"."name" AS "s1_name", "filling"."vendor_id" AS "s1_vendor_id","#,
                r#""vendor"."id" AS "s2_id", "vendor"."name" AS "s2_name""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "cake_filling" ON "cake"."id" = "cake_filling"."cake_id""#,
                r#"LEFT JOIN "filling" ON "cake_filling"."filling_id" = "filling"."id""#,
                r#"LEFT JOIN "vendor" ON "filling"."vendor_id" = "vendor"."id""#,
            ],
        );
    }

    #[test]
    fn required_slot_is_inner() {
        assert_renders(
            cake::Entity::graph()
                .join_one::<fruit::Entity>(cake::Relation::Fruit.def())
                .as_query()
                .to_string(),
            &[
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
                r#""fruit"."id" AS "s1_id", "fruit"."name" AS "s1_name", "fruit"."cake_id" AS "s1_cake_id""#,
                r#"FROM "cake""#,
                r#"INNER JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
            ],
        );
    }

    #[test]
    fn related_maybe_walks_the_junction() {
        assert_renders(
            cake::Entity::graph()
                .related_maybe::<filling::Entity>()
                .as_query()
                .to_string(),
            &[
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
                r#""filling"."id" AS "s1_id", "filling"."name" AS "s1_name", "filling"."vendor_id" AS "s1_vendor_id""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "cake_filling" ON "cake"."id" = "cake_filling"."cake_id""#,
                r#"LEFT JOIN "filling" ON "cake_filling"."filling_id" = "filling"."id""#,
            ],
        );
    }

    #[test]
    fn related_maybe_without_a_junction_is_one_join() {
        assert_eq!(
            cake::Entity::graph()
                .related_maybe::<fruit::Entity>()
                .as_query()
                .to_string(),
            cake::Entity::graph()
                .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
                .as_query()
                .to_string()
        );
    }

    #[test]
    fn same_entity_twice_with_call_site_on() {
        // The canned `TropicalFruit` relation cannot be used here: its
        // authored `on_condition` hard-qualifies `"fruit"` instead of using
        // its `right` parameter, so under an alias it names the wrong table.
        assert_renders(
            cake::Entity::graph()
                .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
                .join_maybe_as::<fruit::Entity>(
                    cake::Relation::Fruit.def().on_condition(|_left, right| {
                        Expr::col((right, fruit::Column::Name))
                            .like("%tropical%")
                            .into_condition()
                    }),
                    alias("tropical"),
                )
                .as_query()
                .to_string(),
            &[
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
                r#""fruit"."id" AS "s1_id", "fruit"."name" AS "s1_name", "fruit"."cake_id" AS "s1_cake_id","#,
                r#""tropical"."id" AS "s2_id", "tropical"."name" AS "s2_name", "tropical"."cake_id" AS "s2_cake_id""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
                r#"LEFT JOIN "fruit" AS "tropical" ON "cake"."id" = "tropical"."cake_id" AND "tropical"."name" LIKE '%tropical%'"#,
            ],
        );
    }

    #[test]
    fn required_slot_takes_an_alias_too() {
        assert_renders(
            cake::Entity::graph()
                .join_one_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("topping"))
                .as_query()
                .to_string(),
            &[
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
                r#""topping"."id" AS "s1_id", "topping"."name" AS "s1_name", "topping"."cake_id" AS "s1_cake_id""#,
                r#"FROM "cake""#,
                r#"INNER JOIN "fruit" AS "topping" ON "cake"."id" = "topping"."cake_id""#,
            ],
        );
    }

    #[test]
    fn filtered_join_composes_with_the_authored_condition() {
        // `TropicalFruit` already carries an authored `on_condition`; the
        // call-site predicate is ANDed in beside it, not over it.
        let sql = cake::Entity::graph()
            .join_maybe_filtered::<fruit::Entity, _>(
                cake::Relation::TropicalFruit.def(),
                |_left, right| {
                    Expr::col((right, fruit::Column::Id))
                        .gt(10)
                        .into_condition()
                },
            )
            .as_query()
            .to_string();

        assert!(
            sql.contains(r#""fruit"."name" LIKE '%tropical%'"#),
            "the authored predicate survives: {sql}"
        );
        assert!(
            sql.contains(r#""fruit"."id" > 10"#),
            "the call-site predicate is added: {sql}"
        );
        assert!(sql.contains("LEFT JOIN"), "the slot stays optional: {sql}");
    }

    #[test]
    fn filtered_join_lands_in_on_not_where() {
        let sql = cake::Entity::graph()
            .join_maybe_filtered::<fruit::Entity, _>(cake::Relation::Fruit.def(), |_left, right| {
                Expr::col((right, fruit::Column::Name))
                    .like("%berry%")
                    .into_condition()
            })
            .as_query()
            .to_string();

        assert!(!sql.contains("WHERE"), "no WHERE clause is written: {sql}");
        assert_eq!(
            sql,
            [
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
                r#""fruit"."id" AS "s1_id", "fruit"."name" AS "s1_name", "fruit"."cake_id" AS "s1_cake_id""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id" AND "fruit"."name" LIKE '%berry%'"#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn filter_and_order_reach_the_statement() {
        assert_renders(
            cake::Entity::graph()
                .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
                .filter(cake::Column::Id.gt(1))
                .order_by_asc(cake::Column::Id)
                .as_query()
                .to_string(),
            &[
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name","#,
                r#""fruit"."id" AS "s1_id", "fruit"."name" AS "s1_name", "fruit"."cake_id" AS "s1_cake_id""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
                r#"WHERE "cake"."id" > 1"#,
                r#"ORDER BY "cake"."id" ASC"#,
            ],
        );
    }

    #[test]
    fn six_slots_are_declarable() {
        let graph = cake::Entity::graph()
            .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
            .join_maybe_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("f2"))
            .join_maybe_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("f3"))
            .join_maybe_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("f4"))
            .join_maybe_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("f5"))
            .join_maybe_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("f6"));

        assert_eq!(graph.qualifiers.len(), 7);
        let sql = graph.as_query().to_string();
        assert!(sql.contains(r#""f6"."cake_id" AS "s6_cake_id""#), "{sql}");
    }

    #[test]
    fn via_hops_consume_no_prefix() {
        let graph = cake::Entity::graph()
            .via(cake_filling::Relation::Cake.def().rev())
            .via(cake_filling::Relation::Filling.def());

        assert_eq!(graph.qualifiers.len(), 1);
        assert_renders(
            graph.as_query().to_string(),
            &[
                r#"SELECT "cake"."id" AS "s0_id", "cake"."name" AS "s0_name""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "cake_filling" ON "cake"."id" = "cake_filling"."cake_id""#,
                r#"LEFT JOIN "filling" ON "cake_filling"."filling_id" = "filling"."id""#,
            ],
        );
    }
}
