//! Alert and status box components for displaying messages and notifications.
//!
//! This module provides maud components for rendering alerts, messages, and
//! status boxes used throughout the archive interface.

use maud::{html, Markup, Render};

/// Alert variant types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertVariant {
    Success,
    Error,
    Warning,
    Info,
}

impl AlertVariant {
    /// Get the CSS class for the alert article element.
    #[must_use]
    pub const fn article_class(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }

    /// Get the CSS class for the message div element.
    #[must_use]
    pub const fn message_class(&self) -> &'static str {
        match self {
            Self::Success => "success-message",
            Self::Error => "error-message",
            Self::Warning => "warning-message",
            Self::Info => "info-message",
        }
    }
}

/// An alert message component.
///
/// Renders as a styled article element with success/error/warning/info styling.
///
/// # Example
///
/// ```ignore
/// use crate::components::alert::Alert;
///
/// let alert = Alert::success("Operation completed successfully!")
///     .with_title("Success");
/// ```
#[derive(Debug, Clone)]
pub struct Alert<'a> {
    pub variant: AlertVariant,
    pub title: Option<&'a str>,
    pub message: &'a str,
}

impl<'a> Alert<'a> {
    /// Create a new alert with the given variant and message.
    #[must_use]
    pub const fn new(variant: AlertVariant, message: &'a str) -> Self {
        Self {
            variant,
            title: None,
            message,
        }
    }

    /// Create a success alert.
    #[must_use]
    pub const fn success(message: &'a str) -> Self {
        Self::new(AlertVariant::Success, message)
    }

    /// Create an error alert.
    #[must_use]
    pub const fn error(message: &'a str) -> Self {
        Self::new(AlertVariant::Error, message)
    }

    /// Create a warning alert.
    #[must_use]
    pub const fn warning(message: &'a str) -> Self {
        Self::new(AlertVariant::Warning, message)
    }

    /// Create an info alert.
    #[must_use]
    pub const fn info(message: &'a str) -> Self {
        Self::new(AlertVariant::Info, message)
    }

    /// Add a title to the alert.
    #[must_use]
    pub const fn with_title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }
}

impl Render for Alert<'_> {
    fn render(&self) -> Markup {
        let class = self.variant.article_class();

        html! {
            article class=(class) {
                @if let Some(title) = self.title {
                    strong { (title) }
                    " "
                }
                (self.message)
            }
        }
    }
}

/// A status box component for admin panel style alerts.
///
/// Renders as a styled div with status-box classes.
///
/// # Example
///
/// ```ignore
/// use crate::components::alert::StatusBox;
///
/// let status = StatusBox::success("Account Status", "Your account is approved and active.");
/// ```
#[derive(Debug, Clone)]
pub struct StatusBox<'a> {
    pub variant: AlertVariant,
    pub title: &'a str,
    pub message: &'a str,
}

impl<'a> StatusBox<'a> {
    /// Create a new status box.
    #[must_use]
    pub const fn new(variant: AlertVariant, title: &'a str, message: &'a str) -> Self {
        Self {
            variant,
            title,
            message,
        }
    }

    /// Create a success status box.
    #[must_use]
    pub const fn success(title: &'a str, message: &'a str) -> Self {
        Self::new(AlertVariant::Success, title, message)
    }

    /// Create an error status box.
    #[must_use]
    pub const fn error(title: &'a str, message: &'a str) -> Self {
        Self::new(AlertVariant::Error, title, message)
    }

    /// Create a warning status box.
    #[must_use]
    pub const fn warning(title: &'a str, message: &'a str) -> Self {
        Self::new(AlertVariant::Warning, title, message)
    }

    /// Create an info status box.
    #[must_use]
    pub const fn info(title: &'a str, message: &'a str) -> Self {
        Self::new(AlertVariant::Info, title, message)
    }

    /// Get the CSS class for this status box.
    #[must_use]
    const fn css_class(&self) -> &'static str {
        match self.variant {
            AlertVariant::Success => "status-box status-box-success",
            AlertVariant::Error => "status-box status-box-error",
            AlertVariant::Warning => "status-box status-box-warning",
            AlertVariant::Info => "status-box status-box-info",
        }
    }
}

impl Render for StatusBox<'_> {
    fn render(&self) -> Markup {
        let class = self.css_class();

        html! {
            div class=(class) {
                strong { (self.title) }
                p { (self.message) }
            }
        }
    }
}

