use pgorm_migration::pgorm::{ConnectionTrait, DbBackend};
use pgorm_migration::prelude::{pgorm_query::extension::postgres::Type, *};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        match db.get_database_backend() {
            DbBackend::MySql | DbBackend::Sqlite => {}
            DbBackend::Postgres => {
                manager
                    .create_type(
                        Type::create()
                            .as_enum(Tea::Enum)
                            .values([Tea::EverydayTea, Tea::BreakfastTea])
                            .to_owned(),
                    )
                    .await?;
            }
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        match db.get_database_backend() {
            DbBackend::MySql | DbBackend::Sqlite => {}
            DbBackend::Postgres => {
                manager
                    .drop_type(Type::drop().name(Tea::Enum).to_owned())
                    .await?;
            }
        }

        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum Tea {
    #[pgorm(iden = "tea")]
    Enum,
    #[pgorm(iden = "EverydayTea")]
    EverydayTea,
    #[pgorm(iden = "BreakfastTea")]
    BreakfastTea,
}
