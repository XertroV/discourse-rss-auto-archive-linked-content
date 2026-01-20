//! Tab components for navigation between related views.
//!
//! This module provides reusable tab components for creating
//! tabbed navigation interfaces. Includes both link-based tabs for
//! navigation between pages and content tabs for showing/hiding
//! panels on the same page.

use maud::{html, Markup, PreEscaped, Render};

/// A single tab in a tab group.
#[derive(Debug, Clone)]
pub struct Tab<'a> {
    /// Display label for the tab
    pub label: &'a str,
    /// URL/href for the tab link
    pub href: &'a str,
    /// Optional count badge to display
    pub count: Option<i64>,
    /// Whether this tab is currently active
    pub active: bool,
}

impl<'a> Tab<'a> {
    /// Create a new tab.
    #[must_use]
    pub fn new(label: &'a str, href: &'a str) -> Self {
        Self {
            label,
            href,
            count: None,
            active: false,
        }
    }

    /// Set the tab as active.
    #[must_use]
    pub fn active(mut self) -> Self {
        self.active = true;
        self
    }

    /// Add a count badge to the tab.
    #[must_use]
    pub fn with_count(mut self, count: i64) -> Self {
        self.count = Some(count);
        self
    }

    /// Optionally add a count badge to the tab.
    #[must_use]
    pub fn with_optional_count(mut self, count: Option<i64>) -> Self {
        self.count = count;
        self
    }
}

impl Render for Tab<'_> {
    fn render(&self) -> Markup {
        let class = if self.active {
            "archive-tab active"
        } else {
            "archive-tab"
        };

        html! {
            a class=(class) href=(self.href) {
                (self.label)
                @if let Some(count) = self.count {
                    @if count > 0 {
                        " "
                        span class="archive-tab-count" aria-label=(format!("{} items", count)) {
                            (count)
                        }
                    }
                }
            }
        }
    }
}

/// A group of tabs for navigation.
#[derive(Debug, Clone, Default)]
pub struct TabGroup<'a> {
    /// The tabs in this group
    pub tabs: Vec<Tab<'a>>,
    /// Optional aria-label for accessibility
    pub aria_label: Option<&'a str>,
}

impl<'a> TabGroup<'a> {
    /// Create a new empty tab group.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            aria_label: None,
        }
    }

    /// Add a tab to the group.
    #[must_use]
    pub fn add_tab(
        mut self,
        label: &'a str,
        href: &'a str,
        count: Option<i64>,
        active: bool,
    ) -> Self {
        let mut tab = Tab::new(label, href);
        if active {
            tab = tab.active();
        }
        if let Some(c) = count {
            tab = tab.with_count(c);
        }
        self.tabs.push(tab);
        self
    }

    /// Add a pre-configured tab to the group.
    #[must_use]
    pub fn push_tab(mut self, tab: Tab<'a>) -> Self {
        self.tabs.push(tab);
        self
    }

    /// Set the aria-label for accessibility.
    #[must_use]
    pub fn with_aria_label(mut self, label: &'a str) -> Self {
        self.aria_label = Some(label);
        self
    }
}

impl Render for TabGroup<'_> {
    fn render(&self) -> Markup {
        html! {
            nav class="archive-tabs" aria-label=[self.aria_label] {
                @for tab in &self.tabs {
                    (tab)
                }
            }
        }
    }
}

/// Create the standard archive list tabs (Recent, All, Failed).
///
/// This is a convenience function for creating the tabs used on the
/// archive listing pages.
#[must_use]
pub fn archive_list_tabs(active: ArchiveTab, failed_count: usize) -> TabGroup<'static> {
    let failed_count_opt = if failed_count > 0 {
        Some(failed_count as i64)
    } else {
        None
    };

    TabGroup::new()
        .with_aria_label("Archive list tabs")
        .add_tab("Recent", "/", None, active == ArchiveTab::Recent)
        .add_tab("All", "/archives/all", None, active == ArchiveTab::All)
        .add_tab(
            "Failed",
            "/archives/failed",
            failed_count_opt,
            active == ArchiveTab::Failed,
        )
}

