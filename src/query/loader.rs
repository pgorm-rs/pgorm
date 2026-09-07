use crate::{
    Condition, ConnectionTrait, EntityName, EntityTrait, Error, Identity, ModelTrait, QueryFilter,
    QueryTrait, Related, RelationDef, RelationType, Req, Select, SelectGraph, error::*,
};
use async_trait::async_trait;
use pgorm_query::{
    AliasName, ColumnRef, DynIden, Expr, FromItem, IntoColumnRef, NamedTable, SharedIden,
    SimpleExpr, TableName, ValueTuple, alias,
};
use std::{collections::HashMap, marker::PhantomData, str::FromStr};

/// Entity, or a `Select<Entity>`; to be used as parameters in [`LoaderTrait`]
pub trait EntityOrSelect<E: EntityTrait>: Send {
    /// The selector, which a bare entity produces with `E::find()`.
    fn into_select(self) -> Select<E>;
}

/// This trait implements the Data Loader API
// [spec:pgorm:req:query.loader+1]
#[async_trait]
pub trait LoaderTrait {
    /// Source model
    type Model: ModelTrait;

    /// Used to eager load has_one relations
    async fn load_one<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Option<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>;

    /// Used to eager load has_many relations
    async fn load_many<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>;

    /// Used to eager load many-to-many relations.
    ///
    /// The junction is the one the relation's `via` already names, so it is
    /// not passed in: there is no second junction to disagree with it.
    async fn load_many_via<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>;
}

impl<E> EntityOrSelect<E> for E
where
    E: EntityTrait,
{
    fn into_select(self) -> Select<E> {
        E::find()
    }
}

impl<E> EntityOrSelect<E> for Select<E>
where
    E: EntityTrait,
{
    fn into_select(self) -> Select<E> {
        self
    }
}

#[async_trait]
impl<M> LoaderTrait for Vec<M>
where
    M: ModelTrait + Sync,
{
    type Model = M;

    async fn load_one<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Option<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        self.as_slice().load_one(stmt, db).await
    }

    async fn load_many<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        self.as_slice().load_many(stmt, db).await
    }

    async fn load_many_via<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        self.as_slice().load_many_via(stmt, db).await
    }
}

// [spec:pgorm:req:query.loader+1]
#[async_trait]
impl<M> LoaderTrait for &[M]
where
    M: ModelTrait + Sync,
{
    type Model = M;

    // [spec:pgorm:sem:query.loader.batching+6]
    // [spec:pgorm:sem:query.loader.regroup+4]
    async fn load_one<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Option<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        // we verify that is HasOne relation
        if <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::via().is_some() {
            return Err(query_err("Relation is ManytoMany instead of HasOne"));
        }
        let rel_def = <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::to();
        if rel_def.rel_type == RelationType::HasMany {
            return Err(query_err("Relation is HasMany instead of HasOne"));
        }

        if self.is_empty() {
            return Ok(Vec::new());
        }

        Ok(load_related(self, stmt.into_select(), rel_def, db)
            .await?
            .into_iter()
            .map(|bucket| bucket.into_iter().next_back())
            .collect())
    }

    // [spec:pgorm:sem:query.loader.batching+6]
    // [spec:pgorm:sem:query.loader.regroup+4]
    async fn load_many<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        // we verify that is HasMany relation

        if <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::via().is_some() {
            return Err(query_err("Relation is ManyToMany instead of HasMany"));
        }
        let rel_def = <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::to();
        if rel_def.rel_type == RelationType::HasOne {
            return Err(query_err("Relation is HasOne instead of HasMany"));
        }

        if self.is_empty() {
            return Ok(Vec::new());
        }

        load_related(self, stmt.into_select(), rel_def, db).await
    }

    // [spec:pgorm:sem:query.loader.many-to-many+3]
    async fn load_many_via<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        let Some(via_rel) =
            <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::via()
        else {
            return Err(query_err("Relation is not ManyToMany"));
        };

        let rel_def = <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::to();
        if rel_def.rel_type != RelationType::HasOne {
            return Err(query_err("Relation to is not HasOne"));
        }

        if self.is_empty() {
            return Ok(Vec::new());
        }

        // The source side of the via relation: the columns the input models
        // are keyed by, and the columns the junction points back at.
        let via_from_col = via_rel.columns.from_identity();

        let keys: Vec<ValueTuple> = self
            .iter()
            .map(|model: &M| extract_key(&via_from_col, model))
            .collect::<Result<_, Error>>()?;

        // One graph read: the caller's target selector is the root, the
        // junction is a hop nobody decodes, and the input entity is the one
        // required slot, which is what carries the key back out of the join.
        // The hop joins LEFT and the slot INNER, which is INNER end to end:
        // the slot's ON references the junction's columns, and NULLs do not
        // satisfy it.
        let graph = join_source::<R, <M as ModelTrait>::Entity>(
            root_graph::<R>(stmt.into_select()).via(rel_def.rev()),
            via_rel,
        );

        collect_buckets(graph, keys, &via_from_col, db).await
    }
}

