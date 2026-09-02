#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{DatabaseConnection, QueryOrder, QuerySelect, Schema, entity::prelude::*, entity::*};
use pretty_assertions::assert_eq;

mod net_decode {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "net_decode")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub label: String,
        pub ip: IpNetwork,
        pub mac: MacAddress,
        pub gateway: Option<IpNetwork>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

fn net(s: &str) -> IpNetwork {
    s.parse().expect("network literal")
}

fn mac(s: &str) -> MacAddress {
    s.parse().expect("mac literal")
}

fn models() -> Vec<net_decode::Model> {
    vec![
        net_decode::Model {
            id: 1,
            label: "alpha".to_owned(),
            ip: net("10.0.0.1/32"),
            mac: mac("00:11:22:33:44:01"),
            gateway: Some(net("10.0.0.254/32")),
        },
        net_decode::Model {
            id: 2,
            label: "bravo".to_owned(),
            ip: net("2001:db8::5/128"),
            mac: mac("aa:bb:cc:dd:ee:ff"),
            gateway: None,
        },
        net_decode::Model {
            id: 3,
            label: "charlie".to_owned(),
            ip: net("192.168.0.0/24"),
            mac: mac("00:00:00:00:00:00"),
            gateway: Some(net("2001:db8::1/128")),
        },
    ]
}

// [spec:pgorm:def:exec.decode.types+1/test]
#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("value_decode_tests_valuedecode").await;
    let db = ctx.db.get().await?;

    let schema = Schema::new();
    create_table_without_asserts(&db, &schema.create_table_from_entity(net_decode::Entity)).await?;

    round_trip_inet_and_macaddr(&db).await?;
    decode_inet_and_macaddr_as_tuple(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:exec.decode.types+1/test]
async fn round_trip_inet_and_macaddr(db: &DatabaseConnection) -> Result<(), DbErr> {
    for model in models() {
        let returned = model.clone().into_active_model().insert(db).await?;
        assert_eq!(returned, model);

        let fetched = net_decode::Entity::find_by_id(model.id).one(db).await?;
        assert_eq!(fetched, model);
    }

    assert_eq!(
        net_decode::Entity::find()
            .order_by_asc(net_decode::Column::Id)
            .all(db)
            .await?,
        models()
    );

    Ok(())
}

// [spec:pgorm:def:exec.decode.types+1/test]
async fn decode_inet_and_macaddr_as_tuple(db: &DatabaseConnection) -> Result<(), DbErr> {
    let decoded: Vec<(IpNetwork, MacAddress, Option<IpNetwork>)> = net_decode::Entity::find()
        .select_only()
        .column(net_decode::Column::Ip)
        .column(net_decode::Column::Mac)
        .column(net_decode::Column::Gateway)
        .order_by_asc(net_decode::Column::Id)
        .into_tuple()
        .all(db)
        .await?;

    assert_eq!(
        decoded,
        models()
            .into_iter()
            .map(|m| (m.ip, m.mac, m.gateway))
            .collect::<Vec<_>>()
    );

    Ok(())
}
