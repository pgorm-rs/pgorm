//! An application crate, apart from the binding's own workspace, that
//! registers its entities with pgorm-napi and builds the one native module
//! JavaScript loads: the binding's whole API and these registrations
//! together, as pgorm-python's application module is built.

pub mod account;
pub mod graphs;
pub mod membership;
pub mod note;

#[cfg(test)]
mod parity;

use neon::prelude::*;
use pgorm_napi::{RegistrationError, Registry};

/// The application's registrations.
// [spec:pgorm:req:napi.entities/test]
pub fn registry() -> Result<Registry, RegistrationError> {
    let mut registry = Registry::default();
    registry.entity::<account::Entity>("app.Account")?;
    registry.entity::<note::Entity>("app.Note")?;
    registry.entity::<membership::Entity>("app.Membership")?;
    graphs::register(&mut registry)?;
    Ok(registry)
}

// [spec:pgorm:req:napi.application/test]
#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    let registry = match registry() {
        Ok(registry) => registry,
        Err(error) => return cx.throw_error(error.to_string()),
    };
    pgorm_napi::install(&mut cx, registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgorm:req:napi.entities/test]
    #[test]
    fn registry_refuses_duplicate_names_and_entities() -> Result<(), RegistrationError> {
        let mut registry = Registry::default();
        registry.entity::<account::Entity>("app.Account")?;
        assert!(registry.entity::<note::Entity>("app.Account").is_err());
        assert!(registry.entity::<account::Entity>("again.Account").is_err());
        assert!(registry.entity::<note::Entity>("").is_err());
        let unregistered = registry.graph::<note::Entity, (), _>("app.NoteOnly", |_| {
            <note::Entity as pgorm::EntityTrait>::graph()
        });
        assert!(unregistered.is_err());
        registry.entity::<note::Entity>("app.Note")?;
        graphs::register(&mut Registry::default()).expect_err("graphs need their entities");
        assert!(
            registry
                .graph::<note::Entity, (), _>("app.NoteOnly", |_| {
                    <note::Entity as pgorm::EntityTrait>::graph()
                })
                .is_ok()
        );
        assert!(
            registry
                .graph::<note::Entity, (), _>("app.NoteOnly", |_| {
                    <note::Entity as pgorm::EntityTrait>::graph()
                })
                .is_err()
        );
        Ok(())
    }
}