/// The alias the input entity's table is joined back under by every loader
/// operation. Internal: it is never handed to a caller, who filters against
/// the target entity by its own name.
const LOADER_SOURCE_ALIAS: AliasName = alias("pgorm_loader_src");

/// The identifier the input entity's table is bound to inside a loader read.
// [spec:pgorm:sem:query.loader.batching+6]
fn source_alias() -> DynIden {
    SharedIden::new(LOADER_SOURCE_ALIAS)
}

/// The read every loader operation issues, minus its terminal: the caller's
/// target selector re-rooted as a graph, and the input entity `F` joined back
/// under [`LOADER_SOURCE_ALIAS`] as the single required slot the key is read
/// from.
///
/// The join is written by the graph's one edge walker, so the relation arrives
/// whole — its column pairs, its `on_condition` and its `condition_type`
/// composed by the same `join_condition` every other join goes through. The
/// loader reconstructs no part of a relation and so can drop no part of one.
// [spec:pgorm:sem:query.loader.batching+6]
fn join_source<R, F>(graph: SelectGraph<R, ()>, rel: RelationDef) -> SelectGraph<R, (Req<F>,)>
where
    R: EntityTrait,
    F: EntityTrait,
{
    graph.join_one_as::<F>(rel.rev(), source_alias())
}

/// Run a loader's graph read under the batch key predicate and hand back, per
/// input key in input order, the target models the read attributed to it.
///
/// Each row carries the input entity's own row beside its target, so the key a
/// target is filed under is read back from the source side rather than
/// re-derived from the target — which is what lets a relation whose
/// `condition_type` is `Any` file one target under several keys.
// [spec:pgorm:sem:query.loader.regroup+4]
async fn collect_buckets<R, F, C>(
    graph: SelectGraph<R, (Req<F>,)>,
    keys: Vec<ValueTuple>,
    from_col: &Identity,
    db: &C,
) -> Result<Vec<Vec<R::Model>>, Error>
where
    C: ConnectionTrait,
    R: EntityTrait,
    R::Model: Send + Sync,
    F: EntityTrait,
{
    let src_tbl = FromItem::from(TableName::Table(source_alias()));
    let condition = prepare_condition(&src_tbl, from_col, &keys)?;
    let graph = QueryFilter::filter(graph, condition);

    let mut buckets: HashMap<ValueTuple, Vec<R::Model>> = keys
        .iter()
        .map(|key: &ValueTuple| (key.clone(), Vec::new()))
        .collect();

    for (target, source) in graph.all(db).await? {
        let key = extract_key(from_col, &source)?;
        let bucket = buckets
            .get_mut(&key)
            .ok_or_else(|| unmatched_key_err(&key, &keys, from_col))?;
        bucket.push(target);
    }

    Ok(keys
        .iter()
        .map(|key: &ValueTuple| buckets.get(key).cloned().unwrap_or_default())
        .collect())
}

/// The whole of a direct load: the relation's target checked against what the
/// graph can qualify, the keys read off the input models in input order, and
/// the one graph read regrouped into a bucket per input.
// [spec:pgorm:sem:query.loader.batching+6]
async fn load_related<M, R, C>(
    models: &[M],
    select: Select<R>,
    rel_def: RelationDef,
    db: &C,
) -> Result<Vec<Vec<R::Model>>, Error>
where
    C: ConnectionTrait,
    M: ModelTrait,
    R: EntityTrait,
    R::Model: Send + Sync,
{
    check_target_ref(&rel_def)?;

    let from_col = rel_def.columns.from_identity();
    let keys: Vec<ValueTuple> = models
        .iter()
        .map(|model: &M| extract_key(&from_col, model))
        .collect::<Result<_, Error>>()?;

    let graph = join_source::<R, <M as ModelTrait>::Entity>(root_graph::<R>(select), rel_def);

    collect_buckets(graph, keys, &from_col, db).await
}

/// Re-root the caller's target selector as a graph.
///
/// The statement keeps everything the caller put on it — its FROM, its
/// filters, its ordering, its limit — and gives up only its projection, which
/// the graph's one writer regenerates under `s0_`. That is the whole reason
/// this is not a `Select<R>` conversion: a graph's select list is generated
/// from its declaration, never inherited from a builder a caller may have
/// edited, and clearing before projecting is what makes the two statements
/// the same one.
// [spec:pgorm:sem:query.loader.batching+6]
// [spec:pgorm:sem:query.loader.many-to-many+3]
fn root_graph<R: EntityTrait>(select: Select<R>) -> SelectGraph<R, ()> {
    let mut query = select.into_query();
    query.clear_selects();
    let mut graph = SelectGraph {
        query,
        qualifiers: Vec::new(),
        marker: PhantomData,
    };
    graph.project::<R>(SharedIden::new(R::default()));
    graph
}

