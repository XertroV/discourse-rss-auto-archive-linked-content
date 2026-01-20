//! Table components for maud templates.
//!
//! This module provides reusable table components that match the styles
//! defined in `static/css/style.css`. Supports multiple table variants
//! including admin, artifacts, stats, jobs, and debug tables.

use maud::{html, Markup, Render};

/// Table variant determines the CSS class applied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TableVariant {
    /// Default table styling
    #[default]
    Default,
    /// Admin panel table (`.admin-table`)
    Admin,
    /// Artifacts listing table (`.artifacts-table`)
    Artifacts,
    /// Statistics table (`.stats-table`)
    Stats,
    /// Jobs/queue table (`.jobs-table`)
    Jobs,
    /// Debug/metadata table (`.debug-table`)
    Debug,
    /// Playlist table (`.playlist-table`)
    Playlist,
}

impl TableVariant {
    /// Get the CSS class for this variant.
    #[must_use]
    pub fn class(&self) -> Option<&'static str> {
        match self {
            TableVariant::Default => None,
            TableVariant::Admin => Some("admin-table"),
            TableVariant::Artifacts => Some("artifacts-table"),
            TableVariant::Stats => Some("stats-table"),
            TableVariant::Jobs => Some("jobs-table"),
            TableVariant::Debug => Some("debug-table"),
            TableVariant::Playlist => Some("playlist-table"),
        }
    }
}

/// A table element with headers and rows.
#[derive(Debug)]
pub struct Table<'a> {
    /// Table variant/style
    pub variant: TableVariant,
    /// Column headers
    pub headers: Vec<&'a str>,
    /// Pre-rendered row content
    pub rows: Vec<Markup>,
    /// Optional additional CSS class
    pub class: Option<&'a str>,
    /// Optional table ID
    pub id: Option<&'a str>,
}

impl<'a> Table<'a> {
    /// Create a new table with the given headers.
    #[must_use]
    pub fn new(headers: Vec<&'a str>) -> Self {
        Self {
            variant: TableVariant::Default,
            headers,
            rows: Vec::new(),
            class: None,
            id: None,
        }
    }

    /// Set the table variant.
    #[must_use]
    pub fn variant(mut self, variant: TableVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Add a pre-rendered row.
    #[must_use]
    pub fn add_row(mut self, row: Markup) -> Self {
        self.rows.push(row);
        self
    }

    /// Add multiple pre-rendered rows.
    #[must_use]
    pub fn rows(mut self, rows: Vec<Markup>) -> Self {
        self.rows = rows;
        self
    }

    /// Add an additional CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Set the table ID.
    #[must_use]
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Build the combined class string.
    fn build_class(&self) -> Option<String> {
        match (self.variant.class(), self.class) {
            (Some(variant_class), Some(extra_class)) => {
                Some(format!("{variant_class} {extra_class}"))
            }
            (Some(variant_class), None) => Some(variant_class.to_string()),
            (None, Some(extra_class)) => Some(extra_class.to_string()),
            (None, None) => None,
        }
    }
}

impl Render for Table<'_> {
    fn render(&self) -> Markup {
        let class_str = self.build_class();
        html! {
            table class=[class_str.as_deref()] id=[self.id] {
                @if !self.headers.is_empty() {
                    thead {
                        tr {
                            @for header in &self.headers {
                                th { (header) }
                            }
                        }
                    }
                }
                tbody {
                    @for row in &self.rows {
                        (row)
                    }
                }
            }
        }
    }
}

/// A table row with cells.
#[derive(Debug, Default)]
pub struct TableRow {
    /// Pre-rendered cell content
    pub cells: Vec<Markup>,
    /// Optional CSS class for the row
    pub class: Option<String>,
    /// Optional data attributes
    pub data_attrs: Vec<(String, String)>,
}

impl TableRow {
    /// Create a new empty table row.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a cell with text content.
    #[must_use]
    pub fn cell(mut self, content: &str) -> Self {
        self.cells.push(html! { td { (content) } });
        self
    }

    /// Add a cell with pre-rendered markup.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // Markup is idiomatically passed by value
    pub fn cell_markup(mut self, content: Markup) -> Self {
        self.cells.push(html! { td { (content) } });
        self
    }

    /// Add a cell with a custom CSS class.
    #[must_use]
    pub fn cell_with_class(mut self, content: &str, class: &str) -> Self {
        self.cells.push(html! { td class=(class) { (content) } });
        self
    }

