use super::m20220118_000001_create_cake_table::Cake;
use pgorm_migration::pgorm::DbBackend;
use pgorm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Fruit::Table)
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
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.database_backend() != DbBackend::Sqlite {
            manager
                .drop_foreign_key(
                    ForeignKey::drop()
                        .table(Fruit::Table)
                        .name("fk-fruit-cake_id")
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_table(Table::drop().table(Fruit::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Fruit {
    Table,
    Id,
    Name,
    CakeId,
}
