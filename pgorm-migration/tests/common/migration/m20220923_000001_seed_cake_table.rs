use pgorm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let insert = Query::insert()
            .into_table(Cake::Table)
            .columns([Cake::Name])
            .values_panic(["Tiramisu".into()])
            .to_owned();

        manager.exec_stmt(insert).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let delete = Query::delete()
            .from_table(Cake::Table)
            .and_where(Expr::col(Cake::Name).eq("Tiramisu"))
            .to_owned();

        manager.exec_stmt(delete).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum Cake {
    Table,
    Name,
}
