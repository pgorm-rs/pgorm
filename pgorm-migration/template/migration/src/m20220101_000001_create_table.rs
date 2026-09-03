use pgorm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // Replace the sample below with your own migration scripts.
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), DbErr> {
        let table = Table::create(Post::Table)
            .if_not_exists()
            .col(
                ColumnDef::new(Post::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .col(ColumnDef::new(Post::Title).string().not_null())
            .col(ColumnDef::new(Post::Text).string().not_null())
            .to_owned();
        tx.execute(&table.build(), &[]).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Post {
    Table,
    Id,
    Title,
    Text,
}
