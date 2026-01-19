# CLAUDE.md

Project guidance for AI assistants working on this codebase.

## Project Overview

Rust service that monitors a Discourse forum RSS feed, archives linked content (Reddit, TikTok, YouTube, etc.) to S3, and serves a public web UI for browsing archives.

## Build Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all tests
cargo test --lib         # Unit tests only
cargo test --test '*'    # Integration tests only
cargo clippy             # Lint check
cargo fmt --check        # Format check
cargo fmt                # Auto-format code
```

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
