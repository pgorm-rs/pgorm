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

    // [spec:pgorm:sem:query.loader.regroup+3]
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

        let from_col = rel_def.columns.from_identity();
        let to_col = rel_def.columns.to_identity();

        let keys: Vec<ValueTuple> = self
            .iter()
            .map(|model: &M| extract_key(&from_col, model))
            .collect::<Result<_, Error>>()?;

        let condition = prepare_condition(&rel_def.to_tbl, &to_col, &keys)?;

        let stmt = <Select<R> as QueryFilter>::filter(stmt.into_select(), condition);

        let data = stmt.all(db).await?;

        let mut hashmap: HashMap<ValueTuple, <R as EntityTrait>::Model> = HashMap::new();
        for value in data {
            let key = extract_key(&to_col, &value)?;
            hashmap.insert(key, value);
        }

        let result: Vec<Option<<R as EntityTrait>::Model>> =
            keys.iter().map(|key| hashmap.get(key).cloned()).collect();

        Ok(result)
    }

    // [spec:pgorm:sem:query.loader.regroup+3]
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

        let from_col = rel_def.columns.from_identity();
        let to_col = rel_def.columns.to_identity();

        let keys: Vec<ValueTuple> = self
            .iter()
            .map(|model: &M| extract_key(&from_col, model))
            .collect::<Result<_, Error>>()?;

        let condition = prepare_condition(&rel_def.to_tbl, &to_col, &keys)?;

        let stmt = <Select<R> as QueryFilter>::filter(stmt.into_select(), condition);

        let data = stmt.all(db).await?;

        let mut hashmap: HashMap<ValueTuple, Vec<<R as EntityTrait>::Model>> =
            keys.iter()
                .fold(HashMap::new(), |mut acc, key: &ValueTuple| {
                    acc.insert(key.clone(), Vec::new());
                    acc
                });

        for value in data {
            let key = extract_key(&to_col, &value)?;

            let vec = hashmap
                .get_mut(&key)
                .ok_or_else(|| unmatched_key_err(&key, &keys, &from_col, &to_col))?;

            vec.push(value);
        }

        let result: Vec<Vec<R::Model>> = keys
            .iter()
            .map(|key: &ValueTuple| hashmap.get(key).cloned().unwrap_or_default())
            .collect();

        Ok(result)
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

        // The source table is joined back in under an alias, so a
        // self-referencing many-to-many does not name one table twice, and the
        // key predicate qualifies against the alias rather than the table.
        let src_alias: DynIden = SharedIden::new(LOADER_SOURCE_ALIAS);
        let src_tbl = FromItem::from(TableName::Table(SharedIden::clone(&src_alias)));
        let condition = prepare_condition(&src_tbl, &via_from_col, &keys)?;

        // One graph read: the caller's target selector is the root, the
        // junction is a hop nobody decodes, and the input entity is the one
        // required slot, which is what carries the key back out of the join.
        // The hop joins LEFT and the slot INNER, which is INNER end to end:
        // the slot's ON references the junction's columns, and NULLs do not
        // satisfy it.
        let graph = many_to_many_graph::<R, <M as ModelTrait>::Entity>(
            stmt.into_select(),
            rel_def,
            via_rel,
            SharedIden::clone(&src_alias),
        );
        let graph = QueryFilter::filter(graph, condition);

        let mut buckets: HashMap<ValueTuple, Vec<R::Model>> = keys
            .iter()
            .map(|key: &ValueTuple| (key.clone(), Vec::new()))
            .collect();

        for (target, source) in graph.all(db).await? {
            let key = extract_key(&via_from_col, &source)?;
            let bucket = buckets
                .get_mut(&key)
                .ok_or_else(|| unmatched_key_err(&key, &keys, &via_from_col, &via_from_col))?;
            bucket.push(target);
        }

        Ok(keys
            .iter()
            .map(|key: &ValueTuple| buckets.get(key).cloned().unwrap_or_default())
            .collect())
    }
}

/// The alias the input entity's table is joined back under by
/// [`LoaderTrait::load_many_via`]. Internal: it is never handed to a caller,
/// who filters against the target entity by its own name.
const LOADER_SOURCE_ALIAS: AliasName = alias("pgorm_loader_src");

