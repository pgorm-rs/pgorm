use pgorm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), DbErr> {
        let table = Table::create(Cake::Table)
            .col(
                ColumnDef::new(Cake::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .col(ColumnDef::new(Cake::Name).string().not_null())
            .to_owned();
        tx.execute(&table.to_string(), &[]).await?;

        let index = Index::create(Cake::Table, Cake::Name)
            .name("cake_name_index")
            .to_owned();
        tx.execute(&index.to_string(), &[]).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum Cake {
    Table,
    Id,
    Name,
}
