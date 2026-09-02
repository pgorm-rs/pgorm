#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{
    ActiveValue::Set, DatabaseConnection, DerivePartialModel, FromQueryResult, QuerySelect,
    entity::prelude::*,
};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn cursor_tests() -> Result<(), DbErr> {
    let ctx = TestContext::new("cursor_tests").await;
    create_tables(&ctx.db).await?;
    bakery_chain_schema::create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    create_insert_default(&db).await?;
    cursor_pagination(&db).await?;
    create_baker_cake(&db).await?;
    cursor_related_pagination(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

pub async fn create_insert_default(db: &DatabaseConnection) -> Result<(), DbErr> {
    use insert_default::*;

    for _ in 0..10 {
        ActiveModel {
            ..Default::default()
        }
        .insert(db)
        .await?;
    }

    assert_eq!(
        Entity::find().all(db).await?,
        [
            Model { id: 1 },
            Model { id: 2 },
            Model { id: 3 },
            Model { id: 4 },
            Model { id: 5 },
            Model { id: 6 },
            Model { id: 7 },
            Model { id: 8 },
            Model { id: 9 },
            Model { id: 10 },
        ]
    );

    Ok(())
}

// [spec:pgorm:def:exec.cursor/test]    `Select::cursor_by`, `asc`/`desc`, and
// the `into_model` / `into_partial_model` re-targeting
// [spec:pgorm:sem:exec.cursor.keyset/test]    `before` / `after` comparison
// direction under both sort orders, and both boundaries at once
// [spec:pgorm:sem:exec.cursor.window/test]    `first` and `last` clearing each
// other, and `last` reversing the fetched buffer back into logical order
pub async fn cursor_pagination(db: &DatabaseConnection) -> Result<(), DbErr> {
    use insert_default::*;

    // Before 5, i.e. id < 5

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.before(5);

    assert_eq!(
        cursor.first(4).all(db).await?,
        [
            Model { id: 1 },
            Model { id: 2 },
            Model { id: 3 },
            Model { id: 4 },
        ]
    );

    assert_eq!(
        cursor.first(5).all(db).await?,
        [
            Model { id: 1 },
            Model { id: 2 },
            Model { id: 3 },
            Model { id: 4 },
        ]
    );

    assert_eq!(
        cursor.last(4).all(db).await?,
        [
            Model { id: 1 },
            Model { id: 2 },
            Model { id: 3 },
            Model { id: 4 },
        ]
    );

    assert_eq!(
        cursor.last(5).all(db).await?,
        [
            Model { id: 1 },
            Model { id: 2 },
            Model { id: 3 },
            Model { id: 4 },
        ]
    );

    // Before 5 DESC, i.e. id > 5

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.before(5).desc();

    assert_eq!(
        cursor.first(4).all(db).await?,
        [
            Model { id: 10 },
            Model { id: 9 },
            Model { id: 8 },
            Model { id: 7 },
        ]
    );

    assert_eq!(
        cursor.first(5).all(db).await?,
        [
            Model { id: 10 },
            Model { id: 9 },
            Model { id: 8 },
            Model { id: 7 },
            Model { id: 6 },
        ]
    );

    assert_eq!(
        cursor.last(4).all(db).await?,
        [
            Model { id: 9 },
            Model { id: 8 },
            Model { id: 7 },
            Model { id: 6 },
        ]
    );

    assert_eq!(
        cursor.last(5).all(db).await?,
        [
            Model { id: 10 },
            Model { id: 9 },
            Model { id: 8 },
            Model { id: 7 },
            Model { id: 6 },
        ]
    );

    // After 5, i.e. id > 5

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(5);

    assert_eq!(
        cursor.first(4).all(db).await?,
        [
            Model { id: 6 },
            Model { id: 7 },
            Model { id: 8 },
            Model { id: 9 },
        ]
    );

    assert_eq!(
        cursor.first(5).all(db).await?,
        [
            Model { id: 6 },
            Model { id: 7 },
            Model { id: 8 },
            Model { id: 9 },
            Model { id: 10 },
        ]
    );

    assert_eq!(
        cursor.first(6).all(db).await?,
        [
            Model { id: 6 },
            Model { id: 7 },
            Model { id: 8 },
            Model { id: 9 },
            Model { id: 10 },
        ]
    );

    assert_eq!(
        cursor.last(4).all(db).await?,
        [
            Model { id: 7 },
            Model { id: 8 },
            Model { id: 9 },
            Model { id: 10 },
        ]
    );

    assert_eq!(
        cursor.last(5).all(db).await?,
        [
            Model { id: 6 },
            Model { id: 7 },
            Model { id: 8 },
            Model { id: 9 },
            Model { id: 10 },
        ]
    );

    assert_eq!(
        cursor.last(6).all(db).await?,
        [
            Model { id: 6 },
            Model { id: 7 },
            Model { id: 8 },
            Model { id: 9 },
            Model { id: 10 },
        ]
    );

    // After 5 DESC, i.e. id < 5

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(5).desc();

    assert_eq!(
        cursor.first(3).all(db).await?,
        [Model { id: 4 }, Model { id: 3 }, Model { id: 2 },]
    );

    assert_eq!(
        cursor.first(4).all(db).await?,
        [
            Model { id: 4 },
            Model { id: 3 },
            Model { id: 2 },
            Model { id: 1 },
        ]
    );

    assert_eq!(
        cursor.first(5).all(db).await?,
        [
            Model { id: 4 },
            Model { id: 3 },
            Model { id: 2 },
            Model { id: 1 },
        ]
    );

    assert_eq!(
        cursor.last(3).all(db).await?,
        [Model { id: 3 }, Model { id: 2 }, Model { id: 1 },]
    );

    assert_eq!(
        cursor.last(4).all(db).await?,
        [
            Model { id: 4 },
            Model { id: 3 },
            Model { id: 2 },
            Model { id: 1 },
        ]
    );

    assert_eq!(
        cursor.last(5).all(db).await?,
        [
            Model { id: 4 },
            Model { id: 3 },
            Model { id: 2 },
            Model { id: 1 },
        ]
    );

    // Between 5 and 8, i.e. id > 5 AND id < 8

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(5).before(8);

    assert_eq!(cursor.first(1).all(db).await?, [Model { id: 6 }]);

    assert_eq!(
        cursor.first(2).all(db).await?,
        [Model { id: 6 }, Model { id: 7 }]
    );

    assert_eq!(
        cursor.first(3).all(db).await?,
        [Model { id: 6 }, Model { id: 7 }]
    );

    assert_eq!(cursor.last(1).all(db).await?, [Model { id: 7 }]);

    assert_eq!(
        cursor.last(2).all(db).await?,
        [Model { id: 6 }, Model { id: 7 }]
    );

    assert_eq!(
        cursor.last(3).all(db).await?,
        [Model { id: 6 }, Model { id: 7 }]
    );

    // Between 8 and 5 DESC, i.e. id < 8 AND id > 5

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(8).before(5).desc();

    assert_eq!(cursor.first(1).all(db).await?, [Model { id: 7 }]);

    assert_eq!(
        cursor.first(2).all(db).await?,
        [Model { id: 7 }, Model { id: 6 }]
    );

    assert_eq!(
        cursor.first(3).all(db).await?,
        [Model { id: 7 }, Model { id: 6 }]
    );

    assert_eq!(cursor.last(1).all(db).await?, [Model { id: 6 }]);

    assert_eq!(
        cursor.last(2).all(db).await?,
        [Model { id: 7 }, Model { id: 6 }]
    );

    assert_eq!(
        cursor.last(3).all(db).await?,
        [Model { id: 7 }, Model { id: 6 }]
    );

    // Ensure asc/desc order can be changed

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.first(2);

    assert_eq!(cursor.all(db).await?, [Model { id: 1 }, Model { id: 2 },]);

    assert_eq!(
        cursor.asc().all(db).await?,
        [Model { id: 1 }, Model { id: 2 },]
    );

    assert_eq!(
        cursor.desc().all(db).await?,
        [Model { id: 10 }, Model { id: 9 },]
    );

    assert_eq!(
        cursor.asc().all(db).await?,
        [Model { id: 1 }, Model { id: 2 },]
    );

    assert_eq!(
        cursor.desc().all(db).await?,
        [Model { id: 10 }, Model { id: 9 },]
    );

    // Fetch custom struct

    #[derive(FromQueryResult, Debug, PartialEq, Clone)]
    struct Row {
        id: i32,
    }

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(5).before(8);

    let mut cursor = cursor.into_model::<Row>();

    assert_eq!(
        cursor.first(2).all(db).await?,
        [Row { id: 6 }, Row { id: 7 }]
    );

    assert_eq!(
        cursor.first(3).all(db).await?,
        [Row { id: 6 }, Row { id: 7 }]
    );

    // Fetch custom struct desc

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(8).before(5).desc();

    let mut cursor = cursor.into_model::<Row>();

    assert_eq!(
        cursor.first(2).all(db).await?,
        [Row { id: 7 }, Row { id: 6 }]
    );

    assert_eq!(
        cursor.first(3).all(db).await?,
        [Row { id: 7 }, Row { id: 6 }]
    );

    // Fetch partial model

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(5).before(8);

    #[derive(DerivePartialModel, FromQueryResult, Debug, PartialEq, Clone)]
    #[pgorm(entity = "Entity")]
    struct PartialRow {
        #[pgorm(from_col = "id")]
        id: i32,
        #[pgorm(from_expr = "pgorm_query::Expr::col(Column::Id).add(1000)")]
        id_shifted: i32,
    }

    let mut cursor = cursor.into_partial_model::<PartialRow>();

    assert_eq!(
        cursor.first(2).all(db).await?,
        [
            PartialRow {
                id: 6,
                id_shifted: 1006,
            },
            PartialRow {
                id: 7,
                id_shifted: 1007,
            }
        ]
    );

    assert_eq!(
        cursor.first(3).all(db).await?,
        [
            PartialRow {
                id: 6,
                id_shifted: 1006,
            },
            PartialRow {
                id: 7,
                id_shifted: 1007,
            }
        ]
    );

    // Fetch partial model desc

    let mut cursor = Entity::find().cursor_by(Column::Id);

    cursor.after(8).before(5).desc();

    let mut cursor = cursor.into_partial_model::<PartialRow>();

    assert_eq!(
        cursor.first(2).all(db).await?,
        [
            PartialRow {
                id: 7,
                id_shifted: 1007,
            },
            PartialRow {
                id: 6,
                id_shifted: 1006,
            }
        ]
    );

    assert_eq!(
        cursor.first(3).all(db).await?,
        [
            PartialRow {
                id: 7,
                id_shifted: 1007,
            },
            PartialRow {
                id: 6,
                id_shifted: 1006,
            },
        ]
    );

    Ok(())
}

use common::bakery_chain::{
    Baker, Bakery, Cake, CakesBakers, baker, bakery, cake, cakes_bakers,
    schema as bakery_chain_schema,
};

fn bakery(i: i32) -> bakery::Model {
    bakery::Model {
        name: i.to_string(),
        profit_margin: 10.4,
        id: i,
    }
}
fn baker(c: char) -> baker::Model {
    baker::Model {
        name: c.clone().to_string(),
        contact_details: serde_json::json!({
            "mobile": "+61424000000",
        }),
        bakery_id: Some((c as i32 - 65) % 10 + 1),
        id: c as i32 - 64,
    }
}

#[derive(Debug, FromQueryResult, PartialEq)]
pub struct CakeBakerlite {
    pub cake_name: String,
    pub cake_id: i32,
    pub baker_name: String,
}

fn cakebaker(cake: char, baker: char) -> CakeBakerlite {
    CakeBakerlite {
        cake_name: cake.to_string(),
        cake_id: cake as i32 - 96,
        baker_name: baker.to_string(),
    }
}

pub async fn create_baker_cake(db: &DatabaseConnection) -> Result<(), DbErr> {
    let mut bakeries: Vec<bakery::ActiveModel> = vec![];
    // bakeries named from 1 to 10
    for i in 1..=10 {
        bakeries.push(bakery::ActiveModel {
            name: Set(i.to_string()),
            profit_margin: Set(10.4),
            ..Default::default()
        });
    }
    let _ = Bakery::insert_many(bakeries).exec(db).await?;

    let mut bakers: Vec<baker::ActiveModel> = vec![];
    let mut cakes: Vec<cake::ActiveModel> = vec![];
    let mut cakes_bakers: Vec<cakes_bakers::ActiveModel> = vec![];
    // baker and cakes named from "A" to "Z" and from "a" to "z"
    for c in 'A'..='Z' {
        bakers.push(baker::ActiveModel {
            name: Set(c.clone().to_string()),
            contact_details: Set(serde_json::json!({
                "mobile": "+61424000000",
            })),
            bakery_id: Set(Some((c as i32 - 65) % 10 + 1)),
            ..Default::default()
        });
        cakes.push(cake::ActiveModel {
            name: Set(c.to_ascii_lowercase().to_string()),
            price: Set(rust_dec(10.25)),
            gluten_free: Set(false),
            serial: Set(Uuid::new_v4()),
            bakery_id: Set(Some((c as i32 - 65) % 10 + 1)),
            ..Default::default()
        });
        cakes_bakers.push(cakes_bakers::ActiveModel {
            cake_id: Set(c as i32 - 64),
            baker_id: Set(c as i32 - 64),
        })
    }
    cakes_bakers.append(
        vec![
            cakes_bakers::ActiveModel {
                cake_id: Set(2),
                baker_id: Set(1),
            },
            cakes_bakers::ActiveModel {
                cake_id: Set(1),
                baker_id: Set(2),
            },
        ]
        .as_mut(),
    );
    Baker::insert_many(bakers).exec(db).await?;
    Cake::insert_many(cakes).exec(db).await?;
    CakesBakers::insert_many(cakes_bakers).exec(db).await?;

    Ok(())
}

// [spec:pgorm:def:exec.cursor/test]    `SelectTwo::cursor_by` and
// `cursor_by_other` on a joined select, decoded through `into_model`
// [spec:pgorm:sem:exec.cursor.order/test]    a joined cursor's automatic
// secondary order on the other entity's primary key, giving the deterministic
// tiebreak the row order below depends on
pub async fn cursor_related_pagination(db: &DatabaseConnection) -> Result<(), DbErr> {
    use common::bakery_chain::*;

    assert_eq!(
        bakery::Entity::find()
            .cursor_by(bakery::Column::Id)
            .before(5)
            .first(4)
            .all(db)
            .await?,
        [bakery(1), bakery(2), bakery(3), bakery(4),]
    );

    assert_eq!(
        bakery::Entity::find()
            .cursor_by(bakery::Column::Id)
            .before(5)
            .first(4)
            .desc()
            .all(db)
            .await?,
        [bakery(10), bakery(9), bakery(8), bakery(7),]
    );

    assert_eq!(
        bakery::Entity::find()
            .find_also_related(Baker)
            .cursor_by(bakery::Column::Id)
            .before(5)
            .first(20)
            .all(db)
            .await?,
        [
            (bakery(1), Some(baker('A'))),
            (bakery(1), Some(baker('K'))),
            (bakery(1), Some(baker('U'))),
            (bakery(2), Some(baker('B'))),
            (bakery(2), Some(baker('L'))),
            (bakery(2), Some(baker('V'))),
            (bakery(3), Some(baker('C'))),
            (bakery(3), Some(baker('M'))),
            (bakery(3), Some(baker('W'))),
            (bakery(4), Some(baker('D'))),
            (bakery(4), Some(baker('N'))),
            (bakery(4), Some(baker('X'))),
        ]
    );

    assert_eq!(
        bakery::Entity::find()
            .find_also_related(Baker)
            .cursor_by(bakery::Column::Id)
            .after(5)
            .last(20)
            .desc()
            .all(db)
            .await?,
        [
            (bakery(4), Some(baker('X'))),
            (bakery(4), Some(baker('N'))),
            (bakery(4), Some(baker('D'))),
            (bakery(3), Some(baker('W'))),
            (bakery(3), Some(baker('M'))),
            (bakery(3), Some(baker('C'))),
            (bakery(2), Some(baker('V'))),
            (bakery(2), Some(baker('L'))),
            (bakery(2), Some(baker('B'))),
            (bakery(1), Some(baker('U'))),
            (bakery(1), Some(baker('K'))),
            (bakery(1), Some(baker('A'))),
        ]
    );

    assert_eq!(
        bakery::Entity::find()
            .find_also_related(Baker)
            .cursor_by(bakery::Column::Id)
            .before(5)
            .first(4)
            .all(db)
            .await?,
        [
            (bakery(1), Some(baker('A'))),
            (bakery(1), Some(baker('K'))),
            (bakery(1), Some(baker('U'))),
            (bakery(2), Some(baker('B'))),
        ]
    );

    assert_eq!(
        bakery::Entity::find()
            .find_also_related(Baker)
            .cursor_by(bakery::Column::Id)
            .after(5)
            .last(4)
            .desc()
            .all(db)
            .await?,
        [
            (bakery(2), Some(baker('B'))),
            (bakery(1), Some(baker('U'))),
            (bakery(1), Some(baker('K'))),
            (bakery(1), Some(baker('A'))),
        ]
    );

    // since "10" is before "2" lexicologically, it return that first
    assert_eq!(
        bakery::Entity::find()
            .find_also_related(Baker)
            .cursor_by(bakery::Column::Name)
            .after("3")
            .last(4)
            .desc()
            .all(db)
            .await?,
        [
            (bakery(10), Some(baker('J'))),
            (bakery(1), Some(baker('U'))),
            (bakery(1), Some(baker('K'))),
            (bakery(1), Some(baker('A'))),
        ]
    );

    // since "10" is before "2" lexicologically, it return that first
    assert_eq!(
        bakery::Entity::find()
            .find_also_related(Baker)
            .cursor_by(bakery::Column::Name)
            .before("3")
            .first(4)
            .all(db)
            .await?,
        [
            (bakery(1), Some(baker('A'))),
            (bakery(1), Some(baker('K'))),
            (bakery(1), Some(baker('U'))),
            (bakery(10), Some(baker('J'))),
        ]
    );

    // since "10" is before "2" lexicologically, it return that first
    assert_eq!(
        bakery::Entity::find()
            .find_also_related(Baker)
            .cursor_by(bakery::Column::Name)
            .after("3")
            .last(4)
            .desc()
            .all(db)
            .await?,
        [
            (bakery(10), Some(baker('J'))),
            (bakery(1), Some(baker('U'))),
            (bakery(1), Some(baker('K'))),
            (bakery(1), Some(baker('A'))),
        ]
    );

    #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
    enum QueryAs {
        CakeId,
        CakeName,
        BakerName,
    }

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by(cake::Column::Name)
            .before("e")
            .first(4)
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![
            cakebaker('a', 'A'),
            cakebaker('a', 'B'),
            cakebaker('b', 'A'),
            cakebaker('b', 'B')
        ]
    );

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by(cake::Column::Name)
            .after("e")
            .last(4)
            .desc()
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![
            cakebaker('b', 'B'),
            cakebaker('b', 'A'),
            cakebaker('a', 'B'),
            cakebaker('a', 'A')
        ]
    );

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by(cake::Column::Name)
            .before("b")
            .first(4)
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![cakebaker('a', 'A'), cakebaker('a', 'B')]
    );

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by(cake::Column::Name)
            .after("b")
            .last(4)
            .desc()
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![cakebaker('a', 'B'), cakebaker('a', 'A')]
    );

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by_other(baker::Column::Name)
            .before("B")
            .first(4)
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![cakebaker('a', 'A'), cakebaker('b', 'A'),]
    );

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by_other(baker::Column::Name)
            .after("B")
            .last(4)
            .desc()
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![cakebaker('b', 'A'), cakebaker('a', 'A'),]
    );

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by_other(baker::Column::Name)
            .before("E")
            .first(20)
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![
            cakebaker('a', 'A'),
            cakebaker('b', 'A'),
            cakebaker('a', 'B'),
            cakebaker('b', 'B'),
            cakebaker('c', 'C'),
            cakebaker('d', 'D'),
        ]
    );

    assert_eq!(
        cake::Entity::find()
            .find_also_related(Baker)
            .select_only()
            .column_as(cake::Column::Id, QueryAs::CakeId)
            .column_as(cake::Column::Name, QueryAs::CakeName)
            .column_as(baker::Column::Name, QueryAs::BakerName)
            .cursor_by_other(baker::Column::Name)
            .after("E")
            .last(20)
            .desc()
            .clone()
            .into_model::<CakeBakerlite>()
            .all(db)
            .await?,
        vec![
            cakebaker('d', 'D'),
            cakebaker('c', 'C'),
            cakebaker('b', 'B'),
            cakebaker('a', 'B'),
            cakebaker('b', 'A'),
            cakebaker('a', 'A'),
        ]
    );

    Ok(())
}

