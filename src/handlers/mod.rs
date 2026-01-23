mod normalize;
mod registry;
mod traits;

// Site handlers
mod bluesky;
mod generic;
mod imgur;
mod instagram;
mod reddit;
mod streamable;
pub mod tiktok;
mod twitter;
pub mod youtube;

pub use normalize::normalize_url;
pub use registry::HandlerRegistry;
pub use traits::{ArchiveResult, SiteHandler};

/// Global handler registry.
pub static HANDLERS: std::sync::LazyLock<HandlerRegistry> = std::sync::LazyLock::new(|| {
    let mut registry = HandlerRegistry::new();
    registry.register(Box::new(reddit::RedditHandler::new()));
    registry.register(Box::new(youtube::YouTubeHandler::new()));
    registry.register(Box::new(tiktok::TikTokHandler::new()));
    registry.register(Box::new(twitter::TwitterHandler::new()));
    registry.register(Box::new(instagram::InstagramHandler::new()));
    registry.register(Box::new(imgur::ImgurHandler::new()));
    registry.register(Box::new(bluesky::BlueskyHandler::new()));
    registry.register(Box::new(streamable::StreamableHandler::new()));
    registry.register(Box::new(generic::GenericHandler::new()));
    registry
});
