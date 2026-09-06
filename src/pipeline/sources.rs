//! The model-decode terminal: the pipeline's sources restated, projected by
//! the graph's writer discipline, decoded as `Option<Model>` per source.
//!
//! Where [`into_model`](Pipeline::into_model) asks the caller for a row type
//! whose projection the caller must have arranged,
//! [`select_sources`](Pipeline::select_sources) takes the relations
//! themselves and arranges the projection: one `s{i}_`-prefixed block per
//! listed source, written before compilation so prqlc never has to invent
//! `_expr_N` names the decode could not predict.

use core::marker::PhantomData;
use std::fmt;

use pgorm_query::{Iden, Values};

use crate::query::graph::{source_column_alias, source_read_cast};
use crate::{
    ConnectionTrait, EntityTrait, Error, FromQueryResult, IdenStr, Iterable, QueryResult,
    SelectorRaw, SelectorTrait,
};

use super::adapter::{self, PlExpr};
use super::builder::{IntoSource, Pipeline, Source};
use super::error::PipelineError;

mod sealed {
    use super::PlExpr;

    /// The element half of the seal: restating a source is meaningful only
    /// for the shapes that carry an entity, so no outside type can claim to.
    pub trait SealedSource {}

    /// The list half of the seal, carrying the projection machinery: the
    /// method traffics in prqlc's PL nodes, which never appear in public
    /// API, so it lives on the unnameable supertrait.
    pub trait SealedList: Sized {
        /// The final projection stage's expressions, one block per source.
        fn projection(self) -> Vec<PlExpr>;
    }
}

/// One relation of a pipeline restated to
/// [`select_sources`](Pipeline::select_sources): an entity for a relation
/// read bare, or the same [`named(..)`](super::IntoSource::named) spelling
/// the join used for a relation read under a name — so two occurrences of
/// one table are told apart exactly as the join told them apart.
///
/// The trait is sealed: only an entity carries the `Model` a source decodes
/// into, so the two spellings above are the whole set.
// [spec:pgorm:sem:pipeline.select-sources]
pub trait SelectableSource: sealed::SealedSource {
    /// The entity whose columns are projected and whose model is decoded.
    type Entity: EntityTrait;

    /// The name the projection qualifies this source's columns by: the
    /// [`named`](super::IntoSource::named) token, or the entity's own table
    /// name (`[spec:pgorm:sem:pipeline.qualify+2]`).
    fn qualifier(&self) -> String;
}

// [spec:pgorm:sem:pipeline.select-sources]
impl<E: EntityTrait> sealed::SealedSource for E {}

/// A relation read bare: qualified by the entity's own name, exactly as its
/// columns are everywhere else in the pipeline.
// [spec:pgorm:sem:pipeline.select-sources]
impl<E: EntityTrait> SelectableSource for E {
    type Entity = E;

    fn qualifier(&self) -> String {
        Iden::to_string(&E::default())
    }
}

// [spec:pgorm:sem:pipeline.select-sources]
impl<S: SelectableSource> sealed::SealedSource for Named<S> {}

/// A relation read under a name: the name is the qualifier, because after
/// [`named`](super::IntoSource::named) it is the relation's only name.
// [spec:pgorm:sem:pipeline.select-sources]
impl<S: SelectableSource> SelectableSource for Named<S> {
    type Entity = S::Entity;

    fn qualifier(&self) -> String {
        self.name.clone()
    }
}

/// Write one source's projection block: every column of `E` in iteration
/// order, qualified by `qualifier`, cast by the read discipline the graph's
/// writer applies ([`source_read_cast`]) and aliased
/// `s{index}_{col}` ([`source_column_alias`]) — the same prefix scheme and
/// cast discipline as [`project_source`](crate::query::graph::project_source),
/// emitted as PL nodes instead of a `SelectStatement`.
// [spec:pgorm:sem:pipeline.select-sources]
// [spec:pgorm:sem:query.graph.writer]
fn project_into<E: EntityTrait>(nodes: &mut Vec<PlExpr>, qualifier: &str, index: usize) {
    for column in <E::Column as Iterable>::iter() {
        let alias = source_column_alias(index, column.as_str());
        let node = adapter::ident_in(vec![qualifier.to_owned()], Iden::to_string(&column));
        let node = match source_read_cast(&column) {
            Some(cast) => adapter::call("as", vec![adapter::ident(cast), node]),
            None => node,
        };
        nodes.push(adapter::aliased(node, alias));
    }
}

