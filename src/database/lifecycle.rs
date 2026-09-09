use pgorm_pool::Object;

use super::{DatabaseConnection, DatabasePool};

impl DatabasePool {
    /// Close the pool, rejecting waiters and discarding idle connections.
    /// Checked-out connections remain usable until their owners release them.
    // [spec:pgorm:req:python.connections]
    pub fn close(&self) {
        self.0.close();
    }

    /// Whether this pool has been explicitly closed.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

impl DatabaseConnection {
    /// Discard this connection instead of returning it to the pool.
    ///
    /// Use this when cancellation leaves the outcome or session state unknown.
    /// Dropping the detached client aborts its connection task.
    // [spec:pgorm:req:python.connections]
    pub fn discard(self) {
        drop(Object::take(self.0));
    }
}
