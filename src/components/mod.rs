//! Maud HTML template components for the web UI.
//!
//! This module provides reusable maud components for generating HTML.
//! Components are organized into submodules by functionality:
//!
//! - `layout`: Base page layout and navigation
//! - `badge`: Status, domain, media type, artifact kind, and NSFW badges
//! - `button`: Configurable button and link-button components
//! - `alert`: Alert messages and status boxes
//! - `card`: Archive cards and grids
//! - `form`: Form elements and input components
//! - `table`: Table components with various styles (admin, artifacts, stats, etc.)
//! - `media`: Video, audio, and image display components
//! - `pagination`: Page navigation controls
//! - `tabs`: Tab navigation components
//!
//! # Example
//!
//! ```ignore
//! use maud::{html, Markup};
//! use crate::components::{BaseLayout, Button, StatusBadge, Alert, Input, Table};
//!
//! fn my_page() -> Markup {
//!     let content = html! {
//!         h1 { "Hello World" }
//!         (Alert::success("Page loaded!"))
//!         (Button::primary("Click me"))
//!         (StatusBadge::complete())
//!         (Input::text("username").placeholder("Enter username"))
//!     };
//!     BaseLayout::new("My Page").render(content)
//! }
//! ```

pub mod alert;
pub mod badge;
pub mod button;
pub mod card;
pub mod form;
pub mod layout;
pub mod media;
pub mod metadata;
pub mod pagination;
pub mod table;
pub mod tabs;

// Re-export layout components
pub use layout::BaseLayout;

// Re-export badge components
pub use badge::{
    ArtifactKindBadge, ArtifactKindVariant, Badge, DomainBadge, MediaTypeBadge, MediaTypeVariant,
    NsfwBadge, SizeBadge, StatusBadge, StatusVariant,
};

// Re-export button components
pub use button::{Button, ButtonVariant};

// Re-export alert components
pub use alert::{Alert, AlertVariant, Message, NsfwWarning, StatusBox};

// Re-export card components
pub use card::{
    ArchiveCard, ArchiveCardWithThumb, ArchiveGrid, EmptyState, StatsCard, StatsCardGrid,
};

// Re-export form components
pub use form::{
    Checkbox, Form, FormGroup, FormHelp, HiddenInput, Input, Label, Select, SelectOption, TextArea,
};

// Re-export table components
pub use table::{
    markup_row, simple_row, KeyValueTable, ResponsiveTable, Table, TableCell, TableRow,
    TableVariant,
};

// Re-export media components
pub use media::{
    render_media_player, render_media_player_with_options, AudioPlayer, ImageViewer,
    MediaContainer, MediaGallery, VideoPlayer,
};

// Re-export pagination components
pub use pagination::Pagination;

// Re-export tab components
pub use tabs::{archive_list_tabs, ArchiveTab, ContentTab, ContentTabs, Tab, TabGroup};

// Re-export metadata components
pub use metadata::{truncate_text, OpenGraphMetadata};

/// Re-export maud for convenience
pub use maud::{html, Markup, PreEscaped, DOCTYPE};
