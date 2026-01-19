mod normalize;
mod registry;
mod traits;

// Site handlers
mod generic;
mod reddit;
mod tiktok;
mod twitter;
mod youtube;

pub use normalize::normalize_url;
pub use registry::HandlerRegistry;
pub use traits::{ArchiveResult, SiteHandler};

use once_cell::sync::Lazy;

/// Global handler registry.
pub static HANDLERS: Lazy<HandlerRegistry> = Lazy::new(|| {
    let mut registry = HandlerRegistry::new();
    registry.register(Box::new(reddit::RedditHandler::new()));
    registry.register(Box::new(youtube::YouTubeHandler::new()));
    registry.register(Box::new(tiktok::TikTokHandler::new()));
    registry.register(Box::new(twitter::TwitterHandler::new()));
    registry.register(Box::new(generic::GenericHandler::new()));
    registry
});
