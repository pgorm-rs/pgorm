use pgorm_migration::prelude::{pgorm_query::extension::Type, *};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
        let create = Type::create()
            .as_enum(Tea::Enum)
            .values([Tea::EverydayTea, Tea::BreakfastTea])
            .to_owned();
        tx.execute(&create.to_string(), &[]).await?;

        Ok(())
    }
}

// Variants are named after the SQL enum labels this migration creates.
#[allow(clippy::enum_variant_names)]
#[derive(DeriveIden)]
pub enum Tea {
    #[pgorm(iden = "tea")]
    Enum,
    #[pgorm(iden = "EverydayTea")]
    EverydayTea,
    #[pgorm(iden = "BreakfastTea")]
    BreakfastTea,
}
