use crate::{
    Condition, ConnectionTrait, DbErr, EntityTrait, Identity, ModelTrait, QueryFilter, Related,
    RelationType, Select, error::*,
};
use async_trait::async_trait;
use pgorm_query::{ColumnRef, DynIden, Expr, IntoColumnRef, SimpleExpr, TableRef, ValueTuple};
use std::{collections::HashMap, str::FromStr};

/// Entity, or a Select<Entity>; to be used as parameters in [`LoaderTrait`]
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
    async fn load_one<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Option<R::Model>>, DbErr>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>;

    /// Used to eager load has_many relations
    async fn load_many<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, DbErr>
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
    ) -> Result<Vec<Vec<R::Model>>, DbErr>
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

    async fn load_one<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Option<R::Model>>, DbErr>
    where
        C: ConnectionTrait,
        R: EntityTrait,
        R::Model: Send + Sync,
        S: EntityOrSelect<R>,
        <<Self as LoaderTrait>::Model as ModelTrait>::Entity: Related<R>,
    {
        self.as_slice().load_one(stmt, db).await
    }

    async fn load_many<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, DbErr>
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
    ) -> Result<Vec<Vec<R::Model>>, DbErr>
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

    // [spec:pgorm:sem:query.loader.regroup]
    async fn load_one<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Option<R::Model>>, DbErr>
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

        let keys: Vec<ValueTuple> = self
            .iter()
            .map(|model: &M| extract_key(&rel_def.from_col, model))
            .collect();

        let condition = prepare_condition(&rel_def.to_tbl, &rel_def.to_col, &keys);

        let stmt = <Select<R> as QueryFilter>::filter(stmt.select(), condition);

        let data = stmt.all(db).await?;

        let hashmap: HashMap<ValueTuple, <R as EntityTrait>::Model> = data.into_iter().fold(
            HashMap::new(),
            |mut acc, value: <R as EntityTrait>::Model| {
                {
                    let key = extract_key(&rel_def.to_col, &value);
                    acc.insert(key, value);
                }

                acc
            },
        );

        let result: Vec<Option<<R as EntityTrait>::Model>> =
            keys.iter().map(|key| hashmap.get(key).cloned()).collect();

        Ok(result)
    }

    // [spec:pgorm:sem:query.loader.regroup]
    async fn load_many<R, S, C>(&self, stmt: S, db: &C) -> Result<Vec<Vec<R::Model>>, DbErr>
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

        let keys: Vec<ValueTuple> = self
            .iter()
            .map(|model: &M| extract_key(&rel_def.from_col, model))
            .collect();

        let condition = prepare_condition(&rel_def.to_tbl, &rel_def.to_col, &keys);

        let stmt = <Select<R> as QueryFilter>::filter(stmt.select(), condition);

        let data = stmt.all(db).await?;

        let mut hashmap: HashMap<ValueTuple, Vec<<R as EntityTrait>::Model>> =
            keys.iter()
                .fold(HashMap::new(), |mut acc, key: &ValueTuple| {
                    acc.insert(key.clone(), Vec::new());
                    acc
                });

        data.into_iter()
            .for_each(|value: <R as EntityTrait>::Model| {
                let key = extract_key(&rel_def.to_col, &value);

                let vec = hashmap
                    .get_mut(&key)
                    .expect("Failed at finding key on hashmap");

                vec.push(value);
            });

        let result: Vec<Vec<R::Model>> = keys
            .iter()
            .map(|key: &ValueTuple| hashmap.get(key).cloned().unwrap_or_default())
            .collect();

        Ok(result)
    }

    // [spec:pgorm:sem:query.loader.many-to-many]
    async fn load_many_to_many<R, S, V, C>(
        &self,
        stmt: S,
        via: V,
        db: &C,
    ) -> Result<Vec<Vec<R::Model>>, DbErr>
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

            if !cmp_table_ref(&via_rel.to_tbl, &via.table_ref()) {
                return Err(query_err(format!(
                    "The given via Entity is incorrect: expected: {:?}, given: {:?}",
                    via_rel.to_tbl,
                    via.table_ref()
                )));
            }

            if self.is_empty() {
                return Ok(Vec::new());
            }

            let pkeys: Vec<ValueTuple> = self
                .iter()
                .map(|model: &M| extract_key(&via_rel.from_col, model))
                .collect();

            // Map of M::PK -> Vec<R::PK>
            let mut keymap: HashMap<ValueTuple, Vec<ValueTuple>> = Default::default();

            let keys: Vec<ValueTuple> = {
                let condition = prepare_condition(&via_rel.to_tbl, &via_rel.to_col, &pkeys);
                let stmt = V::find().filter(condition);
                let data = stmt.all(db).await?;
                data.into_iter().for_each(|model| {
                    let pk = extract_key(&via_rel.to_col, &model);
                    let entry = keymap.entry(pk).or_default();

                    let fk = extract_key(&rel_def.from_col, &model);
                    entry.push(fk);
                });

                keymap.values().flatten().cloned().collect()
            };

            let condition = prepare_condition(&rel_def.to_tbl, &rel_def.to_col, &keys);

            let stmt = <Select<R> as QueryFilter>::filter(stmt.select(), condition);

            let data = stmt.all(db).await?;

            // Map of R::PK -> R::Model
            let data: HashMap<ValueTuple, <R as EntityTrait>::Model> = data
                .into_iter()
                .map(|model| {
                    let key = extract_key(&rel_def.to_col, &model);
                    (key, model)
                })
                .collect();

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