/// An NSFW warning banner component.
///
/// Renders the NSFW warning and hidden message for filtered content.
#[derive(Debug, Clone, Copy)]
pub struct NsfwWarning;

impl NsfwWarning {
    /// Create a new NSFW warning.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for NsfwWarning {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for NsfwWarning {
    fn render(&self) -> Markup {
        html! {
            div class="nsfw-warning" data-nsfw="true" {
                strong { "Warning:" }
                " This archive contains content marked as NSFW (Not Safe For Work)."
            }
            div class="nsfw-hidden-message" data-nsfw-hidden="true" {
                h2 { "NSFW Content Hidden" }
                p { "This archive contains NSFW content that is currently hidden." }
                p {
                    "Click the "
                    strong { "18+" }
                    " button in the header to view this content."
                }
            }
        }
    }
}

/// A simple inline message component.
///
/// For lighter-weight messages that don't need the full article styling.
#[derive(Debug, Clone)]
pub struct Message<'a> {
    pub variant: AlertVariant,
    pub text: &'a str,
}

impl<'a> Message<'a> {
    /// Create a new message.
    #[must_use]
    pub const fn new(variant: AlertVariant, text: &'a str) -> Self {
        Self { variant, text }
    }

    /// Create a success message.
    #[must_use]
    pub const fn success(text: &'a str) -> Self {
        Self::new(AlertVariant::Success, text)
    }

    /// Create an error message.
    #[must_use]
    pub const fn error(text: &'a str) -> Self {
        Self::new(AlertVariant::Error, text)
    }

    /// Create a warning message.
    #[must_use]
    pub const fn warning(text: &'a str) -> Self {
        Self::new(AlertVariant::Warning, text)
    }

    /// Create an info message.
    #[must_use]
    pub const fn info(text: &'a str) -> Self {
        Self::new(AlertVariant::Info, text)
    }
}

impl Render for Message<'_> {
    fn render(&self) -> Markup {
        let class = self.variant.message_class();

        html! {
            div class=(class) {
                (self.text)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_success() {
        let alert = Alert::success("Operation completed!");
        let html = alert.render().into_string();
        assert!(html.contains("class=\"success\""));
        assert!(html.contains("Operation completed!"));
    }

    #[test]
    fn test_alert_error_with_title() {
        let alert = Alert::error("Something went wrong").with_title("Error");
        let html = alert.render().into_string();
        assert!(html.contains("class=\"error\""));
        assert!(html.contains("<strong>Error</strong>"));
        assert!(html.contains("Something went wrong"));
    }

    #[test]
    fn test_alert_warning() {
        let alert = Alert::warning("Be careful!");
        let html = alert.render().into_string();
        assert!(html.contains("class=\"warning\""));
    }

    #[test]
    fn test_alert_info() {
        let alert = Alert::info("Just so you know...");
        let html = alert.render().into_string();
        assert!(html.contains("class=\"info\""));
    }

    #[test]
    fn test_status_box_success() {
        let status = StatusBox::success("Account Active", "Your account is approved.");
        let html = status.render().into_string();
        assert!(html.contains("status-box-success"));
        assert!(html.contains("<strong>Account Active</strong>"));
        assert!(html.contains("Your account is approved."));
    }

    #[test]
    fn test_status_box_error() {
        let status = StatusBox::error("Account Locked", "Too many failed attempts.");
        let html = status.render().into_string();
        assert!(html.contains("status-box-error"));
    }

    #[test]
    fn test_status_box_warning() {
        let status = StatusBox::warning("Pending Approval", "Awaiting admin review.");
        let html = status.render().into_string();
        assert!(html.contains("status-box-warning"));
    }

    #[test]
    fn test_nsfw_warning() {
        let warning = NsfwWarning::new();
        let html = warning.render().into_string();
        assert!(html.contains("nsfw-warning"));
        assert!(html.contains("data-nsfw=\"true\""));
        assert!(html.contains("nsfw-hidden-message"));
        assert!(html.contains("NSFW Content Hidden"));
    }

    #[test]
    fn test_message_success() {
        let msg = Message::success("Done!");
        let html = msg.render().into_string();
        assert!(html.contains("success-message"));
        assert!(html.contains("Done!"));
    }

    #[test]
    fn test_message_error() {
        let msg = Message::error("Failed!");
        let html = msg.render().into_string();
        assert!(html.contains("error-message"));
    }
}
