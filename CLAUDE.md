# CLAUDE.md

Project guidance for AI assistants working on this codebase.

## Project Overview

Rust service that monitors a Discourse forum RSS feed, archives linked content (Reddit, TikTok, YouTube, etc.) to S3, and serves a public web UI for browsing archives.

## Build Commands

```bash
cargo build              # Debug build (use this during development)
cargo build --release    # Release build (very slow, only for production)
cargo test               # Run all tests
cargo test --lib         # Unit tests only
cargo test --test '*'    # Integration tests only
cargo clippy             # Lint check
cargo fmt --check        # Format check
cargo fmt                # Auto-format code
```

**IMPORTANT:** Always use `cargo build` (debug) during development. Release builds take significantly longer and are only needed for production deployment.

## New Session

When a new session is started, start `cargo build` and `cargo test` in background terminals so that everything gets compiled while you work on the users request.

## Before Committing

Always run `cargo fmt` before committing to ensure code is properly formatted (CI checks formatting):

```bash
cargo fmt && git add -A && git commit -m "your message"
```

## Task Tracking

**Keep `MAIN_TASKS.md` up to date:**
- Mark tasks complete when done
- Add new subtasks as discovered
- Remove old completed items periodically to keep file manageable
- Update status markers: `[ ]` pending, `[x]` complete, `[-]` blocked/skipped

## Code Style

### Rust Guidelines

- Use `thiserror` for library errors, `anyhow` for application errors
- Prefer `&str` over `String` in function parameters where possible
- Use `#[must_use]` on functions returning values that shouldn't be ignored
- Derive `Debug` on all public types
- Keep functions under 50 lines; extract helpers when longer
- Use early returns to reduce nesting

### Async Code

- All I/O operations must be async (tokio runtime)
- Use `tokio::spawn` for independent concurrent tasks
- Use `tokio::sync::Semaphore` for concurrency limits
- Prefer `tokio::select!` over manual future polling
- Always set timeouts on network operations

### Error Handling

- Propagate errors with `?` operator
- Add context with `.context()` or `.with_context()`
- Log errors at the point of handling, not propagation
- Use `tracing::error!` for errors, `tracing::warn!` for recoverable issues

### Dependencies

**Keep build times low:**
- Avoid heavy dependencies when lighter alternatives exist
- Prefer crates that compile quickly
- Use feature flags to disable unused functionality
- Periodically audit with `cargo build --timings`

**Recommended crates:**
- HTTP client: `reqwest` (with minimal features)
- Web framework: `axum`
- Database: `sqlx` (sqlite, runtime-tokio)
- Serialization: `serde`, `serde_json`
- HTML parsing: `scraper`
- RSS parsing: `feed-rs`
- Templates: `askama`

**Avoid:**
- Heavy proc-macro crates unless necessary
- Crates with many transitive dependencies
- Synchronous I/O libraries

### Naming Conventions

- Modules: `snake_case`
- Types/Traits: `PascalCase`
- Functions/Methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Handler modules: `handlers/{site}.rs`
- Test modules: inline `#[cfg(test)] mod tests` or `tests/` directory

### Web UI

- All HTML UI elements should use or extend the shadcn-inspired styles defined in [static/css/style.css](static/css/style.css).
- Keep the UI style consistent, minimal, and functional.

## Web UI Development

### Technology Stack

- **Templating:** `maud` - compile-time HTML templating with type safety
- **Routing:** `axum` - web framework for request handling
- **Styling:** Custom CSS in `static/css/style.css` (shadcn-inspired)
- **Components:** Rust modules in `src/components/`

### Page Structure

Pages are organized in `src/web/pages/`:
- `mod.rs` - Common page components (header, footer, layout)
- `home.rs` - Homepage with recent archives
- `archive.rs` - Individual archive detail pages
- `search.rs` - Search results page
- `site.rs` - Site-specific archive listings
- `threads.rs` - Thread-specific archive listings

Each page module typically contains:
- Render function (e.g., `render_archive_detail_page()`)
- Parameter structs for passing data to templates
- Unit tests for rendering logic

### Using Components

Components are reusable UI elements in `src/components/`:

**Pagination Component:**
```rust
use crate::components::Pagination;

let pagination = Pagination::new(current_page, total_pages, "/archives")
    .with_content_type_filter(Some("video"))
    .with_source_filter(Some("reddit.com"));

// In maud template:
html! {
    (pagination)  // Renders pagination controls
}
```

**Creating New Components:**
1. Add module to `src/components/mod.rs`
2. Create `src/components/your_component.rs`
3. Implement `maud::Render` trait for your component
4. Use the component in page templates with `(component_instance)`

### Styling Guidelines

**CSS Classes:**
- Use existing utility classes from `style.css`
- Button styles: `.btn`, `.btn-primary`, `.btn-secondary`
- Cards: `.card`, `.card-header`, `.card-content`
- Status badges: `.status-complete`, `.status-failed`, `.status-pending`
- Layout: `.container`, `.grid`, `.flex`

**Adding New Styles:**
- Add to `static/css/style.css`
- Follow existing naming conventions
- Use CSS custom properties for colors/spacing
- Keep mobile-responsive with media queries

**Button Component Pattern:**
All interactive elements should be proper buttons or links:
- Clickable actions: `<a href="..." class="btn">` for navigation
- Disabled states: `<button class="btn" disabled>` for non-interactive
- Active/selected: `<button class="btn active" disabled>` for current state

### Page Development Workflow

**Adding a New Page:**

