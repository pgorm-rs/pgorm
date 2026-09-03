use pgorm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
        let insert = Query::insert()
            .into_table(Cake::Table)
            .columns([Cake::Name])
            .values_panic(["Battenberg".into()])
            .to_owned();
        tx.execute(&insert.to_string(), &[]).await?;

        Err(Error::Custom("Abort migration".to_owned()))
    }
}

#[derive(DeriveIden)]
pub enum Cake {
    Table,
    Name,
}