    /// Add a cell with markup and a custom CSS class.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // Markup is idiomatically passed by value
    pub fn cell_markup_with_class(mut self, content: Markup, class: &str) -> Self {
        self.cells.push(html! { td class=(class) { (content) } });
        self
    }

    /// Set the row CSS class.
    #[must_use]
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    /// Add a data attribute to the row.
    #[must_use]
    pub fn data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data_attrs.push((key.into(), value.into()));
        self
    }
}

impl Render for TableRow {
    fn render(&self) -> Markup {
        // Build data attributes string manually since maud doesn't support dynamic attribute names
        let data_attrs_str = self
            .data_attrs
            .iter()
            .map(|(k, v)| format!(r#"data-{}="{}""#, html_escape(k), html_escape(v)))
            .collect::<Vec<_>>()
            .join(" ");

        if data_attrs_str.is_empty() {
            html! {
                tr class=[self.class.as_deref()] {
                    @for cell in &self.cells {
                        (cell)
                    }
                }
            }
        } else {
            // Use PreEscaped to inject the data attributes
            let class_attr = self
                .class
                .as_ref()
                .map(|c| format!(r#" class="{}""#, html_escape(c)))
                .unwrap_or_default();
            html! {
                (maud::PreEscaped(format!("<tr{} {}>", class_attr, data_attrs_str)))
                @for cell in &self.cells {
                    (cell)
                }
                (maud::PreEscaped("</tr>"))
            }
        }
    }
}

/// Escape HTML special characters in a string.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A simple helper to create a table row from string cells.
#[must_use]
pub fn simple_row(cells: &[&str]) -> Markup {
    html! {
        tr {
            @for cell in cells {
                td { (*cell) }
            }
        }
    }
}

/// A simple helper to create a table row from markup cells.
#[must_use]
pub fn markup_row(cells: Vec<Markup>) -> Markup {
    html! {
        tr {
            @for cell in cells {
                td { (cell) }
            }
        }
    }
}

/// A table cell element for more fine-grained control.
#[derive(Debug)]
pub struct TableCell<'a> {
    /// Cell content
    pub content: Markup,
    /// Optional CSS class
    pub class: Option<&'a str>,
    /// Column span
    pub colspan: Option<u32>,
    /// Row span
    pub rowspan: Option<u32>,
    /// Whether this is a header cell (th instead of td)
    pub is_header: bool,
}

impl<'a> TableCell<'a> {
    /// Create a new table cell with text content.
    #[must_use]
    pub fn new(content: &str) -> Self {
        Self {
            content: html! { (content) },
            class: None,
            colspan: None,
            rowspan: None,
            is_header: false,
        }
    }

    /// Create a new table cell with markup content.
    #[must_use]
    pub fn markup(content: Markup) -> Self {
        Self {
            content,
            class: None,
            colspan: None,
            rowspan: None,
            is_header: false,
        }
    }

    /// Create a header cell (th).
    #[must_use]
    pub fn header(content: &str) -> Self {
        Self {
            content: html! { (content) },
            class: None,
            colspan: None,
            rowspan: None,
            is_header: true,
        }
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Set the column span.
    #[must_use]
    pub fn colspan(mut self, colspan: u32) -> Self {
        self.colspan = Some(colspan);
        self
    }

    /// Set the row span.
    #[must_use]
    pub fn rowspan(mut self, rowspan: u32) -> Self {
        self.rowspan = Some(rowspan);
        self
    }
}

impl Render for TableCell<'_> {
    fn render(&self) -> Markup {
        if self.is_header {
            html! {
                th class=[self.class] colspan=[self.colspan] rowspan=[self.rowspan] {
                    (self.content)
                }
            }
        } else {
            html! {
                td class=[self.class] colspan=[self.colspan] rowspan=[self.rowspan] {
                    (self.content)
                }
            }
        }
    }
}

/// A key-value table (two columns: label and value).
/// Useful for debug tables and metadata display.
#[derive(Debug)]
pub struct KeyValueTable<'a> {
    /// Key-value pairs
    pub items: Vec<(&'a str, Markup)>,
    /// Table variant
    pub variant: TableVariant,
    /// Optional CSS class
    pub class: Option<&'a str>,
}

impl<'a> KeyValueTable<'a> {
    /// Create a new key-value table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            variant: TableVariant::Default,
            class: None,
        }
    }

    /// Add a key-value pair with text value.
    #[must_use]
    pub fn item(mut self, key: &'a str, value: &str) -> Self {
        self.items.push((key, html! { (value) }));
        self
    }

    /// Add a key-value pair with markup value.
    #[must_use]
    pub fn item_markup(mut self, key: &'a str, value: Markup) -> Self {
        self.items.push((key, value));
        self
    }

    /// Set the table variant.
    #[must_use]
    pub fn variant(mut self, variant: TableVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set additional CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }
}

impl Default for KeyValueTable<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for KeyValueTable<'_> {
    fn render(&self) -> Markup {
        let variant_class = self.variant.class();
        let class_str = match (variant_class, self.class) {
            (Some(v), Some(c)) => Some(format!("{v} {c}")),
            (Some(v), None) => Some(v.to_string()),
            (None, Some(c)) => Some(c.to_string()),
            (None, None) => None,
        };

        html! {
            table class=[class_str.as_deref()] {
                tbody {
                    @for (key, value) in &self.items {
                        tr {
                            th { (*key) }
                            td { (value) }
                        }
                    }
                }
            }
        }
    }
}

