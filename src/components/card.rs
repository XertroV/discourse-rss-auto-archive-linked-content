//! Card components for displaying archives and content.
//!
//! This module provides maud components for rendering archive cards and grids.

use maud::{html, Markup, Render};

use crate::components::badge::{DomainBadge, MediaTypeBadge, NsfwBadge, SizeBadge, StatusBadge};
use crate::db::ArchiveDisplay;

/// An archive card component for displaying archive summaries.
///
/// This is the primary component for showing archives in lists and grids.
///
/// # Example
///
/// ```ignore
/// use crate::components::card::ArchiveCard;
/// use crate::db::ArchiveDisplay;
///
/// let card = ArchiveCard::new(&archive)
///     .with_post_link();
/// ```
#[derive(Debug, Clone)]
pub struct ArchiveCard<'a> {
    pub archive: &'a ArchiveDisplay,
    pub show_post_link: bool,
}

impl<'a> ArchiveCard<'a> {
    /// Create a new archive card.
    #[must_use]
    pub const fn new(archive: &'a ArchiveDisplay) -> Self {
        Self {
            archive,
            show_post_link: false,
        }
    }

    /// Enable showing a link to the source post.
    #[must_use]
    pub const fn with_post_link(mut self) -> Self {
        self.show_post_link = true;
        self
    }
}

impl Render for ArchiveCard<'_> {
    fn render(&self) -> Markup {
        let archive = self.archive;
        let title = archive.content_title.as_deref().unwrap_or("Untitled");
        let content_type = archive.content_type.as_deref().unwrap_or("unknown");
        let archived_time = archive.archived_at.as_deref().unwrap_or("pending");

        // Build the status badge
        let status_badge =
            StatusBadge::from_status(&archive.status).with_error(archive.error_message.as_deref());

        // Build the media type badge
        let type_badge = MediaTypeBadge::from_content_type(content_type);

        // Build the domain badge
        let domain_badge = DomainBadge::new(&archive.domain);

        // Build optional size badge
        let size_bytes = archive.total_size_bytes.unwrap_or(0);

        html! {
            article class="archive-card" data-nsfw=[archive.is_nsfw.then_some("true")] {
                // Hidden NSFW placeholder - shown when NSFW filter is active
                @if archive.is_nsfw {
                    div class="archive-card-nsfw-placeholder" {
                        div class="archive-card-nsfw-placeholder-icon" {
                            "🔞"
                        }
                        h3 { "NSFW Content Hidden" }
                        p { "This archive contains NSFW content." }
                        p class="archive-card-nsfw-placeholder-hint" {
                            "Click the "
                            strong { "18+" }
                            " button in the header to view."
                        }
                    }
                }

                // Main card content - hidden when NSFW filter is active
                div class="archive-card-content" {
                    h3 {
                        @if archive.is_nsfw {
                            (NsfwBadge::new())
                        }
                        a href=(format!("/archive/{}", archive.id)) { (title) }
                    }
                    p class="archive-url" {
                        code class="url-display" title="Click to copy" data-copy-url=(archive.original_url) {
                            (archive.original_url)
                        }
                    }
                    p class="meta" {
                        (status_badge)
                        (type_badge)
                        (domain_badge)
                        @if let Some(author) = &archive.content_author {
                            span class="author" {
                                "by " (author)
                            }
                        }
                        @if size_bytes > 0 {
                            (SizeBadge::new(size_bytes))
                        }
                    }
                    p class="archive-time" { (archived_time) }
                }
            }
        }
    }
}

/// A grid container for displaying multiple archive cards.
///
/// # Example
///
/// ```ignore
/// use crate::components::card::ArchiveGrid;
/// use crate::db::ArchiveDisplay;
///
/// let grid = ArchiveGrid::new(&archives)
///     .with_post_links();
/// ```
#[derive(Debug, Clone)]
pub struct ArchiveGrid<'a> {
    pub archives: &'a [ArchiveDisplay],
    pub show_post_links: bool,
}

impl<'a> ArchiveGrid<'a> {
    /// Create a new archive grid.
    #[must_use]
    pub const fn new(archives: &'a [ArchiveDisplay]) -> Self {
        Self {
            archives,
            show_post_links: false,
        }
    }

    /// Enable showing post links on all cards.
    #[must_use]
    pub const fn with_post_links(mut self) -> Self {
        self.show_post_links = true;
        self
    }
}

impl Render for ArchiveGrid<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="archive-grid" {
                @for archive in self.archives {
                    @if self.show_post_links {
                        (ArchiveCard::new(archive).with_post_link())
                    } @else {
                        (ArchiveCard::new(archive))
                    }
                }
            }
        }
    }
}

/// An empty state component for when there are no archives.
#[derive(Debug, Clone)]
pub struct EmptyState<'a> {
    pub message: &'a str,
}

impl<'a> EmptyState<'a> {
    /// Create a new empty state.
    #[must_use]
    pub const fn new(message: &'a str) -> Self {
        Self { message }
    }

    /// Create a default "no archives" empty state.
    #[must_use]
    pub const fn no_archives() -> Self {
        Self {
            message: "No archives yet.",
        }
    }