/// The argument of [`select_sources`](Pipeline::select_sources): a single
/// [`SelectableSource`] or a tuple of up to six.
///
/// [`Row`](SourceList::Row) is one `Option<Model>` per listed source, in
/// listing order — every position optional, the first included, because the
/// pipeline's joins carry no missability in their types and under a right or
/// full join the *left* side is the absent one.
// [spec:pgorm:sem:pipeline.select-sources]
pub trait SourceList: sealed::SealedList {
    /// What one row decodes into: `Option<Model>` per source, a bare
    /// `Option<Model>` for a single source rather than a one-tuple.
    type Row;

    /// Decode one row, each source through the absence witness
    /// ([`FromQueryResult::from_query_result_optional`]) under its own
    /// `s{i}_` prefix.
    fn decode(res: &QueryResult) -> Result<Self::Row, Error>;
}

// [spec:pgorm:sem:pipeline.select-sources]
impl<S: SelectableSource> sealed::SealedList for S {
    fn projection(self) -> Vec<PlExpr> {
        let mut nodes = Vec::new();
        project_into::<S::Entity>(&mut nodes, &self.qualifier(), 0);
        nodes
    }
}

/// A source listed alone still decodes as `Option<Model>`: listing it does
/// not prove a right-joined pipeline matched it.
// [spec:pgorm:sem:pipeline.select-sources]
impl<S: SelectableSource> SourceList for S {
    type Row = Option<<S::Entity as EntityTrait>::Model>;

    fn decode(res: &QueryResult) -> Result<Self::Row, Error> {
        <S::Entity as EntityTrait>::Model::from_query_result_optional(res, "s0_")
    }
}

/// Generate the list impls for one tuple arity: the projection walks the
/// sources in listing order, the decode reads the same prefixes back.
macro_rules! source_tuple {
    ( $( $s:ident . $idx:tt @ $pre:literal ),+ ) => {
        // [spec:pgorm:sem:pipeline.select-sources]
        impl<$( $s: SelectableSource ),+> sealed::SealedList for ( $( $s, )+ ) {
            fn projection(self) -> Vec<PlExpr> {
                let mut nodes = Vec::new();
                $( project_into::<$s::Entity>(&mut nodes, &self.$idx.qualifier(), $idx); )+
                nodes
            }
        }

        // [spec:pgorm:sem:pipeline.select-sources]
        impl<$( $s: SelectableSource ),+> SourceList for ( $( $s, )+ ) {
            type Row = ( $( Option<<$s::Entity as EntityTrait>::Model>, )+ );

            fn decode(res: &QueryResult) -> Result<Self::Row, Error> {
                Ok(( $(
                    <$s::Entity as EntityTrait>::Model::from_query_result_optional(res, $pre)?,
                )+ ))
            }
        }
    };
}

source_tuple!(S1.0 @ "s0_", S2.1 @ "s1_");
source_tuple!(S1.0 @ "s0_", S2.1 @ "s1_", S3.2 @ "s2_");
source_tuple!(S1.0 @ "s0_", S2.1 @ "s1_", S3.2 @ "s2_", S4.3 @ "s3_");
source_tuple!(S1.0 @ "s0_", S2.1 @ "s1_", S3.2 @ "s2_", S4.3 @ "s3_", S5.4 @ "s4_");
source_tuple!(S1.0 @ "s0_", S2.1 @ "s1_", S3.2 @ "s2_", S4.3 @ "s3_", S5.4 @ "s4_", S6.5 @ "s5_");

/// The selector that decodes one `select_sources` row, staged through
/// [`SelectorRaw`] so execution lands on the ordinary raw-read path.
struct SourcesRow<T>(PhantomData<T>);

// [spec:pgorm:sem:pipeline.select-sources]
impl<T: SourceList> SelectorTrait for SourcesRow<T> {
    type Item = T::Row;

    fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error> {
        T::decode(&res)
    }
}

