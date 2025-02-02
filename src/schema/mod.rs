mod entity;

/// This is a helper struct to convert [`EntityTrait`](crate::EntityTrait)
/// into different [`pgorm_query`](crate::pgorm_query) statements.
#[derive(Debug)]
pub struct Schema {}

impl Schema {
    /// Create a helper for a specific database backend
    pub fn new() -> Self {
        Self {}
    }
}
