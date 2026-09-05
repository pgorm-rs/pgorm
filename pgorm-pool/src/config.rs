//! Configuration used for [`Pool`] creation.

use std::{
    borrow::Cow,
    env, fmt,
    net::IpAddr,
    num::NonZeroUsize,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use tokio_postgres::config::{
    ChannelBinding as PgChannelBinding, LoadBalanceHosts as PgLoadBalanceHosts,
    SslMode as PgSslMode, TargetSessionAttrs as PgTargetSessionAttrs,
};

#[cfg(not(target_arch = "wasm32"))]
use super::Pool;
#[cfg(not(target_arch = "wasm32"))]
use crate::{CreatePoolError, PoolBuilder};
#[cfg(not(target_arch = "wasm32"))]
use tokio_postgres::{
    Socket,
    tls::{MakeTlsConnect, TlsConnect},
};

use super::PoolConfig;

/// Configuration object.
///
/// # Example (from environment)
///
/// By enabling the `serde` feature you can read the configuration using the
/// [`config`](https://crates.io/crates/config) crate as following:
/// ```env
/// PG__HOST=pg.example.com
/// PG__USER=john_doe
/// PG__PASSWORD=topsecret
/// PG__DBNAME=example
/// PG__POOL__MAX_SIZE=16
/// PG__POOL__TIMEOUTS__WAIT__SECS=5
/// PG__POOL__TIMEOUTS__WAIT__NANOS=0
/// ```
/// ```rust
/// #[derive(serde::Deserialize, serde::Serialize)]
/// struct Config {
///     pg: pgorm_pool::Config,
/// }
/// impl Config {
///     pub fn from_env() -> Result<Self, config::ConfigError> {
///         let mut cfg = config::Config::builder()
///            .add_source(config::Environment::default().separator("__"))
///            .build()?;
///            cfg.try_deserialize()
///     }
/// }
/// ```
#[derive(Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct Config {
    /// Initialize the configuration by parsing the URL first.
    /// **Note**: All the other options override settings defined
    /// by the URL except for the `host` and `hosts` options which
    /// are additive!
    pub url: Option<String>,
    /// See [`tokio_postgres::Config::user`].
    pub user: Option<String>,
    /// See [`tokio_postgres::Config::password`].
    pub password: Option<String>,
    /// See [`tokio_postgres::Config::dbname`].
    pub dbname: Option<String>,
    /// See [`tokio_postgres::Config::options`].
    pub options: Option<String>,
    /// See [`tokio_postgres::Config::application_name`].
    pub application_name: Option<String>,
    /// See [`tokio_postgres::Config::ssl_mode`].
    pub ssl_mode: Option<SslMode>,
    /// This is similar to [`Config::hosts`] but only allows one host to be
    /// specified.
    ///
    /// Unlike [`tokio_postgres::Config`] this structure differentiates between
    /// one host and more than one host. This makes it possible to store this
    /// configuration in an environment variable.
    ///
    /// See [`tokio_postgres::Config::host`].
    pub host: Option<String>,
    /// See [`tokio_postgres::Config::host`].
    pub hosts: Option<Vec<String>>,
    /// See [`tokio_postgres::Config::hostaddr`].
    pub hostaddr: Option<IpAddr>,
    /// See [`tokio_postgres::Config::hostaddr`].
    pub hostaddrs: Option<Vec<IpAddr>>,
    /// This is similar to [`Config::ports`] but only allows one port to be
    /// specified.
    ///
    /// Unlike [`tokio_postgres::Config`] this structure differentiates between
    /// one port and more than one port. This makes it possible to store this
    /// configuration in an environment variable.
    ///
    /// See [`tokio_postgres::Config::port`].
    pub port: Option<u16>,
    /// See [`tokio_postgres::Config::port`].
    pub ports: Option<Vec<u16>>,
    /// See [`tokio_postgres::Config::connect_timeout`].
    pub connect_timeout: Option<Duration>,
    /// See [`tokio_postgres::Config::keepalives`].
    pub keepalives: Option<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    /// See [`tokio_postgres::Config::keepalives_idle`].
    pub keepalives_idle: Option<Duration>,
    /// See [`tokio_postgres::Config::target_session_attrs`].
    pub target_session_attrs: Option<TargetSessionAttrs>,
    /// See [`tokio_postgres::Config::channel_binding`].
    pub channel_binding: Option<ChannelBinding>,
    /// See [`tokio_postgres::Config::load_balance_hosts`].
    pub load_balance_hosts: Option<LoadBalanceHosts>,

    /// [`Manager`] configuration.
    ///
    /// [`Manager`]: super::Manager
    pub manager: Option<ManagerConfig>,

    /// [`Pool`] configuration.
    pub pool: Option<PoolConfig>,
}

