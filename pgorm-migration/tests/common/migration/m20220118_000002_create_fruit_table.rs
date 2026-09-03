use super::m20220118_000001_create_cake_table::Cake;
use pgorm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), DbErr> {
        let table = Table::create(Fruit::Table)
            .col(
                ColumnDef::new(Fruit::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .col(ColumnDef::new(Fruit::Name).string().not_null())
            .col(ColumnDef::new(Fruit::CakeId).integer().not_null())
            .foreign_key(
                ForeignKey::create()
                    .name("fk-fruit-cake_id")
                    .from(Fruit::Table, Fruit::CakeId)
                    .to(Cake::Table, Cake::Id),
            )
            .to_owned();
        tx.execute(&table.to_string(), &[]).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum Fruit {
    Table,
    Id,
    Name,
    CakeId,
}
