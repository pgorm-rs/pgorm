use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "model", table_iden)]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    pub owner: String,
    pub name: String,
    pub description: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;
    use pgorm::*;

    #[test]
    fn test_columns_1() {
        assert_eq!(
            Column::iter()
                .map(|col| col.to_string())
                .collect::<Vec<_>>(),
            vec![
                "id".to_owned(),
                "owner".to_owned(),
                "name".to_owned(),
                "description".to_owned(),
            ]
        );
        assert_eq!(Column::Table.to_string().as_str(), "model");
        assert_eq!(Column::Id.to_string().as_str(), "id");
        assert_eq!(Column::Owner.to_string().as_str(), "owner");
        assert_eq!(Column::Name.to_string().as_str(), "name");
        assert_eq!(Column::Description.to_string().as_str(), "description");
    }
}
