//! A pool's configuration, read from the connection string and the options
//! JavaScript passes, and the TLS connector it connects through.

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use neon::prelude::*;
use rustls::pki_types::{CertificateDer, pem::PemObject};
use tokio_postgres::config::SslMode;
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::{
    errors::{Failure, Redactions},
    values::read::{refuse, string},
};

/// What a pool is built from.
#[derive(Debug)]
pub(crate) struct PoolConfig {
    pub(crate) dsn: String,
    pub(crate) tls: Option<String>,
    pub(crate) ca: Option<Vec<u8>>,
    pub(crate) max_size: usize,
    pub(crate) connect_timeout: Duration,
    pub(crate) acquire_timeout: Duration,
    pub(crate) statement_cache_size: usize,
    pub(crate) recycle: String,
}

/// A built pool, the secrets its errors redact, and how long acquiring a
/// connection may wait.
pub(crate) struct Built {
    pub(crate) pool: pgorm::DatabasePool,
    pub(crate) secrets: Redactions,
    pub(crate) acquire_timeout: Duration,
}

fn option<'cx>(
    cx: &mut Cx<'cx>,
    options: Handle<'cx, JsObject>,
    name: &str,
) -> NeonResult<Option<Handle<'cx, JsValue>>> {
    let value: Handle<JsValue> = options.get(cx, name)?;
    Ok(if value.is_a::<JsUndefined, _>(cx) {
        None
    } else {
        Some(value)
    })
}

/// A count option: a non-negative safe integer.
fn count<'cx>(
    cx: &mut Cx<'cx>,
    options: Handle<'cx, JsObject>,
    name: &str,
    default: usize,
) -> NeonResult<usize> {
    let Some(value) = option(cx, options, name)? else {
        return Ok(default);
    };
    let number = match value.downcast::<JsNumber, _>(cx) {
        Ok(number) => number.value(cx),
        Err(_) => return refuse(cx, format!("{name} is a number")),
    };
    if number.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&number) {
        return refuse(cx, format!("{name} is a non-negative integer"));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(number as usize)
}

/// A duration option, in milliseconds: positive and finite.
fn milliseconds<'cx>(
    cx: &mut Cx<'cx>,
    options: Handle<'cx, JsObject>,
    name: &str,
    default: Duration,
) -> NeonResult<Duration> {
    let Some(value) = option(cx, options, name)? else {
        return Ok(default);
    };
    let number = match value.downcast::<JsNumber, _>(cx) {
        Ok(number) => number.value(cx),
        Err(_) => return refuse(cx, format!("{name} is a number of milliseconds")),
    };
    match Duration::try_from_secs_f64(number / 1000.0) {
        Ok(duration) if number > 0.0 => Ok(duration),
        _ => refuse(
            cx,
            format!("{name} is a positive, finite number of milliseconds"),
        ),
    }
}