/// A value the [`fmt::Debug`] impl refuses to print, spelled `_` as
/// [`tokio_postgres::Config`] spells it.
struct Redaction;

impl fmt::Debug for Redaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("_")
    }
}

/// [`Config::url`] with its credentials removed.
///
/// The field holds whatever [`tokio_postgres::Config::from_str`] accepts, which
/// is two spellings. In a URL the credentials are the `userinfo` before the
/// authority's `@`, and everything after it — host, port, database, options —
/// is what makes the printed value worth having, so only the `userinfo` is
/// replaced. A libpq keyword/value string has no such delimiter and its quoting
/// and escaping are not worth re-implementing to find one, so a string that
/// mentions `password` at all is withheld whole.
// [spec:pgorm:sem:conn.pool.config-redaction]    url credentials
fn redact_url(url: &str) -> Cow<'_, str> {
    let Some(scheme_end) = url.find("://") else {
        return if url.contains("password") {
            Cow::Borrowed("_")
        } else {
            Cow::Borrowed(url)
        };
    };

    let authority_start = scheme_end + "://".len();
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |offset| authority_start + offset);

    match url[authority_start..authority_end].rfind('@') {
        Some(at) => Cow::Owned(format!(
            "{}_{}",
            &url[..authority_start],
            &url[authority_start + at..]
        )),
        None => Cow::Borrowed(url),
    }
}

/// Hand-written rather than derived because two fields carry credentials, and
/// the type is built to be deserialized from the environment — the crate's own
/// example fills it from `PG__PASSWORD` — so a derived impl would put the
/// password in whatever log a `?cfg` reaches.
// [spec:pgorm:sem:conn.pool.config-redaction]
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut config_dbg = &mut f.debug_struct("Config");
        config_dbg = config_dbg
            .field("url", &self.url.as_deref().map(redact_url))
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| Redaction))
            .field("dbname", &self.dbname)
            .field("options", &self.options)
            .field("application_name", &self.application_name)
            .field("ssl_mode", &self.ssl_mode)
            .field("host", &self.host)
            .field("hosts", &self.hosts)
            .field("hostaddr", &self.hostaddr)
            .field("hostaddrs", &self.hostaddrs)
            .field("port", &self.port)
            .field("ports", &self.ports)
            .field("connect_timeout", &self.connect_timeout)
            .field("keepalives", &self.keepalives);

        #[cfg(not(target_arch = "wasm32"))]
        {
            config_dbg = config_dbg.field("keepalives_idle", &self.keepalives_idle);
        }

        config_dbg
            .field("target_session_attrs", &self.target_session_attrs)
            .field("channel_binding", &self.channel_binding)
            .field("load_balance_hosts", &self.load_balance_hosts)
            .field("manager", &self.manager)
            .field("pool", &self.pool)
            .finish()
    }
}