/// Which archive tab is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveTab {
    /// Recent archives tab
    Recent,
    /// All archives tab
    All,
    /// Failed archives tab
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_new() {
        let tab = Tab::new("Test", "/test");
        assert_eq!(tab.label, "Test");
        assert_eq!(tab.href, "/test");
        assert!(!tab.active);
        assert!(tab.count.is_none());
    }

    #[test]
    fn test_tab_active() {
        let tab = Tab::new("Test", "/test").active();
        assert!(tab.active);
    }

    #[test]
    fn test_tab_with_count() {
        let tab = Tab::new("Test", "/test").with_count(42);
        assert_eq!(tab.count, Some(42));
    }

    #[test]
    fn test_tab_render_inactive() {
        let tab = Tab::new("Test", "/test");
        let html = tab.render().into_string();

        assert!(html.contains("class=\"archive-tab\""));
        assert!(html.contains("href=\"/test\""));
        assert!(html.contains("Test"));
        assert!(!html.contains("active"));
    }

    #[test]
    fn test_tab_render_active() {
        let tab = Tab::new("Test", "/test").active();
        let html = tab.render().into_string();

        assert!(html.contains("class=\"archive-tab active\""));
    }

    #[test]
    fn test_tab_render_with_count() {
        let tab = Tab::new("Failed", "/failed").with_count(5);
        let html = tab.render().into_string();

        assert!(html.contains("archive-tab-count"));
        assert!(html.contains(">5<"));
        assert!(html.contains("aria-label=\"5 items\""));
    }

    #[test]
    fn test_tab_render_with_zero_count() {
        let tab = Tab::new("Failed", "/failed").with_count(0);
        let html = tab.render().into_string();

        // Zero count should not show the badge
        assert!(!html.contains("archive-tab-count"));
    }

    #[test]
    fn test_tab_group_new() {
        let group = TabGroup::new();
        assert!(group.tabs.is_empty());
        assert!(group.aria_label.is_none());
    }

    #[test]
    fn test_tab_group_add_tab() {
        let group = TabGroup::new()
            .add_tab("Tab 1", "/tab1", None, true)
            .add_tab("Tab 2", "/tab2", Some(10), false);

        assert_eq!(group.tabs.len(), 2);
        assert!(group.tabs[0].active);
        assert!(!group.tabs[1].active);
        assert_eq!(group.tabs[1].count, Some(10));
    }

    #[test]
    fn test_tab_group_render() {
        let group = TabGroup::new()
            .with_aria_label("Test tabs")
            .add_tab("Tab 1", "/tab1", None, true)
            .add_tab("Tab 2", "/tab2", None, false);

        let html = group.render().into_string();

        assert!(html.contains("class=\"archive-tabs\""));
        assert!(html.contains("aria-label=\"Test tabs\""));
        assert!(html.contains("Tab 1"));
        assert!(html.contains("Tab 2"));
    }

    #[test]
    fn test_archive_list_tabs_recent() {
        let tabs = archive_list_tabs(ArchiveTab::Recent, 0);
        let html = tabs.render().into_string();

        assert!(html.contains("Recent"));
        assert!(html.contains("All"));
        assert!(html.contains("Failed"));
        // Recent should be active
        assert!(html.contains("href=\"/\""));
    }

    #[test]
    fn test_archive_list_tabs_with_failed_count() {
        let tabs = archive_list_tabs(ArchiveTab::Failed, 5);
        let html = tabs.render().into_string();

        // Failed tab should have count badge
        assert!(html.contains("archive-tab-count"));
        assert!(html.contains(">5<"));
    }

    #[test]
    fn test_archive_list_tabs_no_failed_count() {
        let tabs = archive_list_tabs(ArchiveTab::All, 0);
        let html = tabs.render().into_string();

        // No count badge when count is 0
        assert!(!html.contains("archive-tab-count"));
    }
}

// ==================== Content Tabs ====================
// These are JavaScript-powered tabs that show/hide content panels
// on the same page without navigation.

/// JavaScript for content tab switching functionality.
/// This is shared across all content tab instances.
const CONTENT_TAB_SCRIPT: &str = r#"
function showTab(tabId, btn) {
    document.querySelectorAll('.tab-content').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-nav button').forEach(b => b.classList.remove('active'));
    document.getElementById(tabId).classList.add('active');
    btn.classList.add('active');
}
"#;

/// A content tab panel with associated button.
#[derive(Debug, Clone)]
pub struct ContentTab {
    /// Unique ID for this tab's content panel.
    pub id: String,
    /// Display label for the tab button.
    pub label: String,
    /// Whether this tab is initially active.
    pub active: bool,
    /// The content to display in this tab panel.
    pub content: Markup,
}

impl ContentTab {
    /// Create a new content tab.
    #[must_use]
    pub fn new(id: &str, label: &str, content: Markup) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            active: false,
            content,
        }
    }

    /// Mark this tab as active.
    #[must_use]
    pub fn active(mut self) -> Self {
        self.active = true;
        self
    }
}