    /// Create a "no results" empty state.
    #[must_use]
    pub const fn no_results() -> Self {
        Self {
            message: "No results found.",
        }
    }
}

impl Render for EmptyState<'_> {
    fn render(&self) -> Markup {
        html! {
            p { (self.message) }
        }
    }
}

/// A card with thumbnail support for archives with images.
#[derive(Debug, Clone)]
pub struct ArchiveCardWithThumb<'a> {
    pub archive: &'a ArchiveDisplay,
    pub thumb_url: Option<&'a str>,
}

impl<'a> ArchiveCardWithThumb<'a> {
    /// Create a new archive card with optional thumbnail.
    #[must_use]
    pub const fn new(archive: &'a ArchiveDisplay) -> Self {
        Self {
            archive,
            thumb_url: None,
        }
    }

    /// Set the thumbnail URL.
    #[must_use]
    pub const fn with_thumb(mut self, url: &'a str) -> Self {
        self.thumb_url = Some(url);
        self
    }
}

impl Render for ArchiveCardWithThumb<'_> {
    fn render(&self) -> Markup {
        let archive = self.archive;
        let title = archive.content_title.as_deref().unwrap_or("Untitled");
        let content_type = archive.content_type.as_deref().unwrap_or("unknown");
        let archived_time = archive.archived_at.as_deref().unwrap_or("pending");

        let status_badge =
            StatusBadge::from_status(&archive.status).with_error(archive.error_message.as_deref());
        let type_badge = MediaTypeBadge::from_content_type(content_type);
        let domain_badge = DomainBadge::new(&archive.domain);
        let size_bytes = archive.total_size_bytes.unwrap_or(0);

        html! {
            article class="archive-card" data-nsfw=[archive.is_nsfw.then_some("true")] {
                // Hidden NSFW placeholder - shown when NSFW filter is active
                @if archive.is_nsfw {
                    div class="archive-card-nsfw-placeholder" {
                        div class="archive-card-nsfw-placeholder-icon" {
                            "🔞"
                        }
                        h3 { "NSFW Content Hidden" }
                        p { "This archive contains NSFW content." }
                        p class="archive-card-nsfw-placeholder-hint" {
                            "Click the "
                            strong { "18+" }
                            " button in the header to view."
                        }
                    }
                }

                // Main card content - hidden when NSFW filter is active
                div class="archive-card-content" {
                    @if let Some(thumb) = self.thumb_url {
                        img class="archive-thumb" src=(thumb) alt=(title) loading="lazy";
                    }
                    h3 {
                        @if archive.is_nsfw {
                            (NsfwBadge::new())
                        }
                        a href=(format!("/archive/{}", archive.id)) { (title) }
                    }
                    p class="archive-url" {
                        code class="url-display" title="Click to copy" data-copy-url=(archive.original_url) {
                            (archive.original_url)
                        }
                    }
                    p class="meta" {
                        (status_badge)
                        (type_badge)
                        (domain_badge)
                        @if let Some(author) = &archive.content_author {
                            span class="author" {
                                "by " (author)
                            }
                        }
                        @if size_bytes > 0 {
                            (SizeBadge::new(size_bytes))
                        }
                    }
                    p class="archive-time" { (archived_time) }
                }
            }
        }
    }
}

/// A stats card component for displaying statistics sections.
///
/// This is used to organize stats sections into card containers
/// that can be laid out in a grid.
///
/// # Example
///
/// ```ignore
/// use crate::components::card::StatsCard;
///
/// let card = StatsCard::new("Overview")
///     .with_items(vec![
///         ("Total Posts", "100"),
///         ("Total Links", "500"),
///     ]);
/// ```
#[derive(Debug, Clone)]
pub struct StatsCard<'a> {
    pub title: &'a str,
    pub items: Vec<(&'a str, String)>,
}

impl<'a> StatsCard<'a> {
    /// Create a new stats card with a title.
    #[must_use]
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            items: Vec::new(),
        }
    }

    /// Add a stat item to the card.
    #[must_use]
    pub fn item(mut self, label: &'a str, value: impl Into<String>) -> Self {
        self.items.push((label, value.into()));
        self
    }
}

impl Render for StatsCard<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="stats-card" {
                h3 class="stats-card-title" { (self.title) }
                div class="stats-card-content" {
                    @for (label, value) in &self.items {
                        div class="stats-card-item" {
                            span class="stats-card-label" { (label) ":" }
                            span class="stats-card-value" { (value) }
                        }
                    }
                }
            }
        }
    }
}

/// A grid container for displaying multiple stats cards.
///
/// # Example
///
/// ```ignore
/// use crate::components::card::StatsCardGrid;
///
/// let grid = StatsCardGrid::new()
///     .card(overview_card)
///     .card(activity_card);
/// ```
#[derive(Debug, Clone, Default)]
pub struct StatsCardGrid {
    pub cards: Vec<Markup>,
}

impl StatsCardGrid {
    /// Create a new empty stats card grid.
    #[must_use]
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    /// Add a card to the grid.
    #[must_use]
    pub fn card(mut self, card: impl Render) -> Self {
        self.cards.push(card.render());
        self
    }
}

