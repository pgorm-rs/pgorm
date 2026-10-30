use pyo3::prelude::*;

use crate::PgormError;

pyo3::create_exception!(pgorm, ConstructionError, PgormError);
pyo3::create_exception!(pgorm, ConnectionError, PgormError);
pyo3::create_exception!(pgorm, DatabaseError, PgormError);
pyo3::create_exception!(pgorm, DecodeError, PgormError);
pyo3::create_exception!(pgorm, TimeoutError, PgormError);
pyo3::create_exception!(pgorm, LifecycleError, PgormError);
pyo3::create_exception!(pgorm, InternalError, PgormError);
pyo3::create_exception!(pgorm, UniqueViolationError, DatabaseError);
pyo3::create_exception!(pgorm, ForeignKeyViolationError, DatabaseError);
pyo3::create_exception!(pgorm, CheckViolationError, DatabaseError);
pyo3::create_exception!(pgorm, NotNullViolationError, DatabaseError);
pyo3::create_exception!(pgorm, ExclusionViolationError, DatabaseError);

/// Connection secrets must never enter diagnostics, including nested causes.
#[derive(Clone, Default)]
pub(crate) struct Redactions(Vec<String>);

impl Redactions {
    pub(crate) fn from_config(config: &pgorm::Config) -> Self {
        let mut values = Vec::new();
        if let Some(user) = config.get_user().filter(|s| !s.is_empty()) {
            values.push(user.to_owned());
        }
        if let Some(password) = config.get_password().filter(|s| !s.is_empty()) {
            values.push(String::from_utf8_lossy(password).into_owned());
        }
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        Self(values)
    }

    fn apply(&self, input: &str) -> String {
        self.0.iter().fold(input.to_owned(), |text, secret| {
            text.replace(secret, "[redacted]")
        })
    }
}

// [spec:pgorm:req:bindings.errors+2]
// [spec:pgorm:req:python.row-count+2]
pub(crate) fn database_error(error: pgorm::Error, secrets: &Redactions) -> PyErr {
    let mut source: &(dyn std::error::Error + 'static) = &error;
    loop {
        if let Some(postgres) = source.downcast_ref::<tokio_postgres::Error>() {
            return postgres_error(postgres, secrets);
        }
        match source.source() {
            Some(next) => source = next,
            None => break,
        }
    }
    match error {
        pgorm::Error::Pool(_) => ConnectionError::new_err("PostgreSQL pool acquisition failed"),
        // [spec:pgorm:req:bindings.listen+2]
        pgorm::Error::Listen(pgorm::ListenError::ChannelTooLong { .. }) => {
            ConstructionError::new_err(secrets.apply(&error.to_string()))
        }
        pgorm::Error::Listen(_) => ConnectionError::new_err(secrets.apply(&error.to_string())),
        pgorm::Error::Conversion { .. } | pgorm::Error::Type(_) | pgorm::Error::Json(_) => {
            DecodeError::new_err(secrets.apply(&error.to_string()))
        }
        _ => DatabaseError::new_err(secrets.apply(&error.to_string())),
    }
}

fn postgres_error(error: &tokio_postgres::Error, secrets: &Redactions) -> PyErr {
    let Some(db) = error.as_db_error() else {
        return client_error(error, secrets);
    };
    let message = secrets.apply(db.message());
    // [spec:pgorm:req:bindings.errors+2]    the constraint classes pgorm's SqlError names
    let exception = match db.code().code() {
        "23505" => UniqueViolationError::new_err(message.clone()),
        "23503" => ForeignKeyViolationError::new_err(message.clone()),
        "23514" => CheckViolationError::new_err(message.clone()),
        "23502" => NotNullViolationError::new_err(message.clone()),
        "23P01" => ExclusionViolationError::new_err(message.clone()),
        _ => DatabaseError::new_err(message.clone()),
    };
    let decorated = Python::attach(|py| -> PyResult<()> {
        let value = exception.value(py);
        value.setattr("sqlstate", db.code().code())?;
        value.setattr("message", message)?;
        value.setattr("severity", db.severity())?;
        for (field, content) in [
            ("detail", db.detail()),
            ("hint", db.hint()),
            ("schema", db.schema()),
            ("table", db.table()),
            ("column", db.column()),
            ("datatype", db.datatype()),
            ("constraint", db.constraint()),
        ] {
            value.setattr(field, content.map(|text| secrets.apply(text)))?;
        }
        Ok(())
    });
    match decorated {
        Ok(()) => exception,
        Err(_) => InternalError::new_err("could not attach PostgreSQL diagnostics"),
    }
}

fn client_error(error: &tokio_postgres::Error, secrets: &Redactions) -> PyErr {
    // Pinned tokio-postgres 0.7.18 exposes no public error-kind accessor for
    // FromSql/ToSql failures. Its top-level Display identifies the phase;
    // inspect that before redaction, never a user-controlled nested cause.
    // Keep the original Rust error intact until this Python boundary so
    // application hooks can still match pgorm::Error::Postgres.
    let phase = error.to_string();
    if phase.starts_with("error deserializing column ")
        || phase.starts_with("invalid column `")
        || phase == "query returned an unexpected number of columns"
    {
        DecodeError::new_err(secrets.apply(&phase))
    } else if phase.starts_with("error serializing parameter ") {
        ConstructionError::new_err(secrets.apply(&phase))
    } else {
        ConnectionError::new_err("PostgreSQL connection or protocol failure")
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    // Client-side cardinality errors have no server diagnostics. Keep the
    // documented optional fields present for every DatabaseError instance.
    let database_error = py.get_type::<DatabaseError>();
    for field in [
        "sqlstate",
        "message",
        "severity",
        "detail",
        "hint",
        "schema",
        "table",
        "column",
        "datatype",
        "constraint",
    ] {
        database_error.setattr(field, py.None())?;
    }
    module.add("ConstructionError", py.get_type::<ConstructionError>())?;
    module.add("ConnectionError", py.get_type::<ConnectionError>())?;
    module.add("DatabaseError", database_error)?;
    module.add("DecodeError", py.get_type::<DecodeError>())?;
    module.add("TimeoutError", py.get_type::<TimeoutError>())?;
    module.add("LifecycleError", py.get_type::<LifecycleError>())?;
    module.add("InternalError", py.get_type::<InternalError>())?;
    module.add(
        "UniqueViolationError",
        py.get_type::<UniqueViolationError>(),
    )?;
    module.add(
        "ForeignKeyViolationError",
        py.get_type::<ForeignKeyViolationError>(),
    )?;
    module.add("CheckViolationError", py.get_type::<CheckViolationError>())?;
    module.add(
        "NotNullViolationError",
        py.get_type::<NotNullViolationError>(),
    )?;
    module.add(
        "ExclusionViolationError",
        py.get_type::<ExclusionViolationError>(),
    )?;
    module.add(
        "CancelledError",
        py.import("asyncio")?.getattr("CancelledError")?,
    )?;
    Ok(())
}
