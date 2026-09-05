use crate::{
    ColumnTrait, Condition, ConnectionTrait, EntityName, EntityTrait, Error, IdenStr, Identity,
    Iterable, ModelTrait, QueryFilter, QuerySelect, QueryTrait, Related, RelationType, Select,
    SelectA, SelectB, SelectTwo, error::*,
};
use async_trait::async_trait;
use pgorm_query::{
    Alias, AliasName, ColumnRef, DynIden, Expr, FromItem, IntoColumnRef, IntoIden, JoinType,
    NamedTable, SeaRc, SelectExpr, SimpleExpr, TableName, ValueTuple, alias,
};
use std::{collections::HashMap, str::FromStr};

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

    // [spec:pgorm:sem:query.loader.many-to-many+2]
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
        let src_alias: DynIden = SeaRc::new(LOADER_SOURCE_ALIAS);
        let src_tbl = FromItem::from(TableName::Table(SeaRc::clone(&src_alias)));
        let condition = prepare_condition(&src_tbl, &via_from_col, &keys)?;

        let select = stmt
            .into_select()
            .join_rev(JoinType::InnerJoin, rel_def)
            .join_as_rev(JoinType::InnerJoin, via_rel, SeaRc::clone(&src_alias));
        let select = <Select<R> as QueryFilter>::filter(select, condition);

        // The input entity's own columns are what carries the key back out of
        // the join: they decode through its `Model`, the same path every other
        // read takes, so no column has to be decoded against a guessed type.
        let mut select_two: SelectTwo<R, <M as ModelTrait>::Entity> =
            SelectTwo::new_without_prepare(select.apply_alias(SelectA.as_str()).into_query());
        for col in <<<M as ModelTrait>::Entity as EntityTrait>::Column as Iterable>::iter() {
            let col_alias = format!("{}{}", SelectB.as_str(), col.as_str());
            let expr = Expr::col((SeaRc::clone(&src_alias), col.into_iden()));
            QuerySelect::query(&mut select_two).expr(SelectExpr::new_as(
                col.select_as(expr),
                SeaRc::new(Alias::new(col_alias)),
            ));
        }

        let mut buckets: HashMap<ValueTuple, Vec<R::Model>> = keys
            .iter()
            .map(|key: &ValueTuple| (key.clone(), Vec::new()))
            .collect();

        for (target, source) in select_two.all(db).await? {
            let Some(source) = source else {
                // The join is inner on both hops, so every returned row has a
                // source; a row without one carries no key and is not ours.
                continue;
            };
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
