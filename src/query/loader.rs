use crate::{
    Condition, ConnectionTrait, EntityName, EntityTrait, Error, Identity, ModelTrait, QueryFilter,
    Related, RelationType, Select, error::*,
};
use async_trait::async_trait;
use pgorm_query::{
    ColumnRef, DynIden, Expr, FromItem, IntoColumnRef, NamedTable, SimpleExpr, TableName,
    ValueTuple,
};
use std::{collections::HashMap, str::FromStr};

/// Entity, or a `Select<Entity>`; to be used as parameters in [`LoaderTrait`]
pub trait EntityOrSelect<E: EntityTrait>: Send {
    /// If self is Entity, use Entity::find()
    fn select(self) -> Select<E>;
}

/// This trait implements the Data Loader API
// [spec:pgorm:req:query.loader]
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

    /// Used to eager load many_to_many relations
    async fn load_many_to_many<R, S, V, C>(
        &self,
        stmt: S,
        via: V,
        db: &C,
    ) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        V: EntityTrait,
        V::Model: Send + Sync,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>;
}

impl<E> EntityOrSelect<E> for E
where
    E: EntityTrait,
{
    fn select(self) -> Select<E> {
        E::find()
    }
}

impl<E> EntityOrSelect<E> for Select<E>
where
    E: EntityTrait,
{
    fn select(self) -> Select<E> {
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

    async fn load_many_to_many<R, S, V, C>(
        &self,
        stmt: S,
        via: V,
        db: &C,
    ) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        V: EntityTrait,
        V::Model: Send + Sync,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        self.as_slice().load_many_to_many(stmt, via, db).await
    }
}

// [spec:pgorm:req:query.loader]
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

        let stmt = <Select<R> as QueryFilter>::filter(stmt.select(), condition);

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

        let stmt = <Select<R> as QueryFilter>::filter(stmt.select(), condition);

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

    // [spec:pgorm:sem:query.loader.many-to-many+1]
    async fn load_many_to_many<R, S, V, C>(
        &self,
        stmt: S,
        via: V,
        db: &C,
    ) -> Result<Vec<Vec<R::Model>>, Error>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        V: EntityTrait,
        V::Model: Send + Sync,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        if let Some(via_rel) =
            <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::via()
        {
            let rel_def =
                <<<Self as LoaderTrait>::Model as ModelTrait>::Entity as Related<R>>::to();
            if rel_def.rel_type != RelationType::HasOne {
                return Err(query_err("Relation to is not HasOne"));
            }

            let via_tbl = FromItem::from(via.table_ref());
            if !cmp_table_ref(&via_rel.to_tbl, &via_tbl) {
                return Err(query_err(format!(
                    "The given via Entity is incorrect: expected: {:?}, given: {via_tbl:?}",
                    via_rel.to_tbl,
                )));
            }

            if self.is_empty() {
                return Ok(Vec::new());
            }

            let via_from_col = via_rel.columns.from_identity();
            let via_to_col = via_rel.columns.to_identity();
            let from_col = rel_def.columns.from_identity();
            let to_col = rel_def.columns.to_identity();

            let pkeys: Vec<ValueTuple> = self
                .iter()
                .map(|model: &M| extract_key(&via_from_col, model))
                .collect::<Result<_, Error>>()?;

            // Map of M::PK -> Vec<R::PK>
            let mut keymap: HashMap<ValueTuple, Vec<ValueTuple>> = Default::default();

            let keys: Vec<ValueTuple> = {
                let condition = prepare_condition(&via_rel.to_tbl, &via_to_col, &pkeys)?;
                let stmt = V::find().filter(condition);
                let data = stmt.all(db).await?;
                for model in data {
                    let pk = extract_key(&via_to_col, &model)?;
                    let fk = extract_key(&from_col, &model)?;
                    keymap.entry(pk).or_default().push(fk);
                }

                keymap.values().flatten().cloned().collect()
            };

            let condition = prepare_condition(&rel_def.to_tbl, &to_col, &keys)?;

            let stmt = <Select<R> as QueryFilter>::filter(stmt.select(), condition);

            // Map of R::PK -> R::Model
            let mut data: HashMap<ValueTuple, <R as EntityTrait>::Model> = HashMap::new();
            for model in stmt.all(db).await? {
                let key = extract_key(&to_col, &model)?;
                data.insert(key, model);
            }

            let result: Vec<Vec<R::Model>> = pkeys
                .into_iter()
                .map(|pkey| {
                    let fkeys = keymap.get(&pkey).cloned().unwrap_or_default();

                    let models: Vec<_> = fkeys
                        .into_iter()
                        .filter_map(|fkey| data.get(&fkey).cloned())
                        .collect();

                    models
                })
                .collect();

            Ok(result)
        } else {
            return Err(query_err("Relation is not ManyToMany"));
        }
    }
}

fn cmp_table_ref(left: &FromItem, right: &FromItem) -> bool {
    // not ideal; but
    format!("{left:?}") == format!("{right:?}")
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