/// The one graph read [`LoaderTrait::load_many_via`] issues: the caller's
/// target selector rooted as the graph, the junction joined as a `via` hop
/// nobody decodes, and the input entity `F` joined back under `src_alias` as
/// the single required slot the key is read from.
// [spec:pgorm:sem:query.loader.many-to-many+3]
fn many_to_many_graph<R, F>(
    select: Select<R>,
    rel_def: RelationDef,
    via_rel: RelationDef,
    src_alias: DynIden,
) -> SelectGraph<R, (Req<F>,)>
where
    R: EntityTrait,
    F: EntityTrait,
{
    root_graph::<R>(select)
        .via(rev(rel_def))
        .join_one_as::<F>(rev(via_rel), src_alias)
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

/// Reverse a relation for the direction the graph joins it in, without
/// reversing what an authored `on_condition` is told.
///
/// [`RelationDef::rev`] hands the closure the swapped identifiers, so a
/// predicate written for `(junction, target)` would silently start receiving
/// `(target, junction)`. The loader walks both hops backwards purely because
/// the caller's selector is the root, which is no reason for a caller's
/// predicate to change meaning.
// [spec:pgorm:sem:query.loader.many-to-many+3]
fn rev(mut rel: RelationDef) -> RelationDef {
    let on_condition = rel.on_condition.take();
    let mut rel = rel.rev();
    rel.on_condition = on_condition.map(|f| {
        Box::new(move |left: DynIden, right: DynIden| f(right, left))
            as Box<dyn Fn(DynIden, DynIden) -> Condition + Send + Sync>
    });
    rel
}

fn identity_columns(identity: &Identity) -> String {
    identity
        .clone()
        .into_iter()
        .map(|col| col.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

// [spec:pgorm:sem:query.loader.regroup+3]
fn unmatched_key_err(
    key: &ValueTuple,
    input_keys: &[ValueTuple],
    from_col: &Identity,
    to_col: &Identity,
) -> Error {
    let sample = match input_keys.first() {
        Some(sample) => format!("{sample:?}"),
        None => "none".to_owned(),
    };
    query_err(format!(
        "Loader cannot regroup a returned row: the key {key:?} read from `{to}` equals none of \
         the keys read from `{from}` (an input key reads as {sample}). The two sides of the \
         relation match in SQL but not as Rust values; check for a width, padding or collation \
         difference between the columns.",
        to = identity_columns(to_col),
        from = identity_columns(from_col),
    ))
}

// [spec:pgorm:sem:query.loader.batching+3]
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

// [spec:pgorm:sem:query.loader.batching+3]
fn extract_key<Model>(target_col: &Identity, model: &Model) -> Result<ValueTuple, Error>
where
    Model: ModelTrait,
{
    Ok(match target_col {
        Identity::Unary(a) => ValueTuple::One(model.get(resolve_column::<Model>(a)?)),
        Identity::Binary(a, b) => ValueTuple::Two(
            model.get(resolve_column::<Model>(a)?),
            model.get(resolve_column::<Model>(b)?),
        ),
        Identity::Ternary(a, b, c) => ValueTuple::Three(
            model.get(resolve_column::<Model>(a)?),
            model.get(resolve_column::<Model>(b)?),
            model.get(resolve_column::<Model>(c)?),
        ),
        Identity::Many(cols) => {
            let mut values = Vec::with_capacity(cols.len());
            for col in cols {
                values.push(model.get(resolve_column::<Model>(col)?));
            }
            ValueTuple::Many(values)
        }
    })
}

// [spec:pgorm:sem:query.loader.batching+3]
fn prepare_condition(
    table: &FromItem,
    col: &Identity,
    keys: &[ValueTuple],
) -> Result<Condition, Error> {
    // TODO when value is hashable, retain only unique values
    let keys = keys.to_owned();
    Ok(match col {
        Identity::Unary(column_a) => {
            let column_a = table_column(table, column_a)?;
            Condition::all().add(Expr::col(column_a).is_in(keys.into_iter().flatten()))
        }
        Identity::Binary(column_a, column_b) => Condition::all().add(
            Expr::tuple([
                SimpleExpr::Column(table_column(table, column_a)?),
                SimpleExpr::Column(table_column(table, column_b)?),
            ])
            .in_tuples(keys),
        ),
        Identity::Ternary(column_a, column_b, column_c) => Condition::all().add(
            Expr::tuple([
                SimpleExpr::Column(table_column(table, column_a)?),
                SimpleExpr::Column(table_column(table, column_b)?),
                SimpleExpr::Column(table_column(table, column_c)?),
            ])
            .in_tuples(keys),
        ),
        Identity::Many(cols) => {
            let mut columns = Vec::with_capacity(cols.len());
            for col in cols {
                columns.push(SimpleExpr::Column(table_column(table, col)?));
            }
            Condition::all().add(Expr::tuple(columns).in_tuples(keys))
        }
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

// [spec:pgorm:sem:query.loader.many-to-many+3/test]    the one read the
// junction-mediated load issues: the caller's selector rooted and reprojected
// under the graph's prefixes, the junction joined but never projected, and the
// input entity joined back under the alias the key predicate qualifies against
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_cfg::{cake, filling};
    use pretty_assertions::assert_eq;

    #[test]
    fn many_to_many_reads_one_graph() {
        let via_rel = <cake::Entity as Related<filling::Entity>>::via()
            .expect("cake is related to filling through a junction");
        let rel_def = <cake::Entity as Related<filling::Entity>>::to();
        let via_from_col = via_rel.columns.from_identity();

        let src_alias: DynIden = SharedIden::new(LOADER_SOURCE_ALIAS);
        let src_tbl = FromItem::from(TableName::Table(SharedIden::clone(&src_alias)));
        let keys = vec![ValueTuple::One(1i32.into()), ValueTuple::One(2i32.into())];
        let condition = prepare_condition(&src_tbl, &via_from_col, &keys)
            .expect("a bare table qualifies the key column");

        let graph = many_to_many_graph::<filling::Entity, cake::Entity>(
            filling::Entity::find(),
            rel_def,
            via_rel,
            SharedIden::clone(&src_alias),
        );

        assert_eq!(
            QueryFilter::filter(graph, condition).as_query().to_string(),
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
