//! Button component for the web UI.
//!
//! Provides a configurable button component that renders as either
//! a `<button>` or `<a>` element based on whether an href is provided.

use maud::{html, Markup, Render};

/// Button style variants matching CSS classes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Primary button (default) - `.btn-primary` or `.btn`
    #[default]
    Primary,
    /// Outline button - `.btn.outline`
    Outline,
    /// Danger button - `.btn-danger`
    Danger,
    /// Secondary button - `.btn-secondary`
    Secondary,
    /// Success button - `.btn-success`
    Success,
    /// Warning button - `.btn-warning`
    Warning,
    /// Ghost button (no background, minimal styling)
    Ghost,
    /// Small button - `.btn-sm`
    Small,
}

impl ButtonVariant {
    /// Returns the CSS class(es) for this variant.
    #[must_use]
    pub fn class(&self) -> &'static str {
        match self {
            Self::Primary => "btn btn-primary",
            Self::Outline => "btn outline",
            Self::Danger => "btn btn-danger",
            Self::Secondary => "btn btn-secondary",
            Self::Success => "btn btn-success",
            Self::Warning => "btn btn-warning",
            Self::Ghost => "btn outline",
            Self::Small => "btn btn-sm",
        }
    }
}

/// A configurable button component.
///
/// # Example
///
/// ```ignore
/// use crate::components::button::Button;
///
/// // Simple primary button
/// let btn = Button::primary("Click me");
///
/// // Link-style button
/// let link_btn = Button::outline("View Archive")
///     .href("/archive/123")
///     .target_blank();
///
/// // Danger button with click handler
/// let delete_btn = Button::danger("Delete")
///     .onclick("confirm('Are you sure?')");
/// ```
#[derive(Debug, Clone)]
pub struct Button<'a> {
    /// Button label text
    pub label: &'a str,
    /// Button style variant
    pub variant: ButtonVariant,
    /// Optional href (renders as `<a>` if present)
    pub href: Option<&'a str>,
    /// Open link in new tab
    pub target_blank: bool,
    /// Disabled state
    pub disabled: bool,
    /// Button type attribute (for `<button>` elements)
    pub r#type: Option<&'a str>,
    /// Additional CSS classes
    pub class: Option<&'a str>,
    /// Element ID
    pub id: Option<&'a str>,
    /// JavaScript onclick handler
    pub onclick: Option<&'a str>,
    /// Download attribute (for download links)
    pub download: Option<&'a str>,
}

impl<'a> Button<'a> {
    /// Creates a new button with the given label and variant.
    #[must_use]
    pub fn new(label: &'a str, variant: ButtonVariant) -> Self {
        Self {
            label,
            variant,
            href: None,
            target_blank: false,
            disabled: false,
            r#type: None,
            class: None,
            id: None,
            onclick: None,
            download: None,
        }
    }

    /// Creates a primary button.
    #[must_use]
    pub fn primary(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Primary)
    }

    /// Creates an outline button.
    #[must_use]
    pub fn outline(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Outline)
    }

    /// Creates a danger button.
    #[must_use]
    pub fn danger(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Danger)
    }

    /// Creates a secondary button.
    #[must_use]
    pub fn secondary(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Secondary)
    }

    /// Creates a success button.
    #[must_use]
    pub fn success(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Success)
    }

    /// Creates a warning button.
    #[must_use]
    pub fn warning(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Warning)
    }

    /// Creates a ghost button.
    #[must_use]
    pub fn ghost(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Ghost)
    }

    /// Creates a small button.
    #[must_use]
    pub fn small(label: &'a str) -> Self {
        Self::new(label, ButtonVariant::Small)
    }

    /// Sets the href, rendering the button as an `<a>` element.
    #[must_use]
    pub fn href(mut self, href: &'a str) -> Self {
        self.href = Some(href);
        self
    }

    /// Sets whether to open the link in a new tab.
    #[must_use]
    pub fn target_blank(mut self) -> Self {
        self.target_blank = true;
        self
    }

    /// Sets the disabled state.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Sets the button type attribute.
    #[must_use]
    pub fn r#type(mut self, r#type: &'a str) -> Self {
        self.r#type = Some(r#type);
        self
    }

    /// Adds additional CSS classes.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Sets the element ID.
    #[must_use]
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the onclick handler.
    #[must_use]
    pub fn onclick(mut self, onclick: &'a str) -> Self {
        self.onclick = Some(onclick);
        self
    }

    /// Sets the download attribute (suggested filename for download).
    #[must_use]
    pub fn download(mut self, filename: &'a str) -> Self {
        self.download = Some(filename);
        self
    }

    /// Builds the full CSS class string.
    fn build_class(&self) -> String {
        let mut classes = self.variant.class().to_string();
        if let Some(extra) = self.class {
            classes.push(' ');
            classes.push_str(extra);
        }
        classes
    }
}

