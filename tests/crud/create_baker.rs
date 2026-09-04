pub use super::*;
use pgorm::set;
use serde::{Deserialize, Serialize};

pub async fn test_create_baker(db: &DatabaseConnection) {
    let seaside_bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    };
    let bakery_insert_res = Bakery::insert(seaside_bakery)
        .exec(db)
        .await
        .expect("could not insert bakery");

    #[derive(Serialize, Deserialize)]
    struct ContactDetails {
        mobile: String,
        home: String,
        address: String,
    }

    let baker_bob_contact = ContactDetails {
        mobile: "+61424000000".to_owned(),
        home: "0395555555".to_owned(),
        address: "12 Test St, Testville, Vic, Australia".to_owned(),
    };
    let baker_bob = baker::ActiveModel {
        name: set("Baker Bob"),
        contact_details: set(serde_json::json!(baker_bob_contact)),
        bakery_id: set(Some(bakery_insert_res.last_insert_id)),
        ..Default::default()
    };
    let res = Baker::insert(baker_bob)
        .exec(db)
        .await
        .expect("could not insert baker");

    let baker: Option<baker::Model> = Baker::find_by_id(res.last_insert_id)
        .one_opt(db)
        .await
        .expect("could not find baker");

    assert!(baker.is_some());
    let baker_model = baker.unwrap();
    assert_eq!(baker_model.name, "Baker Bob");
    assert_eq!(
        baker_model.contact_details["mobile"],
        baker_bob_contact.mobile
    );
    assert_eq!(baker_model.contact_details["home"], baker_bob_contact.home);
    assert_eq!(
        baker_model.contact_details["address"],
        baker_bob_contact.address
    );
    assert_eq!(
        baker_model
            .find_related(Bakery)
            .one(db)
            .await
            .expect("Bakery not found")
            .name,
        "SeaSide Bakery"
    );

    let bakery: Option<bakery::Model> = Bakery::find_by_id(bakery_insert_res.last_insert_id)
        .one_opt(db)
        .await
        .unwrap();

    let related_bakers: Vec<baker::Model> = bakery
        .unwrap()
        .find_related(Baker)
        .all(db)
        .await
        .expect("could not find related bakers");
    assert_eq!(related_bakers.len(), 1);
    assert_eq!(related_bakers[0].name, "Baker Bob")
}
