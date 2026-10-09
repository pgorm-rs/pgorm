//! The source tuples the application registers for its pipelines'
//! `select_sources`: one account alone, an account with a note, and the
//! widest tuple Rust admits.

use pgorm_napi::{RegistrationError, Registry};

use crate::{account::Entity as A, note::Entity as N};

/// Register every tuple, once its entities are registered.
// [spec:pgorm:req:napi.pipeline-sources/test]
pub fn register(registry: &mut Registry) -> Result<(), RegistrationError> {
    registry.sources::<(A,)>("app.SingleAccount")?;
    registry.sources::<(A, N)>("app.AccountWithNote")?;
    registry.sources::<(A, N, N, N, N, N)>("app.SixSources")?;
    Ok(())
}