/// A pipeline whose final projection is decided: sources listed, models to
/// decode. Only the terminals remain.
///
/// On the pattern of [`Grouped`](super::Grouped), no transform can follow —
/// reshaping *after* the selection is unrepresentable rather than merely
/// wrong:
///
/// ```compile_fail,E0599
/// use pgorm::pipeline::{ExprOps, Pipeline};
/// use pgorm::tests_cfg::cake;
///
/// let _ = Pipeline::from(cake::Entity)
///     .select_sources(cake::Entity)
///     .filter(cake::Column::Id.gt(1));
/// ```
///
/// Reshaping *before* the selection is refused at
/// [`into_sql`](SelectedSources::into_sql) with
/// [`PipelineError::ReshapedSources`] naming the offending stage.
// [spec:pgorm:sem:pipeline.select-sources]
pub struct SelectedSources<T> {
    pipeline: Pipeline,
    projection: Vec<PlExpr>,
    marker: PhantomData<T>,
}

impl<T> fmt::Debug for SelectedSources<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SelectedSources")
            .field("pipeline", &self.pipeline)
            .field("projection", &self.projection.len())
            .finish()
    }
}

impl<T> Clone for SelectedSources<T> {
    fn clone(&self) -> Self {
        SelectedSources {
            pipeline: self.pipeline.clone(),
            projection: self.projection.clone(),
            marker: PhantomData,
        }
    }
}

impl Pipeline {
    /// Select the listed sources' entity models: the pipeline's complement
    /// to the source graph, for row-shaped reads the graph excludes — right
    /// and full joins, aggregates beside models, arbitrary composed
    /// relations upstream.
    ///
    /// Each source is an [`IntoSource`](super::IntoSource) some stage of
    /// this pipeline read, restated: an entity for a relation read bare, the
    /// same [`named(..)`](super::IntoSource::named) spelling for a relation
    /// read under a name. Before compilation, one final projection stage is
    /// appended — every column of each source, qualified by the source's
    /// name and aliased `s{i}_{col}` by the same writer discipline as the
    /// graph's — so two sources sharing a column name land under different
    /// prefixes by construction and prqlc never invents `_expr_N` names the
    /// decode cannot predict.
    ///
    /// Every listed source decodes through the absence witness as
    /// `Option<Model>` — every position, the first included: the pipeline's
    /// joins carry no missability in their types, and under a right or full
    /// join the *left* side is the absent one, so the terminal claims
    /// nothing a join could falsify. Callers who can prove a side present
    /// unwrap it; the graph is the surface whose types state it.
    ///
    /// ```
    /// use pgorm::pipeline::{ExprOps, JoinSide, Pipeline};
    /// use pgorm::tests_cfg::{cake, fruit};
    ///
    /// let (sql, _) = Pipeline::from(cake::Entity)
    ///     .join(
    ///         JoinSide::Left,
    ///         fruit::Entity,
    ///         cake::Column::Id.eq(fruit::Column::CakeId),
    ///     )
    ///     .select_sources((cake::Entity, fruit::Entity))
    ///     .into_sql()?;
    /// assert_eq!(
    ///     sql,
    ///     "SELECT cake.id AS s0_id, cake.name AS s0_name, fruit.id AS s1_id, \
    ///      fruit.name AS s1_name, fruit.cake_id AS s1_cake_id FROM cake \
    ///      LEFT OUTER JOIN fruit ON cake.id = fruit.cake_id"
    /// );
    /// # Ok::<_, pgorm::pipeline::PipelineError>(())
    /// ```
    ///
    /// Construction is infallible; a pipeline that already reshaped away its
    /// sources' namespaces (`select`, `group().aggregate()`, `intersect`,
    /// `remove`) is refused at [`into_sql`](SelectedSources::into_sql) with
    /// [`PipelineError::ReshapedSources`] naming the stage. `filter`,
    /// `derive`, `sort`, `take` / `take_range`, `join`, `window`, `distinct`
    /// and `append` leave every source addressable and compose freely ahead.
    ///
    /// A listed source the pipeline never read compiles up to prqlc, which
    /// refuses the unresolvable columns as [`PipelineError::Compile`] — the
    /// terminal checks stage shape, not membership.
    // [spec:pgorm:sem:pipeline.select-sources]
    pub fn select_sources<T: SourceList>(self, sources: T) -> SelectedSources<T> {
        SelectedSources {
            projection: sources.projection(),
            pipeline: self,
            marker: PhantomData,
        }
    }
}