/// A responsive wrapper for tables that enables horizontal scrolling on small screens.
#[derive(Debug)]
pub struct ResponsiveTable {
    /// The table content
    pub table: Markup,
}

impl ResponsiveTable {
    /// Create a new responsive table wrapper.
    #[must_use]
    pub fn new(table: Markup) -> Self {
        Self { table }
    }
}

impl Render for ResponsiveTable {
    fn render(&self) -> Markup {
        html! {
            div style="overflow-x: auto;" {
                (self.table)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_variant_class() {
        assert_eq!(TableVariant::Default.class(), None);
        assert_eq!(TableVariant::Admin.class(), Some("admin-table"));
        assert_eq!(TableVariant::Artifacts.class(), Some("artifacts-table"));
        assert_eq!(TableVariant::Stats.class(), Some("stats-table"));
        assert_eq!(TableVariant::Jobs.class(), Some("jobs-table"));
        assert_eq!(TableVariant::Debug.class(), Some("debug-table"));
        assert_eq!(TableVariant::Playlist.class(), Some("playlist-table"));
    }

    #[test]
    fn test_table_render() {
        let table = Table::new(vec!["Name", "Value"])
            .add_row(simple_row(&["foo", "bar"]))
            .add_row(simple_row(&["baz", "qux"]));
        let html = table.render().into_string();

        assert!(html.contains("<table"));
        assert!(html.contains("<thead>"));
        assert!(html.contains("<th>Name</th>"));
        assert!(html.contains("<th>Value</th>"));
        assert!(html.contains("<tbody>"));
        assert!(html.contains("<td>foo</td>"));
        assert!(html.contains("<td>bar</td>"));
    }

    #[test]
    fn test_table_with_variant() {
        let table = Table::new(vec!["Col"]).variant(TableVariant::Admin);
        let html = table.render().into_string();

        assert!(html.contains(r#"class="admin-table""#));
    }

    #[test]
    fn test_table_with_combined_class() {
        let table = Table::new(vec!["Col"])
            .variant(TableVariant::Stats)
            .class("extra-class");
        let html = table.render().into_string();

        assert!(html.contains(r#"class="stats-table extra-class""#));
    }

    #[test]
    fn test_table_with_id() {
        let table = Table::new(vec!["Col"]).id("my-table");
        let html = table.render().into_string();

        assert!(html.contains(r#"id="my-table""#));
    }

    #[test]
    fn test_table_no_headers() {
        let table = Table::new(vec![]).add_row(simple_row(&["data"]));
        let html = table.render().into_string();

        assert!(!html.contains("<thead>"));
        assert!(html.contains("<tbody>"));
        assert!(html.contains("<td>data</td>"));
    }

    #[test]
    fn test_table_row_render() {
        let row = TableRow::new()
            .cell("first")
            .cell("second")
            .class("highlight");
        let html = row.render().into_string();

        assert!(html.contains(r#"<tr class="highlight""#));
        assert!(html.contains("<td>first</td>"));
        assert!(html.contains("<td>second</td>"));
    }

    #[test]
    fn test_table_row_with_markup() {
        let cell_content = html! { a href="/link" { "Click me" } };
        let row = TableRow::new().cell("text").cell_markup(cell_content);
        let html = row.render().into_string();

        assert!(html.contains("<td>text</td>"));
        assert!(html.contains(r#"<td><a href="/link">Click me</a></td>"#));
    }

    #[test]
    fn test_table_row_with_data_attrs() {
        let row = TableRow::new()
            .cell("data")
            .data("id", "123")
            .data("status", "active");
        let html = row.render().into_string();

        assert!(html.contains(r#"data-id="123""#));
        assert!(html.contains(r#"data-status="active""#));
    }

    #[test]
    fn test_table_row_cell_with_class() {
        let row = TableRow::new().cell_with_class("status", "status-complete");
        let html = row.render().into_string();

        assert!(html.contains(r#"<td class="status-complete">status</td>"#));
    }

    #[test]
    fn test_simple_row() {
        let row = simple_row(&["a", "b", "c"]);
        let html = row.into_string();

        assert!(html.contains("<tr>"));
        assert!(html.contains("<td>a</td>"));
        assert!(html.contains("<td>b</td>"));
        assert!(html.contains("<td>c</td>"));
    }

    #[test]
    fn test_markup_row() {
        let cells = vec![html! { "plain" }, html! { strong { "bold" } }];
        let row = markup_row(cells);
        let html = row.into_string();

        assert!(html.contains("<td>plain</td>"));
        assert!(html.contains("<td><strong>bold</strong></td>"));
    }

    #[test]
    fn test_table_cell_text() {
        let cell = TableCell::new("content");
        let html = cell.render().into_string();

        assert!(html.contains("<td>content</td>"));
    }

    #[test]
    fn test_table_cell_header() {
        let cell = TableCell::header("Header");
        let html = cell.render().into_string();

        assert!(html.contains("<th>Header</th>"));
    }

    #[test]
    fn test_table_cell_with_spans() {
        let cell = TableCell::new("merged").colspan(2).rowspan(3);
        let html = cell.render().into_string();

        assert!(html.contains(r#"colspan="2""#));
        assert!(html.contains(r#"rowspan="3""#));
    }

    #[test]
    fn test_table_cell_with_class() {
        let cell = TableCell::new("styled").class("highlight");
        let html = cell.render().into_string();

        assert!(html.contains(r#"class="highlight""#));
    }

    #[test]
    fn test_key_value_table() {
        let table = KeyValueTable::new()
            .item("Name", "John")
            .item("Age", "30")
            .variant(TableVariant::Debug);
        let html = table.render().into_string();

        assert!(html.contains(r#"class="debug-table""#));
        assert!(html.contains("<th>Name</th>"));
        assert!(html.contains("<td>John</td>"));
        assert!(html.contains("<th>Age</th>"));
        assert!(html.contains("<td>30</td>"));
    }

    #[test]
    fn test_key_value_table_with_markup() {
        let link = html! { a href="/profile" { "View" } };
        let table = KeyValueTable::new()
            .item("Key", "Value")
            .item_markup("Action", link);
        let html = table.render().into_string();

        assert!(html.contains("<th>Action</th>"));
        assert!(html.contains(r#"<td><a href="/profile">View</a></td>"#));
    }

    #[test]
    fn test_responsive_table() {
        let inner_table = Table::new(vec!["A", "B"]).render();
        let responsive = ResponsiveTable::new(inner_table);
        let html = responsive.render().into_string();

        assert!(html.contains(r#"style="overflow-x: auto;""#));
        assert!(html.contains("<table"));
    }

    #[test]
    fn test_table_rows_method() {
        let rows = vec![simple_row(&["1", "a"]), simple_row(&["2", "b"])];
        let table = Table::new(vec!["Num", "Letter"]).rows(rows);
        let html = table.render().into_string();

        assert!(html.contains("<td>1</td>"));
        assert!(html.contains("<td>a</td>"));
        assert!(html.contains("<td>2</td>"));
        assert!(html.contains("<td>b</td>"));
    }

    #[test]
    fn test_full_table_example() {
        // Demonstrate a complete admin table usage
        let row1 = TableRow::new()
            .cell("user1")
            .cell_markup(html! { span class="status-approved" { "Approved" } })
            .class("current-user");

        let row2 = TableRow::new()
            .cell("user2")
            .cell_markup(html! { span class="status-pending" { "Pending" } });

        let table = Table::new(vec!["Username", "Status"])
            .variant(TableVariant::Admin)
            .add_row(row1.render())
            .add_row(row2.render());

        let html = table.render().into_string();

        assert!(html.contains(r#"class="admin-table""#));
        assert!(html.contains("<th>Username</th>"));
        assert!(html.contains("<th>Status</th>"));
        assert!(html.contains(r#"class="current-user""#));
        assert!(html.contains("user1"));
        assert!(html.contains("status-approved"));
        assert!(html.contains("user2"));
        assert!(html.contains("status-pending"));
    }
}
