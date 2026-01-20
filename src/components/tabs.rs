//! Tab components for navigation between related views.
//!
//! This module provides reusable tab components for creating
//! tabbed navigation interfaces.

use maud::{html, Markup, Render};

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