impl Render for Button<'_> {
    fn render(&self) -> Markup {
        let classes = self.build_class();

        if let Some(href) = self.href {
            // Render as anchor element
            html! {
                a
                    class=(classes)
                    href=(href)
                    id=[self.id]
                    target=[self.target_blank.then_some("_blank")]
                    rel=[self.target_blank.then_some("noopener noreferrer")]
                    onclick=[self.onclick]
                    download=[self.download]
                    aria-disabled=[self.disabled.then_some("true")]
                {
                    (self.label)
                }
            }
        } else {
            // Render as button element
            html! {
                button
                    class=(classes)
                    type=(self.r#type.unwrap_or("button"))
                    id=[self.id]
                    disabled[self.disabled]
                    onclick=[self.onclick]
                {
                    (self.label)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primary_button() {
        let btn = Button::primary("Click me");
        let html = btn.render().into_string();
        assert!(html.contains("btn btn-primary"));
        assert!(html.contains("Click me"));
        assert!(html.contains("<button"));
    }

    #[test]
    fn test_outline_button() {
        let btn = Button::outline("Outline");
        let html = btn.render().into_string();
        assert!(html.contains("btn outline"));
    }

    #[test]
    fn test_danger_button() {
        let btn = Button::danger("Delete");
        let html = btn.render().into_string();
        assert!(html.contains("btn-danger"));
    }

    #[test]
    fn test_secondary_button() {
        let btn = Button::secondary("Cancel");
        let html = btn.render().into_string();
        assert!(html.contains("btn-secondary"));
    }

    #[test]
    fn test_success_button() {
        let btn = Button::success("Save");
        let html = btn.render().into_string();
        assert!(html.contains("btn-success"));
    }

    #[test]
    fn test_warning_button() {
        let btn = Button::warning("Caution");
        let html = btn.render().into_string();
        assert!(html.contains("btn-warning"));
    }

    #[test]
    fn test_small_button() {
        let btn = Button::small("Small");
        let html = btn.render().into_string();
        assert!(html.contains("btn-sm"));
    }

    #[test]
    fn test_button_with_href() {
        let btn = Button::primary("Link").href("/test");
        let html = btn.render().into_string();
        assert!(html.contains("<a"));
        assert!(html.contains("href=\"/test\""));
        assert!(!html.contains("<button"));
    }

    #[test]
    fn test_button_target_blank() {
        let btn = Button::primary("External").href("/ext").target_blank();
        let html = btn.render().into_string();
        assert!(html.contains("target=\"_blank\""));
        assert!(html.contains("rel=\"noopener noreferrer\""));
    }

    #[test]
    fn test_button_disabled() {
        let btn = Button::primary("Disabled").disabled();
        let html = btn.render().into_string();
        assert!(html.contains("disabled"));
    }

    #[test]
    fn test_button_with_type() {
        let btn = Button::primary("Submit").r#type("submit");
        let html = btn.render().into_string();
        assert!(html.contains("type=\"submit\""));
    }

    #[test]
    fn test_button_with_extra_class() {
        let btn = Button::primary("Styled").class("extra-class");
        let html = btn.render().into_string();
        assert!(html.contains("btn btn-primary extra-class"));
    }

    #[test]
    fn test_button_with_id() {
        let btn = Button::primary("Identified").id("my-button");
        let html = btn.render().into_string();
        assert!(html.contains("id=\"my-button\""));
    }

    #[test]
    fn test_button_with_onclick() {
        let btn = Button::primary("Action").onclick("alert('hi')");
        let html = btn.render().into_string();
        assert!(html.contains("onclick=\"alert('hi')\""));
    }

    #[test]
    fn test_button_builder_chain() {
        let btn = Button::danger("Delete All")
            .id("delete-btn")
            .class("margin-left")
            .onclick("return confirm('Sure?')")
            .r#type("submit");

        let html = btn.render().into_string();
        assert!(html.contains("btn-danger"));
        assert!(html.contains("id=\"delete-btn\""));
        assert!(html.contains("margin-left"));
        assert!(html.contains("onclick"));
        assert!(html.contains("type=\"submit\""));
    }

    #[test]
    fn test_link_button_disabled() {
        let btn = Button::primary("Disabled Link").href("/test").disabled();
        let html = btn.render().into_string();
        assert!(html.contains("aria-disabled=\"true\""));
    }

    #[test]
    fn test_button_variant_classes() {
        assert_eq!(ButtonVariant::Primary.class(), "btn btn-primary");
        assert_eq!(ButtonVariant::Outline.class(), "btn outline");
        assert_eq!(ButtonVariant::Danger.class(), "btn btn-danger");
        assert_eq!(ButtonVariant::Secondary.class(), "btn btn-secondary");
        assert_eq!(ButtonVariant::Success.class(), "btn btn-success");
        assert_eq!(ButtonVariant::Warning.class(), "btn btn-warning");
        assert_eq!(ButtonVariant::Ghost.class(), "btn outline");
        assert_eq!(ButtonVariant::Small.class(), "btn btn-sm");
    }

    #[test]
    fn test_default_button_type() {
        let btn = Button::primary("Default Type");
        let html = btn.render().into_string();
        assert!(html.contains("type=\"button\""));
    }

    #[test]
    fn test_download_button() {
        let btn = Button::secondary("Download")
            .href("/files/document.pdf")
            .download("my-document.pdf");
        let html = btn.render().into_string();
        assert!(html.contains("download=\"my-document.pdf\""));
        assert!(html.contains("href=\"/files/document.pdf\""));
        assert!(html.contains("<a"));
    }
}