fn cmp_table_ref(left: &TableRef, right: &TableRef) -> bool {
    // not ideal; but
    format!("{left:?}") == format!("{right:?}")
}

// [spec:pgorm:sem:query.loader.batching]
fn extract_key<Model>(target_col: &Identity, model: &Model) -> ValueTuple
where
    Model: ModelTrait,
{
    match target_col {
        Identity::Unary(a) => {
            let column_a =
                <<<Model as ModelTrait>::Entity as EntityTrait>::Column as FromStr>::from_str(
                    &a.to_string(),
                )
                .unwrap_or_else(|_| panic!("Failed at mapping string to column A:1"));
            ValueTuple::One(model.get(column_a))
        }
        Identity::Binary(a, b) => {
            let column_a =
                <<<Model as ModelTrait>::Entity as EntityTrait>::Column as FromStr>::from_str(
                    &a.to_string(),
                )
                .unwrap_or_else(|_| panic!("Failed at mapping string to column A:2"));
            let column_b =
                <<<Model as ModelTrait>::Entity as EntityTrait>::Column as FromStr>::from_str(
                    &b.to_string(),
                )
                .unwrap_or_else(|_| panic!("Failed at mapping string to column B:2"));
            ValueTuple::Two(model.get(column_a), model.get(column_b))
        }
        Identity::Ternary(a, b, c) => {
            let column_a =
                <<<Model as ModelTrait>::Entity as EntityTrait>::Column as FromStr>::from_str(
                    &a.to_string(),
                )
                .unwrap_or_else(|_| panic!("Failed at mapping string to column A:3"));
            let column_b =
                <<<Model as ModelTrait>::Entity as EntityTrait>::Column as FromStr>::from_str(
                    &b.to_string(),
                )
                .unwrap_or_else(|_| panic!("Failed at mapping string to column B:3"));
            let column_c =
                <<<Model as ModelTrait>::Entity as EntityTrait>::Column as FromStr>::from_str(
                    &c.to_string(),
                )
                .unwrap_or_else(|_| panic!("Failed at mapping string to column C:3"));
            ValueTuple::Three(
                model.get(column_a),
                model.get(column_b),
                model.get(column_c),
            )
        }
        Identity::Many(cols) => {
            let values = cols.iter().map(|col| {
                let col_name = col.to_string();
                let column = <<<Model as ModelTrait>::Entity as EntityTrait>::Column as FromStr>::from_str(
                    &col_name,
                )
                .unwrap_or_else(|_| panic!("Failed at mapping '{}' to column", col_name));
                model.get(column)
            })
            .collect();
            ValueTuple::Many(values)
        }
    }
}

// [spec:pgorm:sem:query.loader.batching]
fn prepare_condition(table: &TableRef, col: &Identity, keys: &[ValueTuple]) -> Condition {
    // TODO when value is hashable, retain only unique values
    let keys = keys.to_owned();
    match col {
        Identity::Unary(column_a) => {
            let column_a = table_column(table, column_a);
            Condition::all().add(Expr::col(column_a).is_in(keys.into_iter().flatten()))
        }
        Identity::Binary(column_a, column_b) => Condition::all().add(
            Expr::tuple([
                SimpleExpr::Column(table_column(table, column_a)),
                SimpleExpr::Column(table_column(table, column_b)),
            ])
            .in_tuples(keys),
        ),
        Identity::Ternary(column_a, column_b, column_c) => Condition::all().add(
            Expr::tuple([
                SimpleExpr::Column(table_column(table, column_a)),
                SimpleExpr::Column(table_column(table, column_b)),
                SimpleExpr::Column(table_column(table, column_c)),
            ])
            .in_tuples(keys),
        ),
        Identity::Many(cols) => {
            let columns = cols
                .iter()
                .map(|col| SimpleExpr::Column(table_column(table, col)));
            Condition::all().add(Expr::tuple(columns).in_tuples(keys))
        }
    }
}

// [spec:pgorm:req:query.loader.table-ref-limitation]
fn table_column(tbl: &TableRef, col: &DynIden) -> ColumnRef {
    match tbl.to_owned() {
        TableRef::Table(tbl) => (tbl, col.clone()).into_column_ref(),
        TableRef::SchemaTable(sch, tbl) => (sch, tbl, col.clone()).into_column_ref(),
        val => unimplemented!("Unsupported TableRef {val:?}"),
    }
}
