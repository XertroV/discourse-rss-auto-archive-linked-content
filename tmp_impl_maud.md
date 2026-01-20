# Maud Migration Implementation Plan

Temporary implementation document for migrating from `format!()` string templates to maud macro-based HTML generation with a typed component system.

**Goal**: Replace 3500+ lines of manual HTML string formatting with type-safe, composable maud components that enforce consistent CSS class usage.

---

## Table of Contents

1. [Overview](#overview)
2. [Phase 1: Component Foundation](#phase-1-component-foundation)
3. [Phase 2: Page Migration](#phase-2-page-migration)
4. [Style Guide & Component Reference](#style-guide--component-reference)
5. [Testing Strategy](#testing-strategy)
6. [Maud Best Practices](#maud-best-practices)
7. [Potential Issues & Mitigations](#potential-issues--mitigations)
8. [Parallel Development Structure](#parallel-development-structure)
9. [Migration Checklist](#migration-checklist)

---

## Overview

### Current State

- **File**: `src/web/templates.rs` (~3500 lines)
- **Pattern**: `format!(r#"<html>..."#, var1, var2)` with manual `html_escape()` calls
- **Problems**:
  - No compile-time HTML validation
  - Manual escaping error-prone
  - No component reuse enforcement
  - CSS classes used inconsistently

### Target State

- **Pattern**: Maud `html!{}` macro with typed component structs
- **Benefits**:
  - Compile-time HTML validation
  - Automatic HTML escaping
  - Enforced component variants (enums for CSS classes)
  - IDE support (autocomplete, refactoring)

### Dependencies

```toml
# Cargo.toml changes
[dependencies]
maud = { version = "0.26", features = ["axum"] }

# Remove (currently unused):
# askama = "0.12"
# askama_axum = "0.4"
```

---

## Phase 1: Component Foundation

**Goal**: Create the component module with all reusable primitives. Verify HTML output matches expectations before touching any pages.

### 1.1 Module Structure

```
src/
├── components/
│   ├── mod.rs              # Public exports
│   ├── layout.rs           # BaseLayout, PageHead, Header, Footer
│   ├── button.rs           # Button, ButtonVariant
│   ├── badge.rs            # Badge, StatusBadge, DomainBadge, MediaTypeBadge, NsfwBadge
│   ├── card.rs             # Card, ArchiveCard
│   ├── form.rs             # Form, Input, TextArea, Select, Label
│   ├── table.rs            # Table, TableHead, TableRow
│   ├── alert.rs            # Alert, AlertVariant (success, error, warning, info)
│   ├── pagination.rs       # Pagination
│   ├── tabs.rs             # TabGroup, Tab
│   ├── media.rs            # VideoPlayer, AudioPlayer, ImageViewer, MediaContainer
│   ├── nav.rs              # NavItem, NavGroup
│   └── icons.rs            # Icon (view, download, etc.)
└── web/
    ├── mod.rs
    ├── routes.rs
    └── templates.rs        # Gradually replaced, eventually deleted
```

### 1.2 Component Implementation Order

Each component should be implemented and tested before moving to the next:

1. **layout.rs** - Foundation for all pages
2. **button.rs** - Used everywhere
3. **badge.rs** - Status, domain, media type badges
4. **alert.rs** - Error/success messages
5. **card.rs** - Archive cards (most complex, most used)
6. **form.rs** - Submit page, search, login
7. **table.rs** - Admin panel, job history
8. **pagination.rs** - List pages
9. **tabs.rs** - Archive tabs (Recent/All/Failed)
10. **media.rs** - Video/audio players
11. **nav.rs** - Header navigation
12. **icons.rs** - SVG icons

### 1.3 Component Design Pattern

Each component follows this pattern:

```rust
// src/components/button.rs
use maud::{html, Markup, Render};

/// Button variants map to CSS classes
#[derive(Debug, Clone, Copy, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Outline,
    Danger,
    Ghost,
}

impl ButtonVariant {
    /// Returns the CSS classes for this variant
    fn classes(&self) -> &'static str {
        match self {
            Self::Primary => "btn",
            Self::Outline => "btn outline",
            Self::Danger => "btn btn-danger",
            Self::Ghost => "btn btn-ghost",
        }
    }
}

/// Button component with typed variants
#[derive(Debug)]
pub struct Button<'a> {
    pub label: &'a str,
    pub variant: ButtonVariant,
    pub href: Option<&'a str>,
    pub target_blank: bool,
    pub disabled: bool,
    pub r#type: Option<&'a str>,  // "submit", "button", etc.
}

impl<'a> Button<'a> {
    /// Create a primary button
    pub fn primary(label: &'a str) -> Self {
        Self {
            label,
            variant: ButtonVariant::Primary,
            href: None,
            target_blank: false,
            disabled: false,
            r#type: None,
        }
    }

    /// Create an outline button
    pub fn outline(label: &'a str) -> Self {
        Self {
            label,
            variant: ButtonVariant::Outline,
            ..Self::primary(label)
        }
    }

    /// Builder: set href (converts to <a> tag)
    pub fn href(mut self, url: &'a str) -> Self {
        self.href = Some(url);
        self
    }

    /// Builder: open in new tab
    pub fn target_blank(mut self) -> Self {
        self.target_blank = true;
        self
    }

    /// Builder: set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Render for Button<'_> {
    fn render(&self) -> Markup {
        let classes = self.variant.classes();

        match self.href {
            Some(url) => html! {
                a class=(classes) href=(url)
                    target=[self.target_blank.then_some("_blank")]
                    rel=[self.target_blank.then_some("noopener noreferrer")] {
                    (self.label)
                }
            },
            None => html! {
                button class=(classes)
                    type=(self.r#type.unwrap_or("button"))
                    disabled[self.disabled] {
                    (self.label)
                }
            },
        }
    }
}
```

### 1.4 Layout Component (Critical)

The layout component is the foundation. It handles:
- HTML document structure
- Theme/NSFW JavaScript
- Header with auth state
- Footer

```rust
// src/components/layout.rs
use maud::{html, Markup, DOCTYPE, PreEscaped};
use crate::db::User;

/// Scripts that need to run in <head> before body renders
const HEAD_SCRIPTS: &str = r#"
(function() {
    var theme = localStorage.getItem('theme');
    if (theme) {
        document.documentElement.setAttribute('data-theme', theme);
    } else if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
        document.documentElement.setAttribute('data-theme', 'dark');
    }
})();
"#;

/// Scripts that run after body loads
const BODY_SCRIPTS: &str = r#"
(function() {
    // Theme toggle
    var themeToggle = document.getElementById('theme-toggle');
    if (themeToggle) {
        themeToggle.addEventListener('click', function() {
            var current = document.documentElement.getAttribute('data-theme') || 'light';
            var next = current === 'dark' ? 'light' : 'dark';
            document.documentElement.setAttribute('data-theme', next);
            localStorage.setItem('theme', next);
        });
    }

    // NSFW toggle
    var nsfwToggle = document.getElementById('nsfw-toggle');
    if (nsfwToggle) {
        var nsfwEnabled = localStorage.getItem('nsfw_enabled') === 'true';
        if (nsfwEnabled) {
            document.body.classList.remove('nsfw-hidden');
            nsfwToggle.classList.add('active');
        }
        nsfwToggle.addEventListener('click', function() {
            var enabled = !document.body.classList.contains('nsfw-hidden');
            if (enabled) {
                document.body.classList.add('nsfw-hidden');
                nsfwToggle.classList.remove('active');
                localStorage.setItem('nsfw_enabled', 'false');
            } else {
                document.body.classList.remove('nsfw-hidden');
                nsfwToggle.classList.add('active');
                localStorage.setItem('nsfw_enabled', 'true');
            }
        });
    }
})();
"#;

pub struct BaseLayout<'a> {
    pub title: &'a str,
    pub user: Option<&'a User>,
}

impl<'a> BaseLayout<'a> {
    pub fn new(title: &'a str) -> Self {
        Self { title, user: None }
    }

    pub fn with_user(mut self, user: Option<&'a User>) -> Self {
        self.user = user;
        self
    }

    pub fn render(&self, content: Markup) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" data-theme="light" {
                head {
                    meta charset="UTF-8";
                    meta name="viewport" content="width=device-width, initial-scale=1.0";
                    meta name="color-scheme" content="light dark";
                    meta name="robots" content="noarchive";
                    meta name="x-no-archive" content="1";
                    title { (self.title) " - Discourse Link Archiver" }
                    link rel="stylesheet" href="/static/css/style.css";
                    link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>📦</text></svg>";
                    link rel="alternate" type="application/rss+xml" title="Archive RSS Feed" href="/feed.rss";
                    link rel="alternate" type="application/atom+xml" title="Archive Atom Feed" href="/feed.atom";
                    script { (PreEscaped(HEAD_SCRIPTS)) }
                }
                body.nsfw-hidden {
                    (self.render_header())
                    main.container { (content) }
                    (self.render_footer())
                    script { (PreEscaped(BODY_SCRIPTS)) }
                }
            }
        }
    }

    fn render_header(&self) -> Markup {
        html! {
            header.container {
                nav {
                    ul {
                        li { a href="/" { strong { "Archive" } } }
                    }
                    ul {
                        li { a href="/" { "Home" } }
                        li { a href="/threads" { "Threads" } }
                        li { a href="/search" { "Search" } }
                        li { a href="/submit" { "Submit" } }
                        li { a href="/stats" { "Stats" } }
                        (self.render_auth_nav())
                        li {
                            button #nsfw-toggle .nsfw-toggle title="Toggle NSFW content visibility"
                                aria-label="Toggle NSFW content" { "18+" }
                        }
                        li {
                            button #theme-toggle .theme-toggle title="Toggle dark mode"
                                aria-label="Toggle dark mode" { "🌓" }
                        }
                    }
                }
            }
        }
    }

    fn render_auth_nav(&self) -> Markup {
        match self.user {
            Some(user) if user.is_admin => html! {
                li { a href="/profile" { "Profile" } }
                li { a href="/admin" { "Admin" } }
            },
            Some(_) => html! {
                li { a href="/profile" { "Profile" } }
            },
            None => html! {
                li { a href="/login" { "Login" } }
            },
        }
    }

    fn render_footer(&self) -> Markup {
        html! {
            footer.container {
                small {
                    "Discourse Link Archiver | "
                    a href="/feed.rss" { "RSS" }
                    " | "
                    a href="/feed.atom" { "Atom" }
                }
            }
        }
    }
}
```

---

## Phase 2: Page Migration

**Goal**: Replace each template function with maud equivalents, one at a time.

### 2.1 Migration Order

Pages ordered by complexity and dependency:

| Order | Page | Template Function | Complexity | Dependencies |
|-------|------|-------------------|------------|--------------|
| 1 | Home | `render_home_paginated` | Medium | Layout, ArchiveCard, Pagination, Tabs |
| 2 | Archive Detail | `render_archive_detail` | High | Layout, Card, Badge, MediaPlayer, Table |
| 3 | Search | `render_search` | Medium | Layout, ArchiveCard, Pagination, Form |
| 4 | Stats | `render_stats` | Low | Layout, Card |
| 5 | Threads List | `render_threads_list` | Medium | Layout, Card, Pagination |
| 6 | Thread Detail | `render_thread_detail` | Medium | Layout, ArchiveCard |
| 7 | Post Detail | `render_post_detail` | Medium | Layout, ArchiveCard |
| 8 | Site List | `render_site_list` | Medium | Layout, ArchiveCard, Pagination |
| 9 | Submit Form | `render_submit_form` | Medium | Layout, Form, Alert |
| 10 | Login | `login_page` | Low | Layout, Form, Alert |
| 11 | Profile | `profile_page` | Low | Layout, Form, Alert |
| 12 | Admin Panel | `admin_panel` | High | Layout, Table, Form, Button |
| 13 | Debug Queue | `render_debug_queue` | Medium | Layout, Table, Card |
| 14 | Comparison | `render_comparison` | Medium | Layout, Card |
| 15 | Archive Banner | `render_archive_banner` | Special | Standalone (injected into archived HTML) |

### 2.2 Migration Process Per Page

For each page:

1. **Create snapshot test** (if not exists)
2. **Create new maud version** in `src/web/pages/{page}.rs`
3. **Run snapshot comparison**
4. **Update route handler** to use new version
5. **Run full test suite**
6. **Delete old template function**

### 2.3 Route Handler Updates

Current pattern:
```rust
async fn home(...) -> Response {
    let html = templates::render_home_paginated(...);
    Html(html).into_response()
}
```

New pattern (maud `Markup` implements `IntoResponse`):
```rust
async fn home(...) -> Markup {
    pages::home::render(...)
}
```

Or during transition:
```rust
async fn home(...) -> Response {
    let markup = pages::home::render(...);
    Html(markup.into_string()).into_response()
}
```

---

## Style Guide & Component Reference

### CSS Class Inventory

All CSS classes from `static/css/style.css` that components must use:

#### Layout Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `.container` | Max-width wrapper | Layout |
| `header.container` | Header wrapper | Layout |
| `main.container` | Main content wrapper | Layout |
| `footer.container` | Footer wrapper | Layout |

#### Button Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `.btn` | Base button | Button |
| `.btn.outline` | Outline variant | Button |
| `.btn-danger` | Danger variant | Button |
| `.theme-toggle` | Theme toggle button | Layout |
| `.nsfw-toggle` | NSFW toggle button | Layout |
| `.nsfw-toggle.active` | NSFW enabled state | Layout |

#### Card Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `.archive-card` | Archive card container | ArchiveCard |
| `.archive-card h3` | Card title | ArchiveCard |
| `.archive-card .meta` | Metadata row | ArchiveCard |
| `.archive-card .author` | Author display | ArchiveCard |
| `.archive-url` | URL display | ArchiveCard |
| `.archive-time` | Timestamp | ArchiveCard |
| `.archive-grid` | Grid container | ArchiveGrid |

#### Badge Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `.status-complete` | Complete status | StatusBadge |
| `.status-pending` | Pending status | StatusBadge |
| `.status-failed` | Failed status | StatusBadge |
| `.status-skipped` | Skipped status | StatusBadge |
| `.status-processing` | Processing status | StatusBadge |
| `.domain-badge` | Domain link | DomainBadge |
| `.media-type-badge` | Base media badge | MediaTypeBadge |
| `.media-type-video` | Video type | MediaTypeBadge |
| `.media-type-audio` | Audio type | MediaTypeBadge |
| `.media-type-image` | Image type | MediaTypeBadge |
| `.media-type-text` | Text type | MediaTypeBadge |
| `.nsfw-badge` | NSFW indicator | NsfwBadge |
| `.nsfw-warning` | NSFW warning box | NsfwWarning |

#### Tab Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `.archive-tabs` | Tab container | TabGroup |
| `.archive-tab` | Individual tab | Tab |
| `.archive-tab.active` | Active tab | Tab |
| `.archive-tab-count` | Count badge in tab | Tab |

#### Form Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `form` | Form container | Form |
| `input` | Text input | Input |
| `textarea` | Multi-line input | TextArea |
| `select` | Dropdown | Select |
| `label` | Form label | Label |
| `small` | Helper text | FormHelp |

#### Alert/Message Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `.success-message` | Success alert | Alert::Success |
| `.error-message` | Error alert | Alert::Error |
| `article.success` | Success article | Alert::Success |
| `article.error` | Error article | Alert::Error |

#### Table Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `table` | Base table | Table |
| `th` | Header cell | TableHead |
| `td` | Data cell | TableRow |
| `tr:hover` | Row hover | TableRow |

#### Media Classes
| Class | Usage | Component |
|-------|-------|-----------|
| `.media-container` | Media wrapper | MediaContainer |
| `.video-wrapper` | Video aspect ratio | VideoPlayer |

#### Data Attributes
| Attribute | Usage | Component |
|-----------|-------|-----------|
| `data-nsfw="true"` | Mark NSFW content | ArchiveCard |
| `data-nsfw-hidden="true"` | Show when NSFW hidden | NsfwHiddenMessage |
| `data-theme="light\|dark"` | Theme state | Layout (on `<html>`) |

### Component Variant Mappings

```rust
// Badge variants -> CSS classes
pub enum StatusVariant {
    Complete,   // -> "status-complete"
    Pending,    // -> "status-pending"
    Failed,     // -> "status-failed"
    Skipped,    // -> "status-skipped"
    Processing, // -> "status-processing"
}

pub enum MediaTypeVariant {
    Video,  // -> "media-type-badge media-type-video"
    Audio,  // -> "media-type-badge media-type-audio"
    Image,  // -> "media-type-badge media-type-image"
    Text,   // -> "media-type-badge media-type-text"
}

pub enum AlertVariant {
    Success, // -> "success-message"
    Error,   // -> "error-message"
    Warning, // -> "warning-message" (may need to add to CSS)
    Info,    // -> "info-message" (may need to add to CSS)
}
```

---

## Testing Strategy

### 5.1 Snapshot Tests (Before Migration)

Create snapshot tests for key templates before migrating:

```rust
// tests/template_snapshots.rs
use discourse_link_archiver::web::templates;
use insta::assert_snapshot;

#[test]
fn snapshot_home_page_empty() {
    let html = templates::render_home_paginated(
        &[],  // no archives
        0,    // no failed
        0,    // page 0
        0,    // total pages
        "/",
        None, // no content filter
        None, // no source filter
        None, // no user
    );
    assert_snapshot!(html);
}

#[test]
fn snapshot_archive_card() {
    let archive = create_test_archive_display();
    let html = templates::render_archive_card_display(&archive);
    assert_snapshot!(html);
}

// ... more snapshots
```

Add `insta` dev dependency:
```toml
[dev-dependencies]
insta = "1.34"
```

### 5.2 Component Unit Tests

Each component gets unit tests:

```rust
// src/components/button.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_button_renders_correct_class() {
        let html = Button::primary("Click me").render().into_string();
        assert!(html.contains("class=\"btn\""));
        assert!(html.contains(">Click me<"));
    }

    #[test]
    fn outline_button_renders_correct_class() {
        let html = Button::outline("Cancel").render().into_string();
        assert!(html.contains("class=\"btn outline\""));
    }

    #[test]
    fn button_with_href_renders_anchor() {
        let html = Button::primary("Go").href("/home").render().into_string();
        assert!(html.contains("<a"));
        assert!(html.contains("href=\"/home\""));
    }

    #[test]
    fn button_escapes_label() {
        let html = Button::primary("<script>alert('xss')</script>")
            .render()
            .into_string();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
```

### 5.3 Integration Tests

Add integration tests that verify routes return expected HTML structure:

```rust
// tests/maud_integration.rs
#[tokio::test]
async fn home_page_contains_expected_elements() {
    let app = create_test_app().await;
    let response = app.get("/").await;

    let html = response.text().await;

    // Structure checks
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<html lang=\"en\""));
    assert!(html.contains("data-theme=\"light\""));

    // Header checks
    assert!(html.contains("<header"));
    assert!(html.contains("Archive</strong>"));
    assert!(html.contains("href=\"/search\""));

    // Footer checks
    assert!(html.contains("<footer"));
    assert!(html.contains("feed.rss"));
}

#[tokio::test]
async fn archive_card_has_correct_data_attributes() {
    // Create NSFW archive
    let archive_id = create_test_nsfw_archive().await;

    let app = create_test_app().await;
    let response = app.get("/").await;
    let html = response.text().await;

    // NSFW archive should have data-nsfw attribute
    assert!(html.contains("data-nsfw=\"true\""));
}
```

### 5.4 Visual Regression (Manual)

Checklist for manual visual testing during migration:

- [ ] Light mode appearance matches
- [ ] Dark mode appearance matches
- [ ] NSFW toggle works
- [ ] Theme toggle works
- [ ] Responsive on mobile viewport
- [ ] All links work
- [ ] Forms submit correctly

---

## Maud Best Practices

### 6.1 JavaScript Handling

**Best Practice**: Extract JS to separate files, load via `<script src>`.

```rust
// Instead of inline JS in templates:
script { (PreEscaped(LONG_JS_CODE)) }

// Prefer external files:
script src="/static/js/theme.js" defer {}
script src="/static/js/nsfw.js" defer {}
```

Create:
- `static/js/theme.js` - Theme toggle logic
- `static/js/nsfw.js` - NSFW filter logic
- `static/js/transcript.js` - Transcript viewer (if complex)

**Exception**: Critical rendering JS (theme flicker prevention) stays in `<head>` as inline script.

### 6.2 CSS Handling

**Best Practice**: All CSS in `static/css/style.css`. No inline styles.

Current templates have ~150 lines of inline `<style>` in `base_layout`. These should be:

1. **Moved to style.css** (preferred)
2. Or loaded as a separate CSS file

```rust
// DON'T do this in maud:
style { (PreEscaped(".my-class { color: red; }")) }

// DO this:
link rel="stylesheet" href="/static/css/style.css";
// And add .my-class to style.css
```

**Action Item**: Before migration, audit inline styles in `templates.rs` and move to `style.css`:
- Filter section styles (`.filter-section`, `.filter-btn`)
- Playlist styles (`.playlist-section`, `.playlist-table`)
- Any page-specific styles

### 6.3 Raw HTML (PreEscaped)

Use `PreEscaped` sparingly and document why:

```rust
// Safe: Static string, no user input
(PreEscaped(STATIC_HTML_SNIPPET))

// Safe: Sanitized markdown output
(PreEscaped(&sanitized_html))

// DANGEROUS - never do this:
(PreEscaped(&user_input))  // XSS vulnerability!
```

### 6.4 Optional Attributes

Maud's conditional attribute syntax:

```rust
// Boolean attributes (present or absent)
button disabled[is_disabled] { "Submit" }

// Value attributes (Some or None)
a target=[open_new_tab.then_some("_blank")] { "Link" }

// Conditional class
div class=[is_active.then_some("active")] { }
```

### 6.5 Iteration

```rust
// Iterate over collections
div.archive-grid {
    @for archive in archives {
        (ArchiveCard::new(archive).render())
    }
}

// With index
@for (i, item) in items.iter().enumerate() {
    div data-index=(i) { (item.name) }
}
```

### 6.6 Composition

```rust
// Components can compose other components
impl ArchiveCard<'_> {
    fn render(&self) -> Markup {
        html! {
            article.archive-card data-nsfw=[self.is_nsfw.then_some("true")] {
                h3 {
                    a href=(format!("/archive/{}", self.id)) { (self.title) }
                    @if self.is_nsfw {
                        (NsfwBadge.render())
                    }
                }
                div.meta {
                    (DomainBadge::new(&self.domain).render())
                    (MediaTypeBadge::new(&self.content_type).render())
                    (StatusBadge::new(&self.status).render())
                }
            }
        }
    }
}
```

---

## Potential Issues & Mitigations

### 7.1 HTML Escaping Differences

**Issue**: Maud's escaping might differ slightly from manual `html_escape()`.

**Mitigation**:
- Compare snapshot outputs character-by-character
- Maud escapes: `<`, `>`, `&`, `"`, `'`
- Current `html_escape()` escapes same characters
- Should be equivalent, but verify in tests

### 7.2 Whitespace Differences

**Issue**: Maud produces minimal whitespace; current templates have formatted HTML.

**Mitigation**:
- This is fine - browsers ignore whitespace
- Snapshot tests should normalize whitespace before comparison
- Or accept that snapshots will change

### 7.3 Attribute Order

**Issue**: HTML attribute order may differ.

**Mitigation**:
- Attribute order doesn't affect rendering
- Don't rely on attribute order in tests
- Use DOM-based assertions instead of string matching

### 7.4 Archive Banner (Special Case)

**Issue**: `render_archive_banner` produces HTML injected into archived pages. It must work offline without our CSS.

**Mitigation**:
- Keep inline styles for banner (it's intentional for offline viewing)
- This function returns `String`, not `Markup`
- Can still use maud internally: `html!{...}.into_string()`

```rust
pub fn render_archive_banner(archive: &Archive, link: &Link) -> String {
    html! {
        // Inline styles intentional for offline viewing
        style { (PreEscaped(BANNER_INLINE_CSS)) }
        div.archive-banner style="..." {
            // banner content
        }
    }
    .into_string()
}
```

### 7.5 Large Template Functions

**Issue**: Some template functions are 200+ lines (e.g., `render_archive_detail`).

**Mitigation**:
- Break into smaller helper methods
- Extract repeated patterns into components
- Each section becomes a component or method

### 7.6 Compile Time Impact

**Issue**: Maud is a proc-macro; large templates can slow compilation.

**Mitigation**:
- Keep components small
- Avoid deeply nested `html!{}` blocks
- Use helper methods to break up large templates
- Monitor with `cargo build --timings`

---

## Parallel Development Structure

### 8.1 Component Ownership

For parallel development, assign component modules to different developers:

| Module | Owner | Dependencies |
|--------|-------|--------------|
| `layout.rs` | Dev 1 | None (do first) |
| `button.rs` | Dev 1 | None |
| `badge.rs` | Dev 2 | None |
| `alert.rs` | Dev 2 | None |
| `card.rs` | Dev 1 | badge.rs |
| `form.rs` | Dev 2 | button.rs |
| `table.rs` | Dev 2 | None |
| `pagination.rs` | Dev 1 | button.rs |
| `tabs.rs` | Dev 1 | badge.rs |
| `media.rs` | Dev 2 | None |

### 8.2 Page Ownership

After components are done:

| Page | Owner | Blocking Components |
|------|-------|---------------------|
| Home | Dev 1 | layout, card, pagination, tabs |
| Archive Detail | Dev 2 | layout, card, badge, media, table |
| Search | Dev 1 | layout, card, pagination, form |
| Admin | Dev 2 | layout, table, form, button |
| Login/Profile | Dev 2 | layout, form, alert |

### 8.3 Git Workflow

1. Create feature branch: `feat/maud-migration`
2. Each component in separate commit
3. Each page migration in separate commit
4. PR review before merging each phase

### 8.4 Module Export Strategy

Keep `mod.rs` exports stable for parallel work:

```rust
// src/components/mod.rs
mod layout;
mod button;
mod badge;
// ... more modules as added

pub use layout::BaseLayout;
pub use button::{Button, ButtonVariant};
pub use badge::{Badge, StatusBadge, DomainBadge, MediaTypeBadge, NsfwBadge};
// ... more exports as added
```

---

## Migration Checklist

### Pre-Migration

- [ ] Add `maud` dependency to Cargo.toml
- [ ] Remove `askama`, `askama_axum` from Cargo.toml
- [ ] Add `insta` dev dependency for snapshots
- [ ] Create `src/components/mod.rs`
- [ ] Move inline CSS from templates to `style.css`
- [ ] Extract JS to `static/js/` files
- [ ] Create snapshot tests for existing templates

### Phase 1: Components

- [ ] `layout.rs` - BaseLayout with header/footer
- [ ] `button.rs` - Button with variants
- [ ] `badge.rs` - All badge types
- [ ] `alert.rs` - Alert messages
- [ ] `card.rs` - ArchiveCard
- [ ] `form.rs` - Form elements
- [ ] `table.rs` - Table components
- [ ] `pagination.rs` - Pagination
- [ ] `tabs.rs` - Tab navigation
- [ ] `media.rs` - Media players
- [ ] Unit tests for all components

### Phase 2: Pages

- [ ] Home page
- [ ] Archive detail page
- [ ] Search page
- [ ] Stats page
- [ ] Threads list
- [ ] Thread detail
- [ ] Post detail
- [ ] Site list
- [ ] Submit form
- [ ] Login page
- [ ] Profile page
- [ ] Admin panel
- [ ] Debug queue
- [ ] Comparison view
- [ ] Archive banner (special)

### Post-Migration

- [ ] Delete old `templates.rs`
- [ ] Update all route handlers
- [ ] Run full test suite
- [ ] Visual regression testing
- [ ] Performance benchmarking
- [ ] Update documentation

---

## Appendix: Template Function Inventory

Current template functions in `src/web/templates.rs` to migrate:

| Function | Lines | Complexity | Notes |
|----------|-------|------------|-------|
| `base_layout` | ~180 | High | Foundation - do first |
| `base_layout_with_user` | ~10 | Low | Wrapper |
| `render_home_paginated` | ~50 | Medium | Uses multiple components |
| `render_recent_failed_archives_paginated` | ~40 | Medium | Similar to home |
| `render_recent_all_archives_paginated` | ~40 | Medium | Similar to home |
| `render_recent_archives_tabs` | ~30 | Low | Tab component |
| `render_content_type_filters` | ~50 | Medium | Filter buttons |
| `render_source_filters` | ~50 | Medium | Filter buttons |
| `render_pagination` | ~80 | Medium | Pagination component |
| `render_search` | ~40 | Medium | Search page |
| `render_archive_detail` | ~400 | High | Most complex page |
| `render_archive_card_display` | ~120 | High | Core component |
| `render_thread_card` | ~40 | Medium | Card variant |
| `render_site_list` | ~30 | Low | List page |
| `render_stats` | ~30 | Low | Stats page |
| `render_post_detail` | ~50 | Medium | Post page |
| `render_thread_detail` | ~80 | Medium | Thread page |
| `render_threads_list` | ~50 | Medium | List page |
| `render_submit_form` | ~60 | Medium | Form page |
| `render_submit_success` | ~20 | Low | Message |
| `render_submit_error` | ~20 | Low | Message |
| `render_media_player` | ~120 | High | Media component |
| `render_interactive_transcript` | ~80 | Medium | Transcript viewer |
| `render_archive_banner` | ~150 | Medium | Special - inline CSS |
| `render_comparison` | ~100 | Medium | Diff view |
| `render_debug_queue` | ~120 | Medium | Debug page |
| `render_playlist_content` | ~100 | Medium | Playlist display |
| `login_page` | ~90 | Medium | Auth page |
| `profile_page` | ~10 | Low | Wrapper |
| `profile_page_with_message` | ~90 | Medium | Profile page |
| `admin_panel` | ~220 | High | Admin page |
| `admin_password_reset_result` | ~40 | Low | Message |
| `admin_excluded_domains_page` | ~50 | Medium | Admin subpage |
| `render_excluded_domains_table` | ~70 | Medium | Table |
| `render_comment_edit_history` | ~50 | Medium | Comment history |
| `html_escape` | 6 | - | Delete after migration |

**Total: ~50 functions, ~3500 lines**