mod net_cursor {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "net_cursor")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub label: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// [spec:pgorm:req:exec.cursor.binding-gaps+2/test]
// [spec:pgorm:def:exec.cursor.binding+2/test]    `IpNetwork` and `MacAddress`
// bound by hand through `postgres_protocol`, and the deliberately permissive
// `accepts`
#[pgorm_macros::test]
async fn cursor_over_network_types() -> Result<(), DbErr> {
    use ipnetwork::IpNetwork;
    use mac_address::MacAddress;
    use net_cursor::{Column, Entity, Model};
    use pgorm::{
        ValueHolder,
        types::{ToSql, Type},
    };
    use pgorm_query::{Alias, ColumnDef, IntoIden, Query, QueryBuilder, Table};

    // `accepts` is true for every Postgres type: a `Value` carries no target
    // type, so the whole burden sits in `to_sql`.
    for ty in [
        Type::BOOL,
        Type::INT2,
        Type::INT4,
        Type::INT8,
        Type::FLOAT4,
        Type::FLOAT8,
        Type::NUMERIC,
        Type::TEXT,
        Type::BYTEA,
        Type::JSONB,
        Type::UUID,
        Type::TIMESTAMPTZ,
        Type::INET,
        Type::MACADDR,
        Type::INT4_ARRAY,
        Type::OID,
        Type::ANY,
    ] {
        assert!(
            <ValueHolder as ToSql>::accepts(&ty),
            "ValueHolder must accept {ty}"
        );
    }

    let ctx = TestContext::new("cursor_network_type_tests").await;
    let db = ctx.db.get().await?;

    let ip_col = Alias::new("ip");
    let mac_col = Alias::new("mac");

    let create = Table::create()
        .table(Entity)
        .col(
            ColumnDef::new(Column::Id)
                .integer()
                .not_null()
                .primary_key(),
        )
        .col(ColumnDef::new(Column::Label).string().not_null())
        .col(ColumnDef::new(ip_col.clone()).inet().not_null())
        .col(ColumnDef::new(mac_col.clone()).mac_address().not_null())
        .to_owned();
    create_table_without_asserts(&db, &create).await?;

    let rows: Vec<(i32, &str, &str, &str)> = vec![
        (1, "alpha", "10.0.0.1/32", "00:11:22:33:44:01"),
        (2, "bravo", "10.0.0.2/32", "00:11:22:33:44:02"),
        (3, "charlie", "10.0.0.3/32", "00:11:22:33:44:03"),
        (4, "delta", "10.0.0.4/32", "00:11:22:33:44:04"),
        (5, "echo", "2001:db8::5/128", "00:11:22:33:44:05"),
    ];

    for (id, label, ip, mac) in &rows {
        let ip: IpNetwork = ip.parse().unwrap();
        let mac: MacAddress = mac.parse().unwrap();
        let (sql, values) = Query::insert()
            .into_table(Entity)
            .columns([
                Column::Id.into_iden(),
                Column::Label.into_iden(),
                ip_col.clone().into_iden(),
                mac_col.clone().into_iden(),
            ])
            .values_panic([(*id).into(), (*label).into(), ip.into(), mac.into()])
            .to_owned()
            .build(QueryBuilder);
        let bound: Vec<ValueHolder> = values.into_iter().map(ValueHolder).collect();
        let params: Vec<&(dyn ToSql + Sync)> =
            bound.iter().map(|v| v as &(dyn ToSql + Sync)).collect();
        db.execute(&sql, &params).await?;
    }

    let model = |id: i32, label: &str| Model {
        id,
        label: label.to_owned(),
    };

    let after: IpNetwork = "10.0.0.2/32".parse().unwrap();
    assert_eq!(
        Entity::find()
            .cursor_by("ip")
            .after(after)
            .first(2)
            .all(&db)
            .await?,
        [model(3, "charlie"), model(4, "delta")]
    );

    let before: IpNetwork = "10.0.0.3/32".parse().unwrap();
    assert_eq!(
        Entity::find()
            .cursor_by("ip")
            .before(before)
            .last(2)
            .all(&db)
            .await?,
        [model(1, "alpha"), model(2, "bravo")]
    );

    let after_mac: MacAddress = "00:11:22:33:44:03".parse().unwrap();
    assert_eq!(
        Entity::find()
            .cursor_by("mac")
            .after(after_mac)
            .first(2)
            .all(&db)
            .await?,
        [model(4, "delta"), model(5, "echo")]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

mod cursor_composite {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "cursor_composite")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub a: i32,
        pub b: i32,
        pub c: i32,
        pub d: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// `(a, b)` pairs in ascending order, one row per id starting at 1.
const COMPOSITE_ROWS: [(i32, i32); 8] = [
    (1, 1),
    (1, 2),
    (1, 3),
    (2, 1),
    (2, 2),
    (2, 3),
    (3, 1),
    (3, 2),
];

async fn seed_composite(db: &DatabaseConnection) -> Result<(), DbErr> {
    use cursor_composite::*;

    let schema = pgorm::Schema::new();
    create_table_without_asserts(db, &schema.create_table_from_entity(Entity)).await?;

    for (index, (a, b)) in COMPOSITE_ROWS.into_iter().enumerate() {
        let id = index as i32 + 1;
        ActiveModel {
            id: Set(id),
            a: Set(a),
            b: Set(b),
            // `c` and `d` are unique, so a ternary or n-ary order key sorts
            // exactly as `(a, b)` does.
            c: Set(id),
            d: Set(id),
        }
        .insert(db)
        .await?;
    }

    Ok(())
}

fn ids(rows: &[cursor_composite::Model]) -> Vec<i32> {
    rows.iter().map(|row| row.id).collect()
}

// [spec:pgorm:sem:exec.cursor.keyset/test]    the row-value emulation of a
// composite boundary, in each arity, in both sort directions
#[pgorm_macros::test]
async fn cursor_composite_keyset() -> Result<(), DbErr> {
    use cursor_composite::{Column, Entity};

    let ctx = TestContext::new("cursor_tests_composite_keyset").await;
    let db = ctx.db.get().await?;
    seed_composite(&db).await?;

    // Binary: `(a = 1 AND b > 2) OR (a > 1)`.
    assert_eq!(
        ids(&Entity::find()
            .cursor_by((Column::A, Column::B))
            .after((1, 2))
            .first(3)
            .all(&db)
            .await?),
        [3, 4, 5]
    );

    // `before` is the mirror image, and `last` takes the window from the far
    // end while still reporting rows in logical order.
    assert_eq!(
        ids(&Entity::find()
            .cursor_by((Column::A, Column::B))
            .before((2, 2))
            .last(3)
            .all(&db)
            .await?),
        [2, 3, 4]
    );

    // Both boundaries may be set at once.
    assert_eq!(
        ids(&Entity::find()
            .cursor_by((Column::A, Column::B))
            .after((1, 1))
            .before((2, 3))
            .first(10)
            .all(&db)
            .await?),
        [2, 3, 4, 5]
    );

    // Descending flips the comparison: `after` means "before" in row order.
    assert_eq!(
        ids(&Entity::find()
            .cursor_by((Column::A, Column::B))
            .after((2, 2))
            .desc()
            .first(3)
            .all(&db)
            .await?),
        [4, 3, 2]
    );

    // Ternary and n-ary identities expand the same way.
    assert_eq!(
        ids(&Entity::find()
            .cursor_by((Column::A, Column::B, Column::C))
            .after((1, 2, 2))
            .first(3)
            .all(&db)
            .await?),
        [3, 4, 5]
    );
    assert_eq!(
        ids(&Entity::find()
            .cursor_by((Column::A, Column::B, Column::C, Column::D))
            .after((1, 2, 2, 2))
            .first(3)
            .all(&db)
            .await?),
        [3, 4, 5]
    );
    assert_eq!(
        ids(&Entity::find()
            .cursor_by((Column::A, Column::B, Column::C, Column::D))
            .before((2, 2, 5, 5))
            .last(2)
            .all(&db)
            .await?),
        [3, 4]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.cursor.keyset/test]    a boundary tuple whose arity does
// not match the order columns panics with "column arity mismatch"
#[pgorm_macros::test]
async fn cursor_boundary_arity_mismatch() -> Result<(), DbErr> {
    use cursor_composite::{Column, Entity};
    use futures::FutureExt;

    let ctx = TestContext::new("cursor_tests_arity_mismatch").await;
    let db = ctx.db.get().await?;
    seed_composite(&db).await?;

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    // A pair against a unary order column.
    let unary = std::panic::AssertUnwindSafe(async {
        Entity::find()
            .cursor_by(Column::A)
            .after((1, 2))
            .first(1)
            .all(&db)
            .await
    })
    .catch_unwind()
    .await;

    // And a five-element tuple against a four-column `Identity::Many`.
    let many = std::panic::AssertUnwindSafe(async {
        Entity::find()
            .cursor_by((Column::A, Column::B, Column::C, Column::D))
            .after((1, 2, 3, 4, 5))
            .first(1)
            .all(&db)
            .await
    })
    .catch_unwind()
    .await;

    std::panic::set_hook(hook);

    for outcome in [unary, many] {
        let payload = outcome.expect_err("an arity mismatch must panic");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or_default()
            .to_owned();
        assert_eq!(message, "column arity mismatch");
    }

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.cursor.order/test]    ordering clears any pre-existing
// ORDER BY, applies the order columns in declared order, then the unary
// secondary entries, all in the single resolved direction
#[pgorm_macros::test]
async fn cursor_order_composition() -> Result<(), DbErr> {
    use cursor_composite::{Column, Entity};
    use pgorm::{Identity, IntoIdentity, QueryOrder};
    use pgorm_query::{Alias, IntoIden};

    let ctx = TestContext::new("cursor_tests_order_composition").await;
    let db = ctx.db.get().await?;
    seed_composite(&db).await?;

    // An ORDER BY set on the select before the cursor is discarded.
    assert_eq!(
        ids(&Entity::find()
            .order_by_desc(Column::Id)
            .cursor_by(Column::Id)
            .first(3)
            .all(&db)
            .await?),
        [1, 2, 3]
    );

    // So is one applied to the cursor itself through `QueryOrder`.
    let mut reordered = Entity::find()
        .cursor_by(Column::Id)
        .order_by_desc(Column::Id);
    assert_eq!(ids(&reordered.first(3).all(&db).await?), [1, 2, 3]);

    // Order columns are applied in declared order, so `(b, a)` sorts by `b`
    // first — the opposite grouping to `(a, b)`.
    let table: DynIden = SeaRc::new(Entity);
    let mut by_b_then_a = Entity::find().cursor_by((Column::B, Column::A));
    // A unary secondary entry breaks the remaining tie deterministically.
    by_b_then_a.set_secondary_order_by(vec![(SeaRc::clone(&table), Column::Id.into_identity())]);
    assert_eq!(ids(&by_b_then_a.first(4).all(&db).await?), [1, 4, 7, 2]);

    // The secondary entry follows the cursor's resolved direction too.
    let mut descending = Entity::find().cursor_by((Column::B, Column::A));
    descending.set_secondary_order_by(vec![(SeaRc::clone(&table), Column::Id.into_identity())]);
    assert_eq!(ids(&descending.desc().first(3).all(&db).await?), [6, 3, 8]);

    // Only unary secondary entries are applied. A unary entry naming a column
    // that does not exist reaches the server and is rejected...
    let mut bad_unary = Entity::find().cursor_by(Column::A);
    bad_unary.set_secondary_order_by(vec![(
        SeaRc::clone(&table),
        Identity::Unary(Alias::new("no_such_column").into_iden()),
    )]);
    assert!(matches!(
        bad_unary.first(8).all(&db).await,
        Err(DbErr::Postgres(_))
    ));

    // ... whereas the same columns wrapped in a composite identity are dropped
    // before the SQL is built, so the query succeeds.
    let mut composite_secondary = Entity::find().cursor_by(Column::A);
    composite_secondary.set_secondary_order_by(vec![(
        SeaRc::clone(&table),
        Identity::Binary(
            Alias::new("no_such_column").into_iden(),
            Alias::new("nor_this_one").into_iden(),
        ),
    )]);
    assert_eq!(composite_secondary.first(8).all(&db).await?.len(), 8);

    drop(db);
    ctx.delete().await;

    Ok(())
}
