//! The clears a pool's statement-cache listener makes, counted from the
//! debug event it logs for each one (`conn.pool.statement-cache.listen`).
//!
//! A test cannot wait for a clear through the server. The listener's
//! `LISTEN` shows in `pg_stat_activity` before its reply has reached the
//! client, and the clear is made only once the reply has; a statement
//! prepared in between is thrown away by a clear the test thought was behind
//! it.

use std::{fmt::Debug, future::Future, sync::Arc, time::Duration};

use tokio::sync::watch;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt},
    util::SubscriberInitExt,
};

const TARGET: &str = "pgorm_pool::cache_listener";

/// How each of the listener's clear events ends: `listening; statement caches
/// cleared` once its `LISTEN` takes effect, and `notified; statement caches
/// cleared` once per notification.
const CLEARED: &str = "statement caches cleared";

/// How long [`ListenerClears::reach`] waits.
const ARRIVAL: Duration = Duration::from_secs(10);

/// A count of the listener's clears, for a test run through
/// [`ListenerClears::run`].
#[derive(Clone, Debug)]
pub struct ListenerClears(Arc<watch::Sender<usize>>);

impl Default for ListenerClears {
    fn default() -> Self {
        Self(Arc::new(watch::Sender::new(0)))
    }
}

impl ListenerClears {
    /// Run `test` on a runtime of one thread, so that the listener's task,
    /// which the pool spawns there, logs every event to this count as well as
    /// to the test's output.
    pub fn run<F: Future>(&self, test: F) -> F::Output {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a test runtime");
        let _scoped = tracing_subscriber::registry()
            .with(self.clone())
            .with(tracing_subscriber::fmt::layer().with_test_writer())
            .set_default();
        runtime.block_on(test)
    }

    pub fn count(&self) -> usize {
        *self.0.borrow()
    }

    /// Wait until the listener has made `clears` clears in all, panicking
    /// past [`ARRIVAL`].
    pub async fn reach(&self, clears: usize) {
        let mut made = self.0.subscribe();
        let reached = tokio::time::timeout(ARRIVAL, made.wait_for(|made| *made >= clears)).await;
        assert!(
            matches!(reached, Ok(Ok(_))),
            "the listener did not make {clears} clears within {ARRIVAL:?}, only {}",
            self.count()
        );
    }
}

impl<S: Subscriber> Layer<S> for ListenerClears {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != TARGET {
            return;
        }
        let mut message = Message::default();
        event.record(&mut message);
        if message.0.ends_with(CLEARED) {
            self.0.send_modify(|made| *made += 1);
        }
    }
}

#[derive(Default)]
struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}