impl Render for StatsCardGrid {
    fn render(&self) -> Markup {
        html! {
            div class="stats-card-grid" {
                @for card in &self.cards {
                    (card)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_archive() -> ArchiveDisplay {
        ArchiveDisplay {
            id: 1,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15 12:00:00".to_string()),
            content_title: Some("Test Video".to_string()),
            content_author: Some("testuser".to_string()),
            content_type: Some("video".to_string()),
            is_nsfw: false,
            error_message: None,
            retry_count: 0,
            original_url: "https://example.com/video/123".to_string(),
            domain: "example.com".to_string(),
            total_size_bytes: Some(1048576),
        }
    }

    #[test]
    fn test_archive_card_basic() {
        let archive = sample_archive();
        let card = ArchiveCard::new(&archive);
        let html = card.render().into_string();

        assert!(html.contains("archive-card"));
        assert!(html.contains("Test Video"));
        assert!(html.contains("/archive/1"));
        assert!(html.contains("example.com"));
        assert!(html.contains("status-complete"));
        assert!(html.contains("media-type-video"));
    }

    #[test]
    fn test_archive_card_nsfw() {
        let mut archive = sample_archive();
        archive.is_nsfw = true;
        let card = ArchiveCard::new(&archive);
        let html = card.render().into_string();

        assert!(html.contains("data-nsfw=\"true\""));
        assert!(html.contains("nsfw-badge"));
    }

    #[test]
    fn test_archive_card_with_author() {
        let archive = sample_archive();
        let card = ArchiveCard::new(&archive);
        let html = card.render().into_string();

        assert!(html.contains("by testuser"));
    }

    #[test]
    fn test_archive_card_with_size() {
        let archive = sample_archive();
        let card = ArchiveCard::new(&archive);
        let html = card.render().into_string();

        assert!(html.contains("1.0 MB"));
    }

    #[test]
    fn test_archive_card_failed_with_error() {
        let mut archive = sample_archive();
        archive.status = "failed".to_string();
        archive.error_message = Some("Connection timeout".to_string());
        let card = ArchiveCard::new(&archive);
        let html = card.render().into_string();

        assert!(html.contains("status-failed"));
        assert!(html.contains("Connection timeout"));
    }

    #[test]
    fn test_archive_grid() {
        let archives = vec![sample_archive()];
        let grid = ArchiveGrid::new(&archives);
        let html = grid.render().into_string();

        assert!(html.contains("archive-grid"));
        assert!(html.contains("archive-card"));
    }

    #[test]
    fn test_archive_grid_empty() {
        let archives: Vec<ArchiveDisplay> = vec![];
        let grid = ArchiveGrid::new(&archives);
        let html = grid.render().into_string();

        assert!(html.contains("archive-grid"));
        assert!(!html.contains("archive-card"));
    }

    #[test]
    fn test_empty_state() {
        let empty = EmptyState::no_archives();
        let html = empty.render().into_string();

        assert!(html.contains("No archives yet."));
    }

    #[test]
    fn test_archive_card_with_thumb() {
        let archive = sample_archive();
        let card = ArchiveCardWithThumb::new(&archive).with_thumb("/thumbs/1.jpg");
        let html = card.render().into_string();

        assert!(html.contains("archive-thumb"));
        assert!(html.contains("/thumbs/1.jpg"));
    }

    #[test]
    fn test_archive_card_copy_url_attribute() {
        let archive = sample_archive();
        let card = ArchiveCard::new(&archive);
        let html = card.render().into_string();

        assert!(html.contains("data-copy-url"));
        assert!(html.contains("Click to copy"));
    }

    #[test]
    fn test_stats_card_basic() {
        let card = StatsCard::new("Overview")
            .item("Total Posts", "100")
            .item("Total Links", "500");
        let html = card.render().into_string();

        assert!(html.contains("stats-card"));
        assert!(html.contains("stats-card-title"));
        assert!(html.contains("Overview"));
        assert!(html.contains("Total Posts"));
        assert!(html.contains("100"));
        assert!(html.contains("Total Links"));
        assert!(html.contains("500"));
    }

    #[test]
    fn test_stats_card_empty() {
        let card = StatsCard::new("Empty Section");
        let html = card.render().into_string();

        assert!(html.contains("stats-card"));
        assert!(html.contains("Empty Section"));
        assert!(html.contains("stats-card-content"));
    }

    #[test]
    fn test_stats_card_grid() {
        let card1 = StatsCard::new("Card 1").item("Stat", "10");
        let card2 = StatsCard::new("Card 2").item("Other", "20");
        let grid = StatsCardGrid::new().card(card1).card(card2);
        let html = grid.render().into_string();

        assert!(html.contains("stats-card-grid"));
        assert!(html.contains("Card 1"));
        assert!(html.contains("Card 2"));
        assert!(html.contains("Stat"));
        assert!(html.contains("Other"));
    }

    #[test]
    fn test_stats_card_grid_empty() {
        let grid = StatsCardGrid::new();
        let html = grid.render().into_string();

        assert!(html.contains("stats-card-grid"));
        assert!(!html.contains("stats-card-title"));
    }
}