/// A group of content tabs with navigation and panels.
///
/// # Example
///
/// ```ignore
/// use crate::components::ContentTabs;
///
/// let tabs = ContentTabs::new()
///     .tab("profile-tab", "Profile", html! { p { "Profile content" } }, true)
///     .tab("settings-tab", "Settings", html! { p { "Settings content" } }, false);
///
/// html! {
///     (tabs)
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct ContentTabs {
    tabs: Vec<ContentTab>,
}

impl ContentTabs {
    /// Create a new empty content tabs component.
    #[must_use]
    pub fn new() -> Self {
        Self { tabs: Vec::new() }
    }

    /// Add a tab with its content.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique ID for this tab (used in HTML id attribute)
    /// * `label` - Display text for the tab button
    /// * `content` - The markup to display when this tab is active
    /// * `active` - Whether this tab should be initially active
    #[must_use]
    pub fn tab(mut self, id: &str, label: &str, content: Markup, active: bool) -> Self {
        let tab = if active {
            ContentTab::new(id, label, content).active()
        } else {
            ContentTab::new(id, label, content)
        };
        self.tabs.push(tab);
        self
    }

    /// Add a pre-configured content tab.
    #[must_use]
    pub fn push_tab(mut self, tab: ContentTab) -> Self {
        self.tabs.push(tab);
        self
    }
}

impl Render for ContentTabs {
    fn render(&self) -> Markup {
        html! {
            // Tab navigation
            nav class="tab-nav" {
                @for tab in &self.tabs {
                    @let class_attr = if tab.active { "active" } else { "" };
                    @let onclick_attr = format!("showTab('{}', this)", tab.id);
                    button type="button" class=(class_attr) onclick=(onclick_attr) {
                        (tab.label)
                    }
                }
            }

            // Tab switching script (only included once)
            script { (PreEscaped(CONTENT_TAB_SCRIPT)) }

            // Tab content panels
            @for tab in &self.tabs {
                @let content_class = if tab.active { "tab-content active" } else { "tab-content" };
                div id=(tab.id.as_str()) class=(content_class) {
                    (tab.content)
                }
            }
        }
    }
}

#[cfg(test)]
mod content_tabs_tests {
    use super::*;

    #[test]
    fn test_content_tab_new() {
        let tab = ContentTab::new("test-id", "Test Label", html! { p { "Content" } });
        assert_eq!(tab.id, "test-id");
        assert_eq!(tab.label, "Test Label");
        assert!(!tab.active);
    }

    #[test]
    fn test_content_tab_active() {
        let tab = ContentTab::new("test", "Test", html! {}).active();
        assert!(tab.active);
    }

    #[test]
    fn test_content_tabs_empty() {
        let tabs = ContentTabs::new();
        assert!(tabs.tabs.is_empty());
    }

    #[test]
    fn test_content_tabs_single_tab() {
        let tabs = ContentTabs::new().tab("tab1", "First Tab", html! { p { "Content 1" } }, true);

        let html_output = tabs.render().into_string();
        assert!(html_output.contains("tab-nav"));
        assert!(html_output.contains("First Tab"));
        assert!(html_output.contains("id=\"tab1\""));
        assert!(html_output.contains("tab-content active"));
        assert!(html_output.contains("Content 1"));
    }

    #[test]
    fn test_content_tabs_multiple_tabs() {
        let tabs = ContentTabs::new()
            .tab("tab1", "First", html! { p { "Content 1" } }, true)
            .tab("tab2", "Second", html! { p { "Content 2" } }, false);

        let html_output = tabs.render().into_string();

        // Check both tab buttons exist
        assert!(html_output.contains("First"));
        assert!(html_output.contains("Second"));

        // Check first tab is active
        assert!(html_output.contains("class=\"active\""));

        // Check both content panels exist
        assert!(html_output.contains("id=\"tab1\""));
        assert!(html_output.contains("id=\"tab2\""));
        assert!(html_output.contains("Content 1"));
        assert!(html_output.contains("Content 2"));
    }

    #[test]
    fn test_content_tabs_script_included() {
        let tabs = ContentTabs::new().tab("tab1", "Tab", html! { p { "Test" } }, true);

        let html_output = tabs.render().into_string();
        assert!(html_output.contains("function showTab"));
        assert!(html_output.contains("classList.remove('active')"));
    }

    #[test]
    fn test_content_tabs_onclick_handlers() {
        let tabs = ContentTabs::new()
            .tab("url-tab", "URL", html! {}, true)
            .tab("thread-tab", "Thread", html! {}, false);

        let html_output = tabs.render().into_string();
        assert!(html_output.contains("onclick=\"showTab('url-tab', this)\""));
        assert!(html_output.contains("onclick=\"showTab('thread-tab', this)\""));
    }
}
