use pgorm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), DbErr> {
        let table = Table::create()
            .table(Cake::Table)
            .col(
                ColumnDef::new(Cake::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .col(ColumnDef::new(Cake::Name).string().not_null())
            .to_owned();
        tx.execute(&table.build(QueryBuilder), &[]).await?;

        let index = Index::create(Cake::Name)
            .name("cake_name_index")
            .table(Cake::Table)
            .to_owned();
        tx.execute(&index.build(QueryBuilder), &[]).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum Cake {
    Table,
    Id,
    Name,
}
