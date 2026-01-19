use std::cmp::Reverse;

use super::traits::SiteHandler;

/// Registry of site handlers.
pub struct HandlerRegistry {
    handlers: Vec<Box<dyn SiteHandler>>,
}

impl HandlerRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register a handler.
    pub fn register(&mut self, handler: Box<dyn SiteHandler>) {
        self.handlers.push(handler);
        // Sort by priority (highest first)
        self.handlers.sort_by_key(|h| Reverse(h.priority()));
    }

    /// Find the best handler for a URL.
    #[must_use]
    pub fn find_handler(&self, url: &str) -> Option<&dyn SiteHandler> {
        self.handlers
            .iter()
            .find(|h| h.can_handle(url))
            .map(AsRef::as_ref)
    }

    /// Get all registered handlers.
    #[must_use]
    pub fn handlers(&self) -> &[Box<dyn SiteHandler>] {
        &self.handlers
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
