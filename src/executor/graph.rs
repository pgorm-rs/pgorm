//! Decoding and terminating an N-ary relational read.
//!
//! [`GraphRow<E, S>`] is the [`SelectorTrait`] a [`SelectGraph<E, S>`]
//! converts into — the N-ary generalization of what `SelectTwoModel` was for
//! the pair — and the terminals are that conversion plus the ordinary
//! [`Selector`] machinery. The graph adds a declaration layer, not a second
//! execution path.

use core::marker::PhantomData;
use std::fmt;
use std::num::NonZeroU64;

use crate::{
    ConnectionTrait, EntityTrait, Error, FromQueryResult, Paginator, PaginatorTrait,
    PinBoxSendStream, QueryResult, SelectGraph, Selector, SelectorTrait, Slot,
};

/// The selector that decodes one graph row: the root under `s0_`, then each
/// slot in declaration order under its own `s{i}_` prefix.
///
/// The item is `(E::Model, S1::Out, …, Sn::Out)`, computed from the declared
/// slot tuple; a slotless graph decodes as a bare `E::Model`, not a
/// one-tuple. `Req` slots decode through
/// [`FromQueryResult::from_query_result`], `Opt` slots through
/// [`FromQueryResult::from_query_result_optional`] — the absence witness,
/// unchanged, of which the graph is the N-ary consumer.
///
/// The type is nameable because the cursor and paginator signatures need it;
/// it carries no data and is not constructed by callers.
// [spec:pgorm:sem:query.graph.decode]
pub struct GraphRow<E, S>(PhantomData<(E, S)>);

impl<E, S> fmt::Debug for GraphRow<E, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GraphRow")
    }
}

impl<E, S> Clone for GraphRow<E, S> {
    fn clone(&self) -> Self {
        GraphRow(PhantomData)
    }
}

impl<E, S> GraphRow<E, S> {
    /// The witness value a [`Selector`] carries; it holds nothing.
    pub(crate) fn new() -> Self {
        GraphRow(PhantomData)
    }
}

/// A slotless graph is a single-entity read: one model, no tuple.
// [spec:pgorm:sem:query.graph.decode]
impl<E: EntityTrait> SelectorTrait for GraphRow<E, ()> {
    type Item = E::Model;

    fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error> {
        E::Model::from_query_result(&res, ROOT_PREFIX)
    }
}

/// The prefix the writer aliases the root's columns under.
const ROOT_PREFIX: &str = "s0_";

/// Generate the row decode for one slot-tuple arity.
///
/// The `?` on each source is the abort: the first failing source's error is
/// the row's error, and later sources are not examined.
macro_rules! graph_row {
    ( $( $s:ident @ $pre:literal ),+ ) => {
        // [spec:pgorm:sem:query.graph.decode]
        impl<E: EntityTrait, $( $s: Slot ),+> SelectorTrait for GraphRow<E, ( $( $s, )+ )> {
            type Item = (E::Model, $( $s::Out ),+ );

            fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error> {
                Ok((
                    E::Model::from_query_result(&res, ROOT_PREFIX)?,
                    $( $s::decode(&res, $pre)? ),+
                ))
            }
        }
    };
}

graph_row!(S1 @ "s1_");
graph_row!(S1 @ "s1_", S2 @ "s2_");
graph_row!(S1 @ "s1_", S2 @ "s2_", S3 @ "s3_");
graph_row!(S1 @ "s1_", S2 @ "s2_", S3 @ "s3_", S4 @ "s4_");
graph_row!(S1 @ "s1_", S2 @ "s2_", S3 @ "s3_", S4 @ "s4_", S5 @ "s5_");
graph_row!(S1 @ "s1_", S2 @ "s2_", S3 @ "s3_", S4 @ "s4_", S5 @ "s5_", S6 @ "s6_");

/// What one row of a graph decodes into.
pub type GraphItem<E, S> = <GraphRow<E, S> as SelectorTrait>::Item;

// [spec:pgorm:sem:query.graph.terminals]
impl<E: EntityTrait, S> SelectGraph<E, S>
where
    GraphRow<E, S>: SelectorTrait,
{
    /// Convert into the [`Selector`] every terminal runs through.
    ///
    /// Everything past this conversion is machinery specified elsewhere; the
    /// graph duplicates none of it.
    // [spec:pgorm:sem:query.graph.terminals]
    pub(crate) fn into_selector(self) -> Selector<GraphRow<E, S>> {
        Selector {
            query: self.query,
            selector: GraphRow::new(),
        }
    }

    /// Fetch every row as the declared tuple.
    ///
    /// A decode failure aborts the read at the first bad row, as on every
    /// other model-typed `all`.
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::{cake, fruit}, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let rows: Vec<(cake::Model, Option<fruit::Model>)> = cake::Entity::graph()
    ///     .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:sem:query.graph.terminals]
    pub async fn all<C: ConnectionTrait>(self, db: &C) -> Result<Vec<GraphItem<E, S>>, Error> {
        self.into_selector().all(db).await
    }

    /// Fetch the first row, or `None` when the query returns none.
    ///
    /// `LIMIT 1` is injected, as on [`Selector::one_opt`]. There is no `one`:
    /// a graph row is a join product, so "exactly one" is a claim about the
    /// product rather than about the root.
    // [spec:pgorm:sem:query.graph.terminals]
    pub async fn one_opt<C: ConnectionTrait>(
        self,
        db: &C,
    ) -> Result<Option<GraphItem<E, S>>, Error> {
        self.into_selector().one_opt(db).await
    }

    /// Stream rows as declared tuples, decoding lazily per item.
    ///
    /// A row that fails to decode is yielded as one `Err` item and the stream
    /// continues, as on every other model-typed stream.
    // [spec:pgorm:sem:query.graph.terminals]
    pub async fn stream<'b, C: ConnectionTrait>(
        self,
        db: &C,
    ) -> Result<PinBoxSendStream<'b, Result<GraphItem<E, S>, Error>>, Error>
    where
        GraphRow<E, S>: 'b,
    {
        self.into_selector().stream(db).await
    }
}

/// Pagination reaches the graph through the same selector: page boundaries
/// fall between *rows*, not between root models, so a root with several
/// matching slot rows spans pages exactly as the underlying SQL does.
// [spec:pgorm:sem:query.graph.terminals]
impl<'db, C, E, S> PaginatorTrait<'db, C> for SelectGraph<E, S>
where
    C: ConnectionTrait,
    E: EntityTrait,
    GraphRow<E, S>: SelectorTrait + Send + Sync + 'db,
{
    type Selector = GraphRow<E, S>;

    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, Self::Selector> {
        self.into_selector().paginate(db, page_size)
    }
}
