use pgorm_migration::prelude::{pgorm_query::extension::Type, *};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
        let create = Type::create(Tea::Enum)
            .values(["EverydayTea", "BreakfastTea"])
            .to_owned();
        tx.execute(&create.to_string(), &[]).await?;

        Ok(())
    }
}

// The type is named by an `Iden`; its labels are data and are written as the
// string literals they render to.
#[derive(DeriveIden)]
pub enum Tea {
    #[pgorm(iden = "tea")]
    Enum,
}