1. **Create page module** in `src/web/pages/your_page.rs`:
```rust
use maud::{html, Markup};
use crate::components::Pagination;

pub struct YourPageParams<'a> {
    pub data: &'a [YourData],
    pub user: Option<&'a User>,
}

pub fn render_your_page(params: &YourPageParams) -> Markup {
    html! {
        (super::render_header("Page Title", params.user))
        main class="container" {
            h1 { "Your Page" }
            // Content here
        }
        (super::render_footer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_your_page() {
        // Test rendering
    }
}
```

2. **Add route handler** in `src/web/routes.rs`:
```rust
async fn your_page_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
) -> impl IntoResponse {
    // Fetch data
    let data = db::get_your_data(&state.db).await?;

    // Render
    let params = YourPageParams { data: &data, user: None };
    Html(render_your_page(&params).into_string())
}
```

3. **Register route** in `create_router()`:
```rust
.route("/your-page", get(your_page_handler))
```

4. **Add tests**:
- Unit tests in page module for rendering
- Integration tests in `tests/` for full request/response

### Common Patterns

**Pagination:**
```rust
let total_items = db::count_items().await?;
let total_pages = (total_items + per_page - 1) / per_page;
let pagination = Pagination::new(page, total_pages, "/list");
```

**User Authentication:**
```rust
// In route handler
let user = extract_user_from_session(&state, &headers).await;

// Pass to template
let params = PageParams { user: user.as_ref(), /* ... */ };
```

**Filter Preservation:**
```rust
// Preserve filters in pagination
let pagination = Pagination::new(page, total_pages, "/archives")
    .with_content_type_filter(filter.content_type.as_deref())
    .with_source_filter(filter.source.as_deref());
```

**Conditional Rendering:**
```rust
html! {
    @if items.is_empty() {
        p { "No items found" }
    } @else {
        @for item in items {
            (render_item(item))
        }
    }
}
```

## Testing Strategy

### Unit Tests

Location: Inline in source files under `#[cfg(test)] mod tests`

Test:
- URL normalization logic
- HTML parsing and link extraction
- Quote detection
- Domain matching
- Configuration parsing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_reddit_url() {
        // ...
    }
}
```

### Integration Tests

Location: `tests/` directory

Test:
- Database operations (use temp SQLite)
- Full archive pipeline with mock HTTP
- S3 operations (use localstack or minio)
- Web UI routes

```rust
// tests/archive_pipeline.rs
#[tokio::test]
async fn test_archive_reddit_post() {
    // ...
}
```

### Test Utilities

- Use `tempfile` for temporary directories/databases
- Use `wiremock` for HTTP mocking
- Use test fixtures in `tests/fixtures/`
- Prefer fast, isolated tests over slow integration tests

## Architecture Notes

### Module Structure

- `src/main.rs` - Entry point, CLI handling
- `src/config.rs` - Configuration loading
- `src/db/` - Database models and queries
- `src/rss/` - RSS polling and parsing
- `src/handlers/` - Per-site URL handlers
- `src/archiver/` - Download workers
- `src/s3/` - S3 client wrapper
- `src/web/` - axum routes and templates

### Key Patterns

**Handler Registry:**
Site handlers register URL patterns. The registry matches URLs and dispatches to appropriate handlers.

**Worker Pool:**
Semaphore-limited async tasks process pending archives. Workers acquire permits before spawning downloads.

**Quote Detection:**
DOM traversal marks links inside `<aside class="quote">`, `<blockquote>` as quoted. Quote-only links skip archiving unless first occurrence.

## Common Tasks

### Adding a New Site Handler

1. Create `src/handlers/{site}.rs`
2. Implement `SiteHandler` trait
3. Register in `src/handlers/registry.rs`
4. Add URL patterns and normalization rules
5. Write unit tests for URL matching
6. Write integration test for archive flow

### Adding a Database Migration

1. Create `src/db/migrations/{NNN}_{name}.sql`
2. Run migrations in order by filename
3. Update models in `src/db/models.rs`
4. Add/update queries in `src/db/queries.rs`

### Adding a Web Route

1. Add handler in `src/web/routes.rs`
2. Create template in `templates/`
3. Register route in router
4. Add integration test

## Performance Considerations

- SQLite: Use WAL mode, set `PRAGMA synchronous=NORMAL`
- Batch database inserts when processing multiple links
- Stream large files to S3 (don't load into memory)
- Use connection pooling for database and HTTP clients
- Set reasonable timeouts on all external calls

## Environment Variables

Required:
- `DATABASE_PATH` - SQLite file path
- `RSS_URL` - Discourse posts.rss URL
- `S3_BUCKET` - S3 bucket name
- `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`

Optional:
- `COOKIES_FILE_PATH` - Path to cookies.txt for authenticated archiving

See `SPEC.md` for complete list.

## External Tools

### Monolith
**Purpose:** Creates self-contained HTML archives (complete.html with all resources embedded)

**Configuration:**
- `MONOLITH_PATH` - Path to monolith binary (default: `monolith`)
- `MONOLITH_ENABLED` - Enable/disable monolith archiving
- `MONOLITH_TIMEOUT_SECS` - Execution timeout (default: 60s)
- `MONOLITH_INCLUDE_JS` - Include JavaScript in archive (default: false)

**Known Issues:**
- **Exit code 101 panics:** Older versions (v2.8.3) crash on certain HTML content with panic in unwrap(). Manifests as 6 archives losing complete.html format.
- **Fix:** Upgrade to latest monolith version from https://github.com/Y2Z/monolith/releases
- **Workaround (already implemented):** Monolith now processes raw.html (unmodified) instead of view.html (with injected banner), which prevents banner injection from breaking HTML parsing

**Installation:**
```bash
# Latest version
cargo install monolith

# Or from source
git clone https://github.com/Y2Z/monolith.git
cd monolith && cargo install --path .
```

**Monitoring:** Check logs for "Monolith crashed" warnings - these indicate either a broken monolith install or upstream tool bugs.
