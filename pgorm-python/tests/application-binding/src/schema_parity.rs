use crate::account;
use pgorm::Schema;
use pgorm_python::expressions::Compiled;
use pyo3::prelude::*;

// [spec:pgorm:req:python.schema/test]
#[test]
fn schema_generation_matches_real_entity_traits() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "pgorm._native")?;
        crate::_native(&module)?;
        let entity = module.call_method1("entity", ("app.Account",))?;
        let generated = module.call_method1("schema_from_entity", (&entity,))?;
        let schema = Schema::new();
        let table: Compiled = generated
            .getattr("table")?
            .call_method0("inspect")?
            .extract()?;
        assert_eq!(
            table.sql,
            schema.create_table_from_entity(account::Entity).to_string()
        );
        assert!(table.values.0.is_empty());
        let expected = [
            (
                "enums",
                schema
                    .create_enum_from_entity(account::Entity)
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ),
            (
                "indexes",
                schema
                    .create_index_from_entity(account::Entity)
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ),
            (
                "comments",
                schema
                    .create_comments_from_entity(account::Entity)
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ),
        ];
        for (field, statements) in expected {
            assert!(!statements.is_empty(), "fixture must exercise {field}");
            let native = generated.getattr(field)?;
            assert_eq!(native.len()?, statements.len());
            for (i, expected) in statements.iter().enumerate() {
                let ddl = native.get_item(i)?;
                let compiled = pgorm_python::statements::compile(&ddl)?;
                assert_eq!(&compiled.sql, expected);
                assert!(compiled.values.0.is_empty());
            }
        }
        Ok(())
    })
}