fn identity_columns(identity: &Identity) -> String {
    identity
        .clone()
        .into_iter()
        .map(|col| col.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

// [spec:pgorm:sem:query.loader.regroup+4]
fn unmatched_key_err(key: &ValueTuple, input_keys: &[ValueTuple], from_col: &Identity) -> Error {
    let sample = match input_keys.first() {
        Some(sample) => format!("{sample:?}"),
        None => "none".to_owned(),
    };
    query_err(format!(
        "Loader cannot regroup a returned row: the key {key:?} read back from `{from}` equals \
         none of the keys read from the input models (an input key reads as {sample}). The \
         stored row and the input model match in SQL but not as Rust values; check for a width, \
         padding or collation difference between them.",
        from = identity_columns(from_col),
    ))
}

/// Refuse a relation whose target is not the from item the caller's selector
/// selects from.
///
/// The graph roots at the caller's `Select<R>` — `FROM` the entity's own
/// table — while the join condition qualifies the target side by whatever
/// identifier the relation's `to_tbl` carries. An aliased or value-producing
/// target would name a table the statement does not have, so it is reported
/// on the terms [`table_column`] states rather than rendered.
// [spec:pgorm:req:query.loader.table-ref-limitation+3]
fn check_target_ref(rel: &RelationDef) -> Result<(), Error> {
    for col in rel.columns.to_identity().iter() {
        table_column(&rel.to_tbl, col)?;
    }
    Ok(())
}

// [spec:pgorm:sem:query.loader.batching+6]
fn resolve_column<Model>(col: &DynIden) -> Result<<Model::Entity as EntityTrait>::Column, Error>
where
    Model: ModelTrait,
{
    let name = col.to_string();
    <<Model::Entity as EntityTrait>::Column as FromStr>::from_str(&name).map_err(|_| {
        let entity = <Model::Entity as Default>::default();
        query_err(format!(
            "Relation names column `{name}`, which is not a column of `{}`",
            entity.table_name(),
        ))
    })
}

// [spec:pgorm:sem:query.loader.batching+6]
fn extract_key<Model>(target_col: &Identity, model: &Model) -> Result<ValueTuple, Error>
where
    Model: ModelTrait,
{
    let mut values = Vec::with_capacity(target_col.arity());
    for col in target_col.iter() {
        values.push(model.get(resolve_column::<Model>(col)?));
    }
    Ok(ValueTuple::from(values))
}

// [spec:pgorm:sem:query.loader.batching+6]
fn prepare_condition(
    table: &FromItem,
    col: &Identity,
    keys: &[ValueTuple],
) -> Result<Condition, Error> {
    // TODO when value is hashable, retain only unique values
    let keys = keys.to_owned();
    let mut columns = Vec::with_capacity(col.arity());
    for col in col.iter() {
        columns.push(table_column(table, col)?);
    }
    Ok(match columns.as_slice() {
        [column_a] => {
            Condition::all().add(Expr::col(column_a.clone()).is_in(keys.into_iter().flatten()))
        }
        _ => Condition::all()
            .add(Expr::tuple(columns.iter().cloned().map(SimpleExpr::Column)).in_tuples(keys)),
    })
}

// [spec:pgorm:req:query.loader.table-ref-limitation+3]
fn table_column(tbl: &FromItem, col: &DynIden) -> Result<ColumnRef, Error> {
    match tbl.to_owned() {
        FromItem::Table(NamedTable {
            name: TableName::Table(tbl),
            alias: None,
        }) => Ok((tbl, col.clone()).into_column_ref()),
        FromItem::Table(NamedTable {
            name: TableName::SchemaTable(sch, tbl),
            alias: None,
        }) => Ok((sch, tbl, col.clone()).into_column_ref()),
        val => Err(query_err(format!(
            "Loader cannot qualify key column `{}` against table reference {val:?}: only \
             unaliased `FromItem::Table` relation targets are supported",
            col.to_string(),
        ))),
    }
}

// [spec:pgorm:sem:query.loader.batching+6/test]    the one read a direct load
// issues: the caller's selector rooted, the input entity joined back under the
// alias the key predicate qualifies against, and the authored relation carried
// into the `ON` whole — its predicate under either composition
// [spec:pgorm:sem:query.loader.many-to-many+3/test]    the one read the
// junction-mediated load issues: the caller's selector rooted and reprojected
// under the graph's prefixes, the junction joined but never projected, and the
// input entity joined back under the alias the key predicate qualifies against
#[cfg(test)]
mod tests {
    use super::*;
    use crate::RelationTrait;
    use crate::tests_cfg::{cake, filling, fruit};
    use pgorm_query::IntoValueTuple;
    use pretty_assertions::assert_eq;

    /// The statement a loader sends for a two-key batch.
    #[track_caller]
    fn batch_sql<R: EntityTrait, F: EntityTrait>(
        graph: SelectGraph<R, (Req<F>,)>,
        from_col: &Identity,
    ) -> String {
        let src_tbl = FromItem::from(TableName::Table(source_alias()));
        let keys = vec![1i32.into_value_tuple(), 2i32.into_value_tuple()];
        let condition = prepare_condition(&src_tbl, from_col, &keys)
            .expect("a bare table qualifies the key column");
        QueryFilter::filter(graph, condition).as_query().to_string()
    }

    /// The read a direct load of `cake -> fruit` issues, under the relation
    /// named by `rel`.
    fn direct_sql(rel: RelationDef) -> String {
        let from_col = rel.columns.from_identity();
        let graph = join_source::<fruit::Entity, cake::Entity>(
            root_graph::<fruit::Entity>(fruit::Entity::find()),
            rel,
        );
        batch_sql(graph, &from_col)
    }

    #[test]
    fn many_to_many_reads_one_graph() {
        let via_rel = <cake::Entity as Related<filling::Entity>>::via()
            .expect("cake is related to filling through a junction");
        let rel_def = <cake::Entity as Related<filling::Entity>>::to();
        let via_from_col = via_rel.columns.from_identity();

        let graph = join_source::<filling::Entity, cake::Entity>(
            root_graph::<filling::Entity>(filling::Entity::find()).via(rel_def.rev()),
            via_rel,
        );

        assert_eq!(
            batch_sql(graph, &via_from_col),
            [
                r#"SELECT "filling"."id" AS "s0_id", "filling"."name" AS "s0_name","#,
                r#""filling"."vendor_id" AS "s0_vendor_id","#,
                r#""pgorm_loader_src"."id" AS "s1_id", "pgorm_loader_src"."name" AS "s1_name""#,
                r#"FROM "filling""#,
                r#"LEFT JOIN "cake_filling" ON "filling"."id" = "cake_filling"."filling_id""#,
                r#"INNER JOIN "cake" AS "pgorm_loader_src""#,
                r#"ON "cake_filling"."cake_id" = "pgorm_loader_src"."id""#,
                r#"WHERE "pgorm_loader_src"."id" IN (1, 2)"#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn direct_read_joins_the_input_entity_back() {
        assert_eq!(
            direct_sql(cake::Relation::Fruit.def()),
            [
                r#"SELECT "fruit"."id" AS "s0_id", "fruit"."name" AS "s0_name","#,
                r#""fruit"."cake_id" AS "s0_cake_id","#,
                r#""pgorm_loader_src"."id" AS "s1_id", "pgorm_loader_src"."name" AS "s1_name""#,
                r#"FROM "fruit""#,
                r#"INNER JOIN "cake" AS "pgorm_loader_src""#,
                r#"ON "fruit"."cake_id" = "pgorm_loader_src"."id""#,
                r#"WHERE "pgorm_loader_src"."id" IN (1, 2)"#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn direct_read_carries_the_authored_predicate() {
        let sql = direct_sql(cake::Relation::TropicalFruit.def());

        assert!(
            sql.contains(
                r#"ON "fruit"."cake_id" = "pgorm_loader_src"."id" AND "fruit"."name" LIKE '%tropical%'"#
            ),
            "{sql}"
        );
    }

    #[test]
    fn direct_read_carries_any_composition() {
        let sql = direct_sql(cake::Relation::OrTropicalFruit.def());

        assert!(
            sql.contains(
                r#"ON "fruit"."cake_id" = "pgorm_loader_src"."id" OR "fruit"."name" LIKE '%tropical%'"#
            ),
            "{sql}"
        );
    }

    #[test]
    fn caller_clauses_survive_the_reroot() {
        use crate::{ColumnTrait, QueryOrder};

        let sql = root_graph::<filling::Entity>(
            filling::Entity::find()
                .filter(filling::Column::Name.like("Ch%"))
                .order_by_desc(filling::Column::Id),
        )
        .as_query()
        .to_string();

        assert_eq!(
            sql,
            [
                r#"SELECT "filling"."id" AS "s0_id", "filling"."name" AS "s0_name","#,
                r#""filling"."vendor_id" AS "s0_vendor_id""#,
                r#"FROM "filling""#,
                r#"WHERE "filling"."name" LIKE 'Ch%'"#,
                r#"ORDER BY "filling"."id" DESC"#,
            ]
            .join(" ")
        );
    }
}
