use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use pyo3::PyResult;
use rustls::pki_types::{CertificateDer, pem::PemObject};
use tokio_postgres::config::SslMode;
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::errors::{ConstructionError, InternalError, Redactions};

pub(crate) struct PoolConfig {
    pub(crate) dsn: String,
    pub(crate) tls: Option<String>,
    pub(crate) ca_pem: Option<Vec<u8>>,
    pub(crate) max_size: usize,
    pub(crate) connect_timeout: f64,
    pub(crate) acquire_timeout: f64,
    pub(crate) statement_cache_size: usize,
    pub(crate) recycle: String,
}

pub(crate) fn seconds(value: f64, name: &str) -> PyResult<Duration> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ConstructionError::new_err(format!(
            "{name} must be positive and finite"
        )));
    }
    Duration::try_from_secs_f64(value)
        .map_err(|_| ConstructionError::new_err(format!("{name} is outside the supported range")))
}

// [spec:pgorm:req:python.connections]
impl PoolConfig {
    pub(crate) fn build(self) -> PyResult<(pgorm::DatabasePool, Redactions, Duration)> {
        if self.max_size == 0 || self.max_size > tokio::sync::Semaphore::MAX_PERMITS {
            return Err(ConstructionError::new_err(
                "max_size is outside the supported range",
            ));
        }
        let mut config: pgorm::Config = self.dsn.parse().map_err(|_| {
            ConstructionError::new_err("invalid PostgreSQL connection configuration")
        })?;
        config.connect_timeout(seconds(self.connect_timeout, "connect_timeout")?);
        let acquire_timeout = seconds(self.acquire_timeout, "acquire_timeout")?;
        let secrets = Redactions::from_config(&config);
        let mode = self.tls.as_deref().unwrap_or(match config.get_ssl_mode() {
            SslMode::Disable => "disable",
            _ => "verify-full",
        });
        let recycling_method = match self.recycle.as_str() {
            "verified" => pgorm::RecyclingMethod::Verified,
            "fast" => pgorm::RecyclingMethod::Fast,
            _ => {
                return Err(ConstructionError::new_err(
                    "recycle must be 'verified' or 'fast'",
                ));
            }
        };
        let manager = pgorm::ManagerConfig {
            recycling_method,
            tag: None,
            statement_cache: match NonZeroUsize::new(self.statement_cache_size) {
                None => pgorm::StatementCacheSize::Disabled,
                Some(size) => pgorm::StatementCacheSize::Bounded(size),
            },
        };
        let pool = match mode {
            "disable" => {
                if self.ca_pem.is_some() || config.get_ssl_mode() == SslMode::Require {
                    return Err(ConstructionError::new_err(
                        "TLS configuration conflicts with tls='disable'",
                    ));
                }
                config.ssl_mode(SslMode::Disable);
                pgorm::connect_with(config, pgorm::NoTls, manager, |b| b.max_size(self.max_size))
            }
            "verify-full" => {
                config.ssl_mode(SslMode::Require);
                let connector = tls_connector(self.ca_pem.as_deref())?;
                pgorm::connect_with(config, connector, manager, |b| b.max_size(self.max_size))
            }
            _ => {
                return Err(ConstructionError::new_err(
                    "tls must be 'verify-full' or 'disable'",
                ));
            }
        }
        .map_err(|_| ConstructionError::new_err("invalid PostgreSQL pool configuration"))?;
        Ok((pool, secrets, acquire_timeout))
    }
}

/// A rustls connector trusting `ca_pem` when given, and the platform's trust
/// store otherwise.
fn tls_connector(ca_pem: Option<&[u8]>) -> PyResult<MakeRustlsConnect> {
    let mut roots = rustls::RootCertStore::empty();
    if let Some(pem) = ca_pem {
        let certificates = CertificateDer::pem_slice_iter(pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ConstructionError::new_err("invalid PEM certificate data"))?;
        if certificates.is_empty() {
            return Err(ConstructionError::new_err(
                "CA data contains no certificates",
            ));
        }
        for certificate in certificates {
            roots
                .add(certificate)
                .map_err(|_| ConstructionError::new_err("invalid CA certificate"))?;
        }
    } else {
        platform_roots(&mut roots)?;
    }
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| InternalError::new_err("TLS protocol initialization failed"))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(MakeRustlsConnect::new(client))
}

/// Adds the platform's trusted roots to `roots`: the system keychain's on
/// macOS, the system store's on Windows, and elsewhere the CA bundle and
/// directory OpenSSL would use. `SSL_CERT_FILE` and `SSL_CERT_DIR`, when set,
/// replace the platform's store entirely.
///
/// A store that yields no usable root fails here, at construction, naming
/// what the loader reported; it never leaves a pool that trusts nothing and
/// so fails every connection later with an unexplained certificate error.
/// The loader also reports conditions that are routine on a working host,
/// such as macOS having no per-user trust settings, so its errors are only
/// surfaced when no root was found.
///
/// The load reads the process environment, so it runs with the interpreter
/// attached: Python code cannot then rewrite `os.environ` underneath it.
fn platform_roots(roots: &mut rustls::RootCertStore) -> PyResult<()> {
    let found = rustls_native_certs::load_native_certs();
    let (added, _) = roots.add_parsable_certificates(found.certs);
    if added > 0 {
        return Ok(());
    }
    let reported = found
        .errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let detail = if reported.is_empty() {
        String::new()
    } else {
        format!(" ({})", reported.join("; "))
    };
    let replaced = std::env::var_os("SSL_CERT_FILE").is_some()
        || std::env::var_os("SSL_CERT_DIR").is_some_and(|dirs| {
            std::env::split_paths(&dirs).any(|dir| !dir.as_os_str().is_empty())
        });
    Err(ConstructionError::new_err(if replaced {
        format!(
            "SSL_CERT_FILE and SSL_CERT_DIR, which replace the platform's certificate store, \
             name no usable root certificate{detail}; pass cafile, or point them at a CA bundle"
        )
    } else {
        format!(
            "the platform's certificate store holds no usable root certificate{detail}; \
             pass cafile, or point SSL_CERT_FILE or SSL_CERT_DIR at a CA bundle"
        )
    }))
}