impl<T: SourceList> SelectedSources<T> {
    /// Refuse a reshaped pipeline by the stage that reshaped it, before
    /// prqlc gets to answer with an opaque unresolved-name diagnostic;
    /// otherwise append the projection stage.
    // [spec:pgorm:sem:pipeline.select-sources]
    fn into_pipeline(self) -> Result<Pipeline, PipelineError> {
        let SelectedSources {
            mut pipeline,
            projection,
            ..
        } = self;
        if let Some(stage) = pipeline.reshaped {
            return Err(PipelineError::ReshapedSources(stage));
        }
        pipeline
            .stages
            .push(adapter::call("select", vec![adapter::tuple(projection)]));
        Ok(pipeline)
    }

    /// Compile to PostgreSQL SQL and the values bound along the way, the
    /// projection stage appended last.
    ///
    /// Everything [`Pipeline::into_sql`] does still happens here — the
    /// reserved-alias screen, prqlc's resolution, the placeholder census —
    /// preceded by the reshaped-pipeline refusal.
    // [spec:pgorm:sem:pipeline.select-sources]
    pub fn into_sql(self) -> Result<(String, Values), PipelineError> {
        self.into_pipeline()?.into_sql()
    }

    /// Compile, execute and decode every row as one `Option<Model>` per
    /// listed source.
    // [spec:pgorm:sem:pipeline.select-sources]
    pub async fn all<C>(self, db: &C) -> Result<Vec<T::Row>, Error>
    where
        C: ConnectionTrait,
    {
        let (sql, values) = self.into_sql()?;
        SelectorRaw {
            stmt: sql,
            values,
            selector: SourcesRow::<T>(PhantomData),
        }
        .all(db)
        .await
    }

    /// Compile, execute and decode the first row, failing with
    /// [`Error::RecordNotFound`] when there is none.
    ///
    /// A `take 1` stage is appended to the pipeline before the projection,
    /// as on [`Pipeline::one`], so at most one row leaves the server.
    // [spec:pgorm:sem:pipeline.select-sources]
    pub async fn one<C>(mut self, db: &C) -> Result<T::Row, Error>
    where
        C: ConnectionTrait,
    {
        self.pipeline = self.pipeline.take(1);
        let (sql, values) = self.into_sql()?;
        SelectorRaw {
            stmt: sql,
            values,
            selector: SourcesRow::<T>(PhantomData),
        }
        .one(db)
        .await
    }

    /// Compile, execute and decode the first row, or `None` when there is
    /// none. Appends `take 1` like [`one`](SelectedSources::one).
    // [spec:pgorm:sem:pipeline.select-sources]
    pub async fn one_opt<C>(mut self, db: &C) -> Result<Option<T::Row>, Error>
    where
        C: ConnectionTrait,
    {
        self.pipeline = self.pipeline.take(1);
        let (sql, values) = self.into_sql()?;
        SelectorRaw {
            stmt: sql,
            values,
            selector: SourcesRow::<T>(PhantomData),
        }
        .one_opt(db)
        .await
    }
}

/// A relation read under a caller-chosen name: what
/// [`named`](IntoSource::named) returns.
///
/// As a join or set-operation operand it behaves exactly as the wrapped
/// relation would, aliased — the name replaces the relation's own, as SQL's
/// `AS` does. Unlike an anonymous [`Source`], it keeps the wrapped relation's
/// type, so a `Named` entity can also restate a source to
/// [`select_sources`](Pipeline::select_sources), where the entity supplies
/// the columns and the name qualifies them.
// [spec:pgorm:sem:pipeline.self-join]
// [spec:pgorm:sem:pipeline.select-sources]
#[derive(Debug, Clone)]
pub struct Named<R> {
    pub(super) relation: R,
    pub(super) name: String,
}

/// Naming a relation aliases it on the way in; naming it again replaces the
/// name, as a second `AS` would.
// [spec:pgorm:sem:pipeline.self-join]
impl<R: IntoSource> IntoSource for Named<R> {
    fn into_source(self) -> Source {
        let mut source = self.relation.into_source();
        source.alias = Some(self.name);
        source
    }
}