/// This error is returned if there is something wrong with the configuration
#[derive(Debug)]
pub enum ConfigError {
    /// This variant is returned if the `url` is invalid
    InvalidUrl(tokio_postgres::Error),
    /// This variant is returned if the `dbname` is missing from the config
    DbnameMissing,
    /// This variant is returned if the `dbname` contains an empty string
    DbnameEmpty,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(e) => write!(f, "configuration property \"url\" is invalid: {}", e),
            Self::DbnameMissing => write!(f, "configuration property \"dbname\" not found"),
            Self::DbnameEmpty => write!(
                f,
                "configuration property \"dbname\" contains an empty string",
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Create a new [`Config`] instance with default values. This function is
    /// identical to [`Config::default()`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Creates a new [`Pool`] using this [`Config`].
    ///
    /// # Errors
    ///
    /// See [`CreatePoolError`] for details.
    pub fn create_pool<T>(&self, tls: T) -> Result<Pool, CreatePoolError>
    where
        T: MakeTlsConnect<Socket> + Clone + Sync + Send + 'static,
        T::Stream: Sync + Send,
        T::TlsConnect: Sync + Send,
        <T::TlsConnect as TlsConnect<Socket>>::Future: Send,
    {
        use deadpool::Runtime;

        let builder = self.builder(tls).map_err(CreatePoolError::Config)?;
        builder
            .runtime(Runtime::Tokio1)
            .build()
            .map_err(CreatePoolError::Build)
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Creates a new [`PoolBuilder`] using this [`Config`].
    ///
    /// # Errors
    ///
    /// See [`ConfigError`] and [`tokio_postgres::Error`] for details.
    pub fn builder<T>(&self, tls: T) -> Result<PoolBuilder, ConfigError>
    where
        T: MakeTlsConnect<Socket> + Clone + Sync + Send + 'static,
        T::Stream: Sync + Send,
        T::TlsConnect: Sync + Send,
        <T::TlsConnect as TlsConnect<Socket>>::Future: Send,
    {
        let pg_config = self.get_pg_config()?;
        let manager_config = self.get_manager_config();
        let manager = crate::Manager::from_config(pg_config, tls, manager_config);
        let pool_config = self.get_pool_config();
        Ok(Pool::builder(manager).config(pool_config))
    }

    /// Returns [`tokio_postgres::Config`] which can be used to connect to
    /// the database.
    ///
    /// Every field this struct declares is applied here. A field carried but
    /// never forwarded would be a setting the caller wrote and the connection
    /// never saw, which for `target_session_attrs` and `channel_binding` means
    /// silently connecting to a standby or without the binding that was asked
    /// for.
    // [spec:pgorm:req:conn.pool.config-forwarding]
    #[allow(unused_results)]
    pub fn get_pg_config(&self) -> Result<tokio_postgres::Config, ConfigError> {
        let mut cfg = if let Some(url) = &self.url {
            tokio_postgres::Config::from_str(url).map_err(ConfigError::InvalidUrl)?
        } else {
            tokio_postgres::Config::new()
        };
        if let Some(user) = self.user.as_ref().filter(|s| !s.is_empty()) {
            cfg.user(user.as_str());
        }
        if !cfg.get_user().is_some_and(|u| !u.is_empty())
            && let Ok(user) = env::var("USER")
        {
            cfg.user(&user);
        }
        if let Some(password) = &self.password {
            cfg.password(password);
        }
        if let Some(dbname) = self.dbname.as_ref().filter(|s| !s.is_empty()) {
            cfg.dbname(dbname);
        }
        match cfg.get_dbname() {
            None => {
                return Err(ConfigError::DbnameMissing);
            }
            Some("") => {
                return Err(ConfigError::DbnameEmpty);
            }
            _ => {}
        }
        if let Some(options) = &self.options {
            cfg.options(options.as_str());
        }
        if let Some(application_name) = &self.application_name {
            cfg.application_name(application_name.as_str());
        }
        if let Some(host) = &self.host {
            cfg.host(host.as_str());
        }
        if let Some(hosts) = &self.hosts {
            for host in hosts.iter() {
                cfg.host(host.as_str());
            }
        }
        if cfg.get_hosts().is_empty() {
            // Systems that support it default to unix domain sockets.
            #[cfg(unix)]
            {
                cfg.host_path("/run/postgresql");
                cfg.host_path("/var/run/postgresql");
                cfg.host_path("/tmp");
            }
            // Windows and other systems use 127.0.0.1 instead.
            #[cfg(not(unix))]
            cfg.host("127.0.0.1");
        }
        if let Some(hostaddr) = self.hostaddr {
            cfg.hostaddr(hostaddr);
        }
        if let Some(hostaddrs) = &self.hostaddrs {
            for hostaddr in hostaddrs {
                cfg.hostaddr(*hostaddr);
            }
        }
        if let Some(port) = self.port {
            cfg.port(port);
        }
        if let Some(ports) = &self.ports {
            for port in ports.iter() {
                cfg.port(*port);
            }
        }
        if let Some(connect_timeout) = self.connect_timeout {
            cfg.connect_timeout(connect_timeout);
        }
        if let Some(keepalives) = self.keepalives {
            cfg.keepalives(keepalives);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(keepalives_idle) = self.keepalives_idle {
            cfg.keepalives_idle(keepalives_idle);
        }
        if let Some(mode) = self.ssl_mode {
            cfg.ssl_mode(mode.into());
        }
        if let Some(target_session_attrs) = self.target_session_attrs {
            cfg.target_session_attrs(target_session_attrs.into());
        }
        if let Some(channel_binding) = self.channel_binding {
            cfg.channel_binding(channel_binding.into());
        }
        if let Some(load_balance_hosts) = self.load_balance_hosts {
            cfg.load_balance_hosts(load_balance_hosts.into());
        }
        Ok(cfg)
    }

    /// Returns [`ManagerConfig`] which can be used to construct a
    /// [`deadpool::managed::Pool`] instance.
    #[must_use]
    pub fn get_manager_config(&self) -> ManagerConfig {
        self.manager.clone().unwrap_or_default()
    }

    /// Returns [`deadpool::managed::PoolConfig`] which can be used to construct
    /// a [`deadpool::managed::Pool`] instance.
    #[must_use]
    pub fn get_pool_config(&self) -> PoolConfig {
        self.pool.unwrap_or_default()
    }
}

/// Possible methods of how a connection is recycled.
///
/// The default is [`Fast`] which does not check the connection health or
/// perform any clean-up queries.
///
/// [`Fast`]: RecyclingMethod::Fast
/// [`Verified`]: RecyclingMethod::Verified
// [spec:pgorm:sem:conn.pool.recycle]
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum RecyclingMethod {
    /// Only run [`Client::is_closed()`][1] when recycling existing connections.
    ///
    /// Unless you have special needs this is a safe choice.
    ///
    /// [1]: tokio_postgres::Client::is_closed
    #[default]
    Fast,

    /// Run [`Client::is_closed()`][1] and execute a test query.
    ///
    /// This is slower, but guarantees that the database connection is ready to
    /// be used. Normally, [`Client::is_closed()`][1] should be enough to filter
    /// out bad connections, but under some circumstances (i.e. hard-closed
    /// network connections) it's possible that [`Client::is_closed()`][1]
    /// returns `false` while the connection is dead. You will receive an error
    /// on your first query then.
    ///
    /// [1]: tokio_postgres::Client::is_closed
    Verified,

    /// Like [`Verified`] query method, but instead use the following sequence
    /// of statements which guarantees a pristine connection:
    /// ```sql
    /// CLOSE ALL;
    /// SET SESSION AUTHORIZATION DEFAULT;
    /// RESET ALL;
    /// UNLISTEN *;
    /// SELECT pg_advisory_unlock_all();
    /// DISCARD TEMP;
    /// DISCARD SEQUENCES;
    /// ```
    ///
    /// This is similar to calling `DISCARD ALL`. but doesn't call
    /// `DEALLOCATE ALL` and `DISCARD PLAN`, so that the statement cache is not
    /// rendered ineffective.
    ///
    /// [`Verified`]: RecyclingMethod::Verified
    Clean,

    /// Like [`Verified`] but allows to specify a custom SQL to be executed.
    ///
    /// [`Verified`]: RecyclingMethod::Verified
    Custom(String),
}

impl RecyclingMethod {
    const DISCARD_SQL: &'static str = "\
        CLOSE ALL; \
        SET SESSION AUTHORIZATION DEFAULT; \
        RESET ALL; \
        UNLISTEN *; \
        SELECT pg_advisory_unlock_all(); \
        DISCARD TEMP; \
        DISCARD SEQUENCES;\
    ";

    /// Returns SQL query to be executed when recycling a connection.
    pub fn query(&self) -> Option<&str> {
        match self {
            Self::Fast => None,
            Self::Verified => Some(""),
            Self::Clean => Some(Self::DISCARD_SQL),
            Self::Custom(sql) => Some(sql),
        }
    }
}

/// How many prepared statements one connection's
/// [`StatementCache`](super::StatementCache) may hold.
///
/// The key space is the SQL text, and one logical query can produce many texts:
/// an `IN` list rendered with a placeholder per element is a different text at
/// every arity. A cache that grew with it would hold server-side prepared
/// statements the connection never uses again, so the size is capped and
/// insertion into a full cache evicts to make room.
///
/// There is deliberately no unbounded variant — that is the growth this type
/// exists to stop, and a caller who wants an effectively unlimited cache can
/// say so with a large [`Bounded`](StatementCacheSize::Bounded). A zero bound
/// is unrepresentable for the same reason: it would be
/// [`Disabled`](StatementCacheSize::Disabled) spelled a second way.
// [spec:pgorm:req:conn.pool.statement-cache.bound+1]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum StatementCacheSize {
    /// Hold at most this many statements per connection.
    Bounded(NonZeroUsize),

    /// Cache nothing: every statement is prepared afresh and closed when the
    /// last handle to it drops, as if no cache existed.
    Disabled,
}

/// The default bound, an order of magnitude above the worst measured spread of
/// one logical query (25 texts, from 25 `IN`-list arities), and small enough
/// that even a large pool keeps its server-side statement count in the low
/// thousands.
const DEFAULT_STATEMENT_CACHE_SIZE: NonZeroUsize = match NonZeroUsize::new(256) {
    Some(size) => size,
    None => NonZeroUsize::MIN,
};

// [spec:pgorm:req:conn.pool.statement-cache.bound+1]    the default bound
impl Default for StatementCacheSize {
    fn default() -> Self {
        Self::Bounded(DEFAULT_STATEMENT_CACHE_SIZE)
    }
}

impl StatementCacheSize {
    /// The cap, or `None` when nothing is to be cached.
    pub(crate) fn limit(self) -> Option<NonZeroUsize> {
        match self {
            Self::Bounded(limit) => Some(limit),
            Self::Disabled => None,
        }
    }
}

/// Configuration object for a [`Manager`].
///
/// [`Manager`]: super::Manager
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ManagerConfig {
    /// Method of how a connection is recycled. See [`RecyclingMethod`].
    pub recycling_method: RecyclingMethod,

    /// Tag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,

    /// How many prepared statements each connection caches, and whether it
    /// caches at all. See [`StatementCacheSize`].
    #[serde(default)]
    pub statement_cache: StatementCacheSize,
}

static DEFAULT_TAG_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(transparent)]
pub(crate) struct Tag(pub Arc<String>);

// [spec:pgorm:sem:conn.pool.get+1]    generated default tag
impl Default for Tag {
    fn default() -> Self {
        Self(Arc::new(format!(
            "default-{}",
            DEFAULT_TAG_COUNT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Properties required of a session.
///
/// This is a 1:1 copy of the [`PgTargetSessionAttrs`] enumeration.
/// This is duplicated here in order to add support for the
/// [`serde::Deserialize`] trait which is required for the [`serde`] support.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub enum TargetSessionAttrs {
    /// No special properties are required.
    Any,

    /// The session must allow writes.
    ReadWrite,
}

// [spec:pgorm:req:conn.pool.config-forwarding]    the enum the field is forwarded as
impl From<TargetSessionAttrs> for PgTargetSessionAttrs {
    fn from(attrs: TargetSessionAttrs) -> Self {
        match attrs {
            TargetSessionAttrs::Any => Self::Any,
            TargetSessionAttrs::ReadWrite => Self::ReadWrite,
        }
    }
}

/// TLS configuration.
///
/// This is a 1:1 copy of the [`PgSslMode`] enumeration.
/// This is duplicated here in order to add support for the
/// [`serde::Deserialize`] trait which is required for the [`serde`] support.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub enum SslMode {
    /// Do not use TLS.
    Disable,

    /// Attempt to connect with TLS but allow sessions without.
    Prefer,

    /// Require the use of TLS.
    Require,
}

impl From<SslMode> for PgSslMode {
    fn from(mode: SslMode) -> Self {
        match mode {
            SslMode::Disable => Self::Disable,
            SslMode::Prefer => Self::Prefer,
            SslMode::Require => Self::Require,
        }
    }
}

/// Channel binding configuration.
///
/// This is a 1:1 copy of the [`PgChannelBinding`] enumeration.
/// This is duplicated here in order to add support for the
/// [`serde::Deserialize`] trait which is required for the [`serde`] support.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub enum ChannelBinding {
    /// Do not use channel binding.
    Disable,

    /// Attempt to use channel binding but allow sessions without.
    Prefer,

    /// Require the use of channel binding.
    Require,
}

// [spec:pgorm:req:conn.pool.config-forwarding]    the enum the field is forwarded as
impl From<ChannelBinding> for PgChannelBinding {
    fn from(cb: ChannelBinding) -> Self {
        match cb {
            ChannelBinding::Disable => Self::Disable,
            ChannelBinding::Prefer => Self::Prefer,
            ChannelBinding::Require => Self::Require,
        }
    }
}

/// Load balancing configuration.
///
/// This is a 1:1 copy of the [`PgLoadBalanceHosts`] enumeration.
/// This is duplicated here in order to add support for the
/// [`serde::Deserialize`] trait which is required for the [`serde`] support.
#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub enum LoadBalanceHosts {
    /// Make connection attempts to hosts in the order provided.
    Disable,
    /// Make connection attempts to hosts in a random order.
    Random,
}

// [spec:pgorm:req:conn.pool.config-forwarding]    the enum the field is forwarded as
impl From<LoadBalanceHosts> for PgLoadBalanceHosts {
    fn from(cb: LoadBalanceHosts) -> Self {
        match cb {
            LoadBalanceHosts::Disable => Self::Disable,
            LoadBalanceHosts::Random => Self::Random,
        }
    }
}