impl PoolConfig {
    /// `dsn` and the pool options object, as JavaScript passes them.
    // [spec:pgorm:req:napi.connections]
    pub(crate) fn read<'cx>(
        cx: &mut Cx<'cx>,
        dsn: Handle<'cx, JsValue>,
        options: Handle<'cx, JsObject>,
    ) -> NeonResult<Self> {
        let dsn = string(cx, dsn)?;
        let tls = match option(cx, options, "tls")? {
            Some(tls) => Some(string(cx, tls)?),
            None => None,
        };
        let ca = match option(cx, options, "ca")? {
            None => None,
            Some(ca) if ca.is_a::<JsString, _>(cx) => Some(string(cx, ca)?.into_bytes()),
            Some(ca) => match ca.downcast::<JsUint8Array, _>(cx) {
                Ok(bytes) => {
                    use neon::types::buffer::TypedArray;
                    Some(bytes.as_slice(cx).to_vec())
                }
                Err(_) => return refuse(cx, "ca is PEM text, as a string or a Uint8Array"),
            },
        };
        let recycle = match option(cx, options, "recycle")? {
            Some(recycle) => string(cx, recycle)?,
            None => "verified".to_owned(),
        };
        Ok(Self {
            dsn,
            tls,
            ca,
            max_size: count(cx, options, "maxSize", 10)?,
            connect_timeout: milliseconds(cx, options, "connectTimeout", Duration::from_secs(10))?,
            acquire_timeout: milliseconds(cx, options, "acquireTimeout", Duration::from_secs(30))?,
            statement_cache_size: count(cx, options, "statementCacheSize", 128)?,
            recycle,
        })
    }

    /// The pool. TLS verifies the server's certificate and host name unless
    /// the connection string says `sslmode=disable` or `tls` is `"disable"`;
    /// there is no silent fallback to plaintext.
    // [spec:pgorm:req:napi.connections]
    pub(crate) fn build(self) -> Result<Built, Failure> {
        let invalid = |message: &str| Failure::Construction(message.to_owned());
        if self.max_size == 0 || self.max_size > tokio::sync::Semaphore::MAX_PERMITS {
            return Err(invalid("maxSize is outside the supported range"));
        }
        let mut config: pgorm::Config = self
            .dsn
            .parse()
            .map_err(|_| invalid("invalid PostgreSQL connection string"))?;
        if config.get_hosts().is_empty() {
            return Err(invalid("the connection string names no host"));
        }
        config.connect_timeout(self.connect_timeout);
        let secrets = Redactions::from_config(&config);
        let mode = match self.tls.as_deref() {
            Some(mode) => mode,
            None if config.get_ssl_mode() == SslMode::Disable => "disable",
            None => "verify-full",
        };
        let recycling_method = match self.recycle.as_str() {
            "verified" => pgorm::RecyclingMethod::Verified,
            "fast" => pgorm::RecyclingMethod::Fast,
            _ => return Err(invalid("recycle is \"verified\" or \"fast\"")),
        };
        let manager = pgorm::ManagerConfig {
            recycling_method,
            tag: None,
            statement_cache: match NonZeroUsize::new(self.statement_cache_size) {
                None => pgorm::StatementCacheSize::Disabled,
                Some(size) => pgorm::StatementCacheSize::Bounded(size),
            },
        };
        let max_size = self.max_size;
        let pool = match mode {
            "disable" => {
                if self.ca.is_some() || config.get_ssl_mode() == SslMode::Require {
                    return Err(invalid(
                        "a CA or sslmode=require conflicts with tls \"disable\"",
                    ));
                }
                config.ssl_mode(SslMode::Disable);
                pgorm::connect_with(config, pgorm::NoTls, manager, |builder| {
                    builder.max_size(max_size)
                })
            }
            "verify-full" => {
                config.ssl_mode(SslMode::Require);
                let connector = tls_connector(self.ca.as_deref())?;
                pgorm::connect_with(config, connector, manager, |builder| {
                    builder.max_size(max_size)
                })
            }
            _ => return Err(invalid("tls is \"verify-full\" or \"disable\"")),
        }
        .map_err(|_| invalid("invalid PostgreSQL pool configuration"))?;
        Ok(Built {
            pool,
            secrets,
            acquire_timeout: self.acquire_timeout,
        })
    }
}

/// A rustls connector trusting `ca` when given, and the platform's trust
/// store otherwise.
fn tls_connector(ca: Option<&[u8]>) -> Result<MakeRustlsConnect, Failure> {
    let invalid = |message: &str| Failure::Construction(message.to_owned());
    let mut roots = rustls::RootCertStore::empty();
    if let Some(pem) = ca {
        let certificates = CertificateDer::pem_slice_iter(pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid("ca holds invalid PEM certificate data"))?;
        if certificates.is_empty() {
            return Err(invalid("ca holds no certificate"));
        }
        for certificate in certificates {
            roots
                .add(certificate)
                .map_err(|_| invalid("ca holds a certificate rustls cannot use"))?;
        }
    } else {
        let found = rustls_native_certs::load_native_certs();
        let (added, _) = roots.add_parsable_certificates(found.certs);
        if added == 0 {
            return Err(invalid(
                "the platform's trust store holds no certificate to verify a server with: pass ca",
            ));
        }
    }
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| Failure::Internal("TLS protocol initialization failed".to_owned()))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(MakeRustlsConnect::new(client))
}
