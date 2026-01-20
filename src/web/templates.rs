use crate::db::{
    thread_key_from_url, Archive, ArchiveArtifact, ArchiveDisplay, ArchiveJob, Link,
    LinkOccurrenceWithPost, Post, QueueStats, ThreadDisplay,
};
use crate::web::diff::DiffResult;
use urlencoding::encode;

/// Base HTML layout.
fn base_layout(title: &str, content: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="light">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="color-scheme" content="light dark">
    <title>{title} - Discourse Link Archiver</title>
    <link rel="stylesheet" href="/static/css/style.css">
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>📦</text></svg>">
    <link rel="alternate" type="application/rss+xml" title="Archive RSS Feed" href="/feed.rss">
    <link rel="alternate" type="application/atom+xml" title="Archive Atom Feed" href="/feed.atom">
    <style>
        /* NSFW filtering styles */
        body.nsfw-hidden [data-nsfw="true"] {{ display: none !important; }}
    </style>
    <script>
        (function() {{
            // Theme preference
            var theme = localStorage.getItem('theme');
            if (theme) {{
                document.documentElement.setAttribute('data-theme', theme);
            }} else if (window.matchMedia('(prefers-color-scheme: dark)').matches) {{
                document.documentElement.setAttribute('data-theme', 'dark');
            }}
            // NSFW preference - default to hidden
            // Wait for DOM to be ready before accessing body
            if (document.body) {{
                var nsfwEnabled = localStorage.getItem('nsfw_enabled') === 'true';
                if (!nsfwEnabled) {{
                    document.body.classList.add('nsfw-hidden');
                }}
            }} else {{
                document.addEventListener('DOMContentLoaded', function() {{
                    var nsfwEnabled = localStorage.getItem('nsfw_enabled') === 'true';
                    if (!nsfwEnabled) {{
                        document.body.classList.add('nsfw-hidden');
                    }}
                }});
            }}
        }})();
    </script>
</head>
<body class="nsfw-hidden">
    <header class="container">
        <nav>
            <ul>
                <li><a href="/"><strong>Archive</strong></a></li>
            </ul>
            <ul>
                <li><a href="/">Home</a></li>
                <li><a href="/threads">Threads</a></li>
                <li><a href="/search">Search</a></li>
                <li><a href="/submit">Submit</a></li>
                <li><a href="/stats">Stats</a></li>
                <li><button id="nsfw-toggle" class="nsfw-toggle" title="Toggle NSFW content visibility" aria-label="Toggle NSFW content">18+</button></li>
                <li><button id="theme-toggle" class="theme-toggle" title="Toggle dark mode" aria-label="Toggle dark mode">🌓</button></li>
            </ul>
        </nav>
    </header>
    <main class="container">
        {content}
    </main>
    <footer class="container">
        <small>Discourse Link Archiver | <a href="/feed.rss">RSS</a> | <a href="/feed.atom">Atom</a></small>
    </footer>
    <script>
        (function() {{
            // Theme toggle
            var themeToggle = document.getElementById('theme-toggle');
            if (themeToggle) {{
                themeToggle.addEventListener('click', function() {{
                    var html = document.documentElement;
                    var current = html.getAttribute('data-theme');
                    var next = (current === 'dark') ? 'light' : 'dark';
                    html.setAttribute('data-theme', next);
                    localStorage.setItem('theme', next);
                }});
            }}

            // NSFW toggle
            var nsfwToggle = document.getElementById('nsfw-toggle');
            if (nsfwToggle) {{
                // Initialize button state
                var nsfwEnabled = localStorage.getItem('nsfw_enabled') === 'true';
                if (nsfwEnabled) {{
                    document.body.classList.remove('nsfw-hidden');
                    nsfwToggle.classList.add('active');
                }} else {{
                    document.body.classList.add('nsfw-hidden');
                    nsfwToggle.classList.remove('active');
                }}

                nsfwToggle.addEventListener('click', function() {{
                    var isEnabled = localStorage.getItem('nsfw_enabled') === 'true';
                    if (isEnabled) {{
                        // Disable NSFW content
                        localStorage.setItem('nsfw_enabled', 'false');
                        document.body.classList.add('nsfw-hidden');
                        nsfwToggle.classList.remove('active');
                    }} else {{
                        // Enable NSFW content
                        localStorage.setItem('nsfw_enabled', 'true');
                        document.body.classList.remove('nsfw-hidden');
                        nsfwToggle.classList.add('active');
                    }}
                }});
            }}
        }})();
    </script>
</body>
</html>"#
    )
}

/// Render home page with recent archives.
pub fn render_home(archives: &[ArchiveDisplay], recent_failed_count: usize) -> String {
    render_recent_archives_page(
        "Recent Archives",
        "Home",
        archives,
        RecentArchivesTab::Recent,
        recent_failed_count,
    )
}

/// Render home page with recent archives and pagination.
pub fn render_home_paginated(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
    page: usize,
    total_pages: usize,
) -> String {
    render_recent_archives_page_paginated(
        "Recent Archives",
        "Home",
        archives,
        RecentArchivesTab::Recent,
        recent_failed_count,
        page,
        total_pages,
        "/",
    )
}

pub fn render_recent_failed_archives(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
) -> String {
    render_recent_archives_page(
        "Recent Failed Archives",
        "Failed Archives",
        archives,
        RecentArchivesTab::Failed,
        recent_failed_count,
    )
}

pub fn render_recent_failed_archives_paginated(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
    page: usize,
    total_pages: usize,
) -> String {
    render_recent_archives_page_paginated(
        "Recent Failed Archives",
        "Failed Archives",
        archives,
        RecentArchivesTab::Failed,
        recent_failed_count,
        page,
        total_pages,
        "/archives/failed",
    )
}

pub fn render_recent_all_archives(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
) -> String {
    render_recent_archives_page(
        "All Recent Archives",
        "All Archives",
        archives,
        RecentArchivesTab::All,
        recent_failed_count,
    )
}

pub fn render_recent_all_archives_paginated(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
    page: usize,
    total_pages: usize,
) -> String {
    render_recent_archives_page_paginated(
        "All Recent Archives",
        "All Archives",
        archives,
        RecentArchivesTab::All,
        recent_failed_count,
        page,
        total_pages,
        "/archives/all",
    )
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum RecentArchivesTab {
    Recent,
    All,
    Failed,
}

fn render_recent_archives_tabs(active: RecentArchivesTab, recent_failed_count: usize) -> String {
    let recent_class = if active == RecentArchivesTab::Recent {
        "archive-tab active"
    } else {
        "archive-tab"
    };
    let all_class = if active == RecentArchivesTab::All {
        "archive-tab active"
    } else {
        "archive-tab"
    };
    let failed_class = if active == RecentArchivesTab::Failed {
        "archive-tab active"
    } else {
        "archive-tab"
    };

    let failed_count_badge = if recent_failed_count > 0 {
        format!(
            r#" <span class=\"archive-tab-count\" aria-label=\"{recent_failed_count} failed archives\">{recent_failed_count}</span>"#
        )
    } else {
        String::new()
    };

    format!(
        r#"<nav class="archive-tabs" aria-label="Archive list tabs">\
            <a class="{recent_class}" href="/">Recent</a>\
            <a class="{all_class}" href="/archives/all">All</a>\
            <a class="{failed_class}" href="/archives/failed">Failed{failed_count_badge}</a>\
        </nav>"#
    )
}

fn render_recent_archives_page(
    heading: &str,
    page_title: &str,
    archives: &[ArchiveDisplay],
    active_tab: RecentArchivesTab,
    recent_failed_count: usize,
) -> String {
    let mut content = String::new();
    content.push_str(&format!("<h1>{}</h1>", html_escape(heading)));
    content.push_str(&render_recent_archives_tabs(
        active_tab,
        recent_failed_count,
    ));

    if archives.is_empty() {
        content.push_str("<p>No archives yet.</p>");
    } else {
        content.push_str(r#"<div class="archive-grid">"#);
        for archive in archives {
            content.push_str(&render_archive_card_display(archive));
        }
        content.push_str("</div>");
    }

    base_layout(page_title, &content)
}

fn render_recent_archives_page_paginated(
    heading: &str,
    page_title: &str,
    archives: &[ArchiveDisplay],
    active_tab: RecentArchivesTab,
    recent_failed_count: usize,
    page: usize,
    total_pages: usize,
    base_url: &str,
) -> String {
    let mut content = String::new();
    content.push_str(&format!("<h1>{}</h1>", html_escape(heading)));
    content.push_str(&render_recent_archives_tabs(
        active_tab,
        recent_failed_count,
    ));

    if archives.is_empty() {
        content.push_str("<p>No archives yet.</p>");
    } else {
        content.push_str(r#"<div class="archive-grid">"#);
        for archive in archives {
            content.push_str(&render_archive_card_display(archive));
        }
        content.push_str("</div>");

        // Add pagination controls if needed
        if total_pages > 1 {
            content.push_str(&render_pagination(page, total_pages, base_url));
        }
    }

    base_layout(page_title, &content)
}

/// Render pagination controls.
fn render_pagination(current_page: usize, total_pages: usize, base_url: &str) -> String {
    let mut html = String::from(r#"<nav class="pagination">"#);

    // Previous button
    if current_page > 0 {
        let prev_url = if current_page == 1 {
            base_url.to_string()
        } else {
            format!("{}?page={}", base_url, current_page - 1)
        };
        html.push_str(&format!(r#"<a href="{prev_url}">&laquo; Previous</a>"#));
    } else {
        html.push_str(r#"<span class="disabled">&laquo; Previous</span>"#);
    }

    // Page numbers (show current, ±2, and first/last)
    let start = current_page.saturating_sub(2);
    let end = (current_page + 3).min(total_pages);

    if start > 0 {
        let url = if 0 == 0 {
            base_url
        } else {
            &format!("{base_url}?page=0")
        };
        html.push_str(&format!(r#"<a href="{url}">1</a>"#));
        if start > 1 {
            html.push_str("<span>...</span>");
        }
    }

    for page_num in start..end {
        let url = if page_num == 0 {
            base_url.to_string()
        } else {
            format!("{base_url}?page={page_num}")
        };

        if page_num == current_page {
            html.push_str(&format!(r#"<span class="current">{}</span>"#, page_num + 1));
        } else {
            html.push_str(&format!(r#"<a href="{}">{}</a>"#, url, page_num + 1));
        }
    }

    if end < total_pages {
        if end < total_pages - 1 {
            html.push_str("<span>...</span>");
        }
        let url = format!("{}?page={}", base_url, total_pages - 1);
        html.push_str(&format!(r#"<a href="{url}">{total_pages}</a>"#));
    }

    // Next button
    if current_page + 1 < total_pages {
        let next_url = format!("{}?page={}", base_url, current_page + 1);
        html.push_str(&format!(r#"<a href="{next_url}">Next &raquo;</a>"#));
    } else {
        html.push_str(r#"<span class="disabled">Next &raquo;</span>"#);
    }

    html.push_str("</nav>");
    html
}

/// Render search results page.
pub fn render_search(query: &str, archives: &[ArchiveDisplay], page: u32) -> String {
    let mut content = String::from("<h1>Search Archives</h1>");

    content.push_str(&format!(
        r#"<form method="get" action="/search">
            <input type="search" name="q" value="{}" placeholder="Search...">
            <button type="submit">Search</button>
        </form>"#,
        html_escape(query)
    ));

    if !query.is_empty() {
        content.push_str(&format!(
            "<p>Found {} results for \"{}\"</p>",
            archives.len(),
            html_escape(query)
        ));
    }

    content.push_str(r#"<div class="archive-grid">"#);
    for archive in archives {
        content.push_str(&render_archive_card_display(archive));
    }
    content.push_str("</div>");

    // Pagination
    if archives.len() >= 20 {
        content.push_str(&format!(
            r#"<nav>
                <a href="/search?q={}&page={}">Next page</a>
            </nav>"#,
            html_escape(query),
            page + 1
        ));
    }

    base_layout("Search", &content)
}

/// Render archive detail page.
pub fn render_archive_detail(
    archive: &Archive,
    link: &Link,
    artifacts: &[ArchiveArtifact],
    occurrences: &[LinkOccurrenceWithPost],
    jobs: &[ArchiveJob],
) -> String {
    let title = archive
        .content_title
        .as_deref()
        .unwrap_or("Untitled Archive");

    let nsfw_badge = if archive.is_nsfw {
        r#" <span class="nsfw-badge">NSFW</span>"#
    } else {
        ""
    };

    let mut content = String::new();

    // Show NSFW warning if applicable
    if archive.is_nsfw {
        content.push_str(r#"<div class="nsfw-warning" data-nsfw="true"><strong>Warning:</strong> This archive contains content marked as NSFW (Not Safe For Work).</div>"#);
        // Add a hidden message that shows when NSFW filter is active
        content.push_str(
            r#"<div class="nsfw-hidden-message" data-nsfw-hidden="true">
            <h2>NSFW Content Hidden</h2>
            <p>This archive contains NSFW content that is currently hidden.</p>
            <p>Click the <strong>18+</strong> button in the header to view this content.</p>
        </div>"#,
        );
    }

    // Format status with icon
    #[allow(clippy::useless_format)]
    let status_display = match archive.status.as_str() {
        "complete" => format!(r#"<span class="status-complete">✓ complete</span>"#),
        "failed" => format!(r#"<span class="status-failed">✗ failed</span>"#),
        "pending" => format!(r#"<span class="status-pending">⏳ pending</span>"#),
        "processing" => format!(r#"<span class="status-processing">⟳ processing</span>"#),
        "skipped" => format!(r#"<span class="status-skipped">⊘ skipped</span>"#),
        _ => format!(
            r#"<span class="status-{}">{}</span>"#,
            archive.status, archive.status
        ),
    };

    // Add data-nsfw attribute if this is NSFW content
    let nsfw_attr = if archive.is_nsfw {
        r#" data-nsfw="true""#
    } else {
        ""
    };

    // Format HTTP status code with color
    let http_status_display = archive
        .http_status_code
        .map(|code| {
            let (class, emoji) = match code {
                200..=299 => ("http-status-success", "✓"),
                301 | 302 | 303 | 307 | 308 => ("http-status-redirect", "↻"),
                400..=499 => ("http-status-client-error", "✗"),
                500..=599 => ("http-status-server-error", "⚠"),
                _ => ("http-status-unknown", "?"),
            };
            format!(
                r#"<br><strong>HTTP Status:</strong> <span class="{class}">{emoji} {code}</span>"#
            )
        })
        .unwrap_or_default();

    content.push_str(&format!(
        r#"<h1>{}{nsfw_badge}</h1>
        <article{nsfw_attr}>
            <header>
                <p class="meta">
                    <strong>Status:</strong> {}<br>
                    <strong>Original URL:</strong> <a href="{}" target="_blank" rel="noopener">{}</a><br>
                    <strong>Domain:</strong> {}{}<br>
                    <strong>Archived:</strong> {}
                </p>
            </header>"#,
        html_escape(title),
        status_display,
        html_escape(&link.normalized_url),
        html_escape(&link.normalized_url),
        html_escape(&link.domain),
        http_status_display,
        archive.archived_at.as_deref().unwrap_or("pending"),
        nsfw_attr = nsfw_attr
    ));

    // Show error details for failed/skipped archives
    if archive.status == "failed" || archive.status == "skipped" {
        content.push_str(r#"<section class="archive-error">"#);
        content.push_str("<h2>Archive Result</h2>");

        if let Some(ref error) = archive.error_message {
            content.push_str(&format!(
                r#"<p class="error-message"><strong>Error:</strong> <code>{}</code></p>"#,
                html_escape(error)
            ));
        }

        if let Some(ref last_attempt) = archive.last_attempt_at {
            content.push_str(&format!(
                r#"<p><strong>Last Attempt:</strong> {}</p>"#,
                html_escape(last_attempt)
            ));
        }

        content.push_str(&format!(
            r#"<p><strong>Retry Count:</strong> {}</p>"#,
            archive.retry_count
        ));

        content.push_str("</section>");
    }

    // Debug actions section
    content.push_str(&format!(
        r#"<section class="debug-actions">
            <h3>Debug Actions</h3>
            <div class="debug-buttons">
                <form method="post" action="/archive/{id}/rearchive" style="display: inline;">
                    <button type="submit" class="debug-button" title="Re-run the full archive pipeline including redirect handling">
                        🔄 Re-archive
                    </button>
                </form>
                <form method="post" action="/archive/{id}/toggle-nsfw" style="display: inline;">
                    <button type="submit" class="debug-button" title="Toggle NSFW status">
                        {nsfw_toggle_icon} Toggle NSFW
                    </button>
                </form>"#,
        id = archive.id,
        nsfw_toggle_icon = if archive.is_nsfw { "🔓" } else { "🔞" }
    ));

    // Add retry skipped button only for skipped archives
    if archive.status == "skipped" {
        content.push_str(&format!(
            r#"
                <form method="post" action="/archive/{}/retry-skipped" style="display: inline;">
                    <button type="submit" class="debug-button" title="Reset skipped archive for retry">
                        🔁 Retry Skipped
                    </button>
                </form>"#,
            archive.id
        ));
    }

    // Delete button with confirmation
    content.push_str(&format!(
        r#"
                <form method="post" action="/archive/{}/delete" style="display: inline;" onsubmit="return confirm('Are you sure you want to delete this archive? This cannot be undone.');">
                    <button type="submit" class="debug-button debug-button-danger" title="Delete archive and all artifacts">
                        🗑️ Delete
                    </button>
                </form>
            </div>
        </section>"#,
        archive.id
    ));

    if let Some(ref author) = archive.content_author {
        content.push_str(&format!(
            "<p><strong>Author:</strong> {}</p>",
            html_escape(author)
        ));
    }

    if let Some(ref text) = archive.content_text {
        let text_size = text.len();
        let size_display = if text_size >= 1024 {
            format!("{:.1} KB", text_size as f64 / 1024.0)
        } else {
            format!("{text_size} bytes")
        };
        content.push_str(&format!(
            r#"<section class="content-text-section"><details>
            <summary><h2 style="display: inline;">Plaintext Content</h2> <span class="text-size">({size_display})</span></summary>
            <pre class="content-text">{}</pre>
            </details></section>"#,
            html_escape(text)
        ));
    }

    let thumb_key = archive.s3_key_thumb.as_deref().or_else(|| {
        artifacts
            .iter()
            .find(|a| a.kind == "thumb")
            .map(|a| a.s3_key.as_str())
    });

    let primary_key_opt = archive.s3_key_primary.as_deref();
    let is_html_archive = primary_key_opt.is_some_and(|primary_key| {
        primary_key.ends_with(".html") || archive.content_type.as_deref() == Some("thread")
    });

    // Show Media section for non-HTML archives (videos, images, etc.)
    // For HTML archives, we show the embedded preview instead
    if let Some(primary_key) = primary_key_opt {
        if !is_html_archive {
            let download_name = suggested_download_filename(&link.domain, archive.id, primary_key);
            content.push_str("<section><h2>Media</h2>");
            content.push_str(&render_media_player(
                primary_key,
                archive.content_type.as_deref(),
                thumb_key,
            ));
            content.push_str(&format!(
                r#"<p><a href="/s3/{}" class="media-download" download="{}" target="_blank" rel="noopener">
                    <span>Download</span>
                </a></p>"#,
                html_escape(primary_key),
                html_escape(&download_name)
            ));
            content.push_str("</section>");
        }
    }

    // Fallback: if primary media wasn't rendered, show first video artifact inline
    if primary_key_opt.is_none() && !is_html_archive {
        if let Some(video_artifact) = artifacts.iter().find(|a| a.kind == "video") {
            let download_name =
                suggested_download_filename(&link.domain, archive.id, &video_artifact.s3_key);
            content.push_str("<section><h2>Media</h2>");
            content.push_str(&render_media_player(
                &video_artifact.s3_key,
                Some("video"),
                thumb_key,
            ));
            content.push_str(&format!(
                r#"<p><a href="/s3/{}" class="media-download" download="{}" target="_blank" rel="noopener">
                    <span>Download</span>
                </a></p>"#,
                html_escape(&video_artifact.s3_key),
                html_escape(&download_name)
            ));
            content.push_str("</section>");
        }
    }

    // Embedded HTML preview (collapsible) for webpage archives
    // Priority: complete.html (styled) > view.html (with banner) > raw.html
    if archive.s3_key_primary.is_some() && is_html_archive {
        // Find the best HTML artifact: complete.html (monolith) > view.html > raw.html
        let preview_key = artifacts
            .iter()
            .find(|a| a.s3_key.ends_with("complete.html"))
            .map(|a| &a.s3_key)
            .or_else(|| {
                artifacts
                    .iter()
                    .find(|a| a.s3_key.ends_with("view.html"))
                    .map(|a| &a.s3_key)
            })
            .or_else(|| {
                artifacts
                    .iter()
                    .find(|a| a.s3_key.ends_with("raw.html"))
                    .map(|a| &a.s3_key)
            });

        // Determine which version we're showing for the label
        let preview_type = if preview_key.is_some_and(|k| k.ends_with("complete.html")) {
            " (Full Archive)"
        } else if preview_key.is_some_and(|k| k.ends_with("view.html")) {
            " (With Banner)"
        } else {
            " (Original)"
        };

        if let Some(embed_key) = preview_key {
            content.push_str(&format!(
                r#"<section class="embedded-preview">
                    <details open>
                        <summary><h2 style="display: inline;">Archived Page Preview{}</h2></summary>
                        <div class="iframe-container">
                            <iframe src="/s3/{}" sandbox="allow-same-origin" loading="lazy" title="Archived webpage preview"></iframe>
                        </div>
                    </details>
                </section>"#,
                preview_type,
                html_escape(embed_key)
            ));
        }
    }

    // Screenshot preview section (if screenshot exists)
    let screenshot_artifact = artifacts.iter().find(|a| a.kind == "screenshot");
    let pdf_artifact = artifacts.iter().find(|a| a.kind == "pdf");
    let mhtml_artifact = artifacts.iter().find(|a| a.kind == "mhtml");

    // Show screenshot/captures section if any capture artifacts exist
    if screenshot_artifact.is_some() || pdf_artifact.is_some() || mhtml_artifact.is_some() {
        content.push_str(&format!(
            r#"<section class="captures-section"{nsfw_attr}><h2>Page Captures</h2>"#
        ));
        content.push_str(r#"<div class="captures-grid">"#);

        // Screenshot preview
        if let Some(screenshot) = screenshot_artifact {
            content.push_str(&format!(
                r#"<div class="capture-item">
                    <h4>Screenshot</h4>
                    <a href="/s3/{}" target="_blank" rel="noopener">
                        <img src="/s3/{}" alt="Page screenshot" class="screenshot-preview" loading="lazy">
                    </a>
                    <p class="capture-meta">{}</p>
                </div>"#,
                html_escape(&screenshot.s3_key),
                html_escape(&screenshot.s3_key),
                screenshot
                    .size_bytes
                    .map_or_else(|| "Unknown size".to_string(), format_bytes)
            ));
        }

        // PDF link
        if let Some(pdf) = pdf_artifact {
            content.push_str(&format!(
                r#"<div class="capture-item">
                    <h4>PDF Document</h4>
                    <a href="/s3/{}" target="_blank" rel="noopener" class="capture-link">
                        <span class="capture-icon">📄</span>
                        <span>View PDF</span>
                    </a>
                    <p class="capture-meta">{}</p>
                </div>"#,
                html_escape(&pdf.s3_key),
                pdf.size_bytes
                    .map_or_else(|| "Unknown size".to_string(), format_bytes)
            ));
        }

        // MHTML link
        if let Some(mhtml) = mhtml_artifact {
            let download_name =
                suggested_download_filename(&link.domain, archive.id, &mhtml.s3_key);
            content.push_str(&format!(
                r#"<div class="capture-item">
                    <h4>MHTML Archive</h4>
                    <a href="/s3/{}" download="{}" class="capture-link">
                        <span class="capture-icon">📦</span>
                        <span>Download MHTML</span>
                    </a>
                    <p class="capture-meta">{}</p>
                </div>"#,
                html_escape(&mhtml.s3_key),
                html_escape(&download_name),
                mhtml
                    .size_bytes
                    .map_or_else(|| "Unknown size".to_string(), format_bytes)
            ));
        }

        content.push_str("</div></section>");
    } else if archive.status == "complete" {
        // Archive completed but no captures - indicate this
        content.push_str(r#"<section class="captures-section captures-missing">
            <h2>Page Captures</h2>
            <p class="captures-note">No screenshot, PDF, or MHTML captures were generated for this archive.
            These captures may be disabled in the server configuration, or the page may have failed to render.</p>
        </section>"#);
    }

    // Archived Files section (list of all artifacts)
    if !artifacts.is_empty() {
        content.push_str("<section><h2>Archived Files</h2>");
        content.push_str(r#"<table class="artifacts-table"><thead><tr><th>Type</th><th>File</th><th>Size</th><th>Dedup</th><th>Actions</th></tr></thead><tbody>"#);

        for artifact in artifacts {
            let kind_display = artifact_kind_display(&artifact.kind);
            let filename = artifact
                .s3_key
                .rsplit('/')
                .next()
                .unwrap_or(&artifact.s3_key);
            let download_name =
                suggested_download_filename(&link.domain, archive.id, &artifact.s3_key);
            let size_display = artifact
                .size_bytes
                .map_or_else(|| "Unknown".to_string(), format_bytes);

            // Dedup info
            let dedup_info = if let Some(dup_id) = artifact.duplicate_of_artifact_id {
                format!(
                    r#"<span class="dedup-dup" title="Duplicate of artifact #{dup_id}">🔗 #{dup_id}</span>"#
                )
            } else if artifact.perceptual_hash.is_some() {
                format!(
                    r#"<span class="dedup-hash" title="Hash: {}">✓</span>"#,
                    html_escape(artifact.perceptual_hash.as_deref().unwrap_or(""))
                )
            } else {
                "—".to_string()
            };

            // Determine if this artifact is viewable in browser
            let is_viewable = is_viewable_in_browser(filename);
            let escaped_key = html_escape(&artifact.s3_key);
            let escaped_filename = html_escape(filename);
            let actions = if is_viewable {
                format!(
                    r#"<a href="/s3/{key}" target="_blank" title="View {name}" aria-label="View {name}" class="action-link">{view}</a> <a href="/s3/{key}" download="{dl}" title="Download {name}" aria-label="Download {name}" class="action-link">{download}</a>"#,
                    key = escaped_key,
                    name = escaped_filename,
                    dl = html_escape(&download_name),
                    view = view_icon(),
                    download = download_icon()
                )
            } else {
                format!(
                    r#"<a href="/s3/{key}" download="{dl}" title="Download {name}" aria-label="Download {name}" class="action-link">{download}</a>"#,
                    key = escaped_key,
                    name = escaped_filename,
                    dl = html_escape(&download_name),
                    download = download_icon()
                )
            };

            content.push_str(&format!(
                r#"<tr>
                    <td><span class="artifact-kind artifact-kind-{}">{}</span></td>
                    <td><code>{}</code></td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>"#,
                html_escape(&artifact.kind),
                html_escape(kind_display),
                html_escape(filename),
                html_escape(&size_display),
                dedup_info,
                actions
            ));
        }

        content.push_str("</tbody></table>");

        // Calculate total size
        let total_size: i64 = artifacts.iter().filter_map(|a| a.size_bytes).sum();
        if total_size > 0 {
            content.push_str(&format!(
                r#"<p class="total-size"><strong>Total Size:</strong> {}</p>"#,
                format_bytes(total_size)
            ));
        }

        content.push_str("</section>");
    }

    if let Some(ref wayback) = archive.wayback_url {
        content.push_str(&format!(
            r#"<section><h2>Wayback Machine</h2><p><a href="{}" target="_blank" rel="noopener">View on Wayback Machine</a></p></section>"#,
            html_escape(wayback)
        ));
    }

    if let Some(ref archive_today) = archive.archive_today_url {
        content.push_str(&format!(
            "<section><h2>Archive.today</h2><p><a href=\"{}\" target=\"_blank\" rel=\"noopener\">View on Archive.today</a></p></section>",
            html_escape(archive_today)
        ));
    }

    if let Some(ref ipfs_cid) = archive.ipfs_cid {
        content.push_str("<section><h2>IPFS</h2>");
        content.push_str(&format!(
            "<p><strong>CID:</strong> <code>{}</code></p>",
            html_escape(ipfs_cid)
        ));
        content.push_str("<p><strong>Public Gateways:</strong></p><ul>");
        content.push_str(&format!(
            "<li><a href=\"https://ipfs.io/ipfs/{cid}\" target=\"_blank\" rel=\"noopener\">ipfs.io</a></li>",
            cid = html_escape(ipfs_cid)
        ));
        content.push_str(&format!(
            "<li><a href=\"https://dweb.link/ipfs/{cid}\" target=\"_blank\" rel=\"noopener\">dweb.link</a></li>",
            cid = html_escape(ipfs_cid)
        ));
        content.push_str(&format!(
            "<li><a href=\"https://gateway.pinata.cloud/ipfs/{cid}\" target=\"_blank\" rel=\"noopener\">gateway.pinata.cloud</a></li>",
            cid = html_escape(ipfs_cid)
        ));
        content.push_str("</ul></section>");
    }

    content.push_str("</article>");

    // Link occurrences section
    if !occurrences.is_empty() {
        content.push_str(r#"<section class="link-occurrences"><h2>Link Occurrences</h2>"#);
        content.push_str(&format!(
            "<p>This link has been found in {} post(s):</p>",
            occurrences.len()
        ));
        content.push_str(r#"<table class="occurrences-table"><thead><tr><th>Post</th><th>Author</th><th>In Quote</th><th>Seen At</th></tr></thead><tbody>"#);

        for occ in occurrences {
            let post_title = occ.post_title.as_deref().unwrap_or("Untitled Post");
            let author = occ.post_author.as_deref().unwrap_or("Unknown");
            let in_quote = if occ.in_quote { "Yes" } else { "No" };

            content.push_str(&format!(
                r#"<tr>
                    <td><a href="/post/{}" title="View all archives from this post">{} →</a></td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>"#,
                html_escape(&occ.post_guid),
                html_escape(post_title),
                html_escape(author),
                in_quote,
                html_escape(&occ.seen_at)
            ));
        }

        content.push_str("</tbody></table></section>");
    }

    // Archive jobs section (collapsible, open if any job failed)
    if !jobs.is_empty() {
        let all_succeeded = jobs
            .iter()
            .all(|j| j.status == "completed" || j.status == "skipped");
        let open_attr = if all_succeeded { "" } else { " open" };

        content.push_str(&format!(
            r#"<section class="archive-jobs"><details{}>
            <summary><h2 style="display: inline;">Archive Jobs ({})</h2></summary>"#,
            open_attr,
            jobs.len()
        ));

        content.push_str(r#"<table class="jobs-table"><thead><tr><th>Job</th><th>Status</th><th>Started</th><th>Completed</th><th>Details</th></tr></thead><tbody>"#);

        for job in jobs {
            let job_type_display = match job.job_type.as_str() {
                "fetch_html" => "Fetch HTML",
                "yt_dlp" => "yt-dlp",
                "gallery_dl" => "gallery-dl",
                "screenshot" => "Screenshot",
                "pdf" => "PDF",
                "mhtml" => "MHTML",
                "monolith" => "Monolith",
                "s3_upload" => "S3 Upload",
                "wayback" => "Wayback Machine",
                "archive_today" => "Archive.today",
                "ipfs" => "IPFS",
                other => other,
            };

            let (status_class, status_icon) = match job.status.as_str() {
                "completed" => ("job-completed", "✓"),
                "failed" => ("job-failed", "✗"),
                "running" => ("job-running", "⟳"),
                "pending" => ("job-pending", "⏳"),
                "skipped" => ("job-skipped", "⊘"),
                _ => ("", ""),
            };

            let started = job.started_at.as_deref().unwrap_or("—");
            let completed = job.completed_at.as_deref().unwrap_or("—");

            let details = if let Some(ref error) = job.error_message {
                format!(
                    r#"<span class="job-error" title="{}">{}</span>"#,
                    html_escape(error),
                    html_escape(&error.chars().take(50).collect::<String>())
                )
            } else if let Some(ref meta) = job.metadata {
                format!(r#"<code class="job-meta">{}</code>"#, html_escape(meta))
            } else {
                "—".to_string()
            };

            content.push_str(&format!(
                r#"<tr>
                    <td>{}</td>
                    <td class="{}">{} {}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>"#,
                job_type_display,
                status_class,
                status_icon,
                html_escape(&job.status),
                html_escape(started),
                html_escape(completed),
                details
            ));
        }

        content.push_str("</tbody></table></details></section>");
    }

    // Collapsible debug info section
    content.push_str(r#"<section class="debug-info"><details><summary><h2 style="display: inline;">Debug Info</h2></summary>"#);
    content.push_str(r#"<table class="debug-table"><tbody>"#);

    // Archive internal fields
    content.push_str(&format!(
        r#"<tr><th>Archive ID</th><td>{}</td></tr>"#,
        archive.id
    ));
    content.push_str(&format!(
        r#"<tr><th>Link ID</th><td>{}</td></tr>"#,
        archive.link_id
    ));
    content.push_str(&format!(
        r#"<tr><th>Created At</th><td>{}</td></tr>"#,
        html_escape(&archive.created_at)
    ));
    content.push_str(&format!(
        r#"<tr><th>Status</th><td>{}</td></tr>"#,
        html_escape(&archive.status)
    ));
    content.push_str(&format!(
        r#"<tr><th>Retry Count</th><td>{}</td></tr>"#,
        archive.retry_count
    ));
    if let Some(ref next_retry) = archive.next_retry_at {
        content.push_str(&format!(
            r#"<tr><th>Next Retry At</th><td>{}</td></tr>"#,
            html_escape(next_retry)
        ));
    }
    if let Some(ref last_attempt) = archive.last_attempt_at {
        content.push_str(&format!(
            r#"<tr><th>Last Attempt At</th><td>{}</td></tr>"#,
            html_escape(last_attempt)
        ));
    }
    if let Some(ref error) = archive.error_message {
        content.push_str(&format!(
            r#"<tr><th>Error Message</th><td><code>{}</code></td></tr>"#,
            html_escape(error)
        ));
    }
    content.push_str(&format!(
        r#"<tr><th>Is NSFW</th><td>{}</td></tr>"#,
        archive.is_nsfw
    ));
    if let Some(ref nsfw_source) = archive.nsfw_source {
        content.push_str(&format!(
            r#"<tr><th>NSFW Source</th><td>{}</td></tr>"#,
            html_escape(nsfw_source)
        ));
    }
    if let Some(ref content_type) = archive.content_type {
        content.push_str(&format!(
            r#"<tr><th>Content Type</th><td>{}</td></tr>"#,
            html_escape(content_type)
        ));
    }

    // Link internal fields
    content.push_str(
        r#"<tr><th colspan="2" style="background: var(--border-color);">Link Info</th></tr>"#,
    );
    content.push_str(&format!(
        r#"<tr><th>Original URL</th><td><code>{}</code></td></tr>"#,
        html_escape(&link.original_url)
    ));
    content.push_str(&format!(
        r#"<tr><th>Normalized URL</th><td><code>{}</code></td></tr>"#,
        html_escape(&link.normalized_url)
    ));
    if let Some(ref canonical) = link.canonical_url {
        content.push_str(&format!(
            r#"<tr><th>Canonical URL</th><td><code>{}</code></td></tr>"#,
            html_escape(canonical)
        ));
    }
    if let Some(ref final_url) = link.final_url {
        content.push_str(&format!(
            r#"<tr><th>Final URL (after redirects)</th><td><code>{}</code></td></tr>"#,
            html_escape(final_url)
        ));
    }
    content.push_str(&format!(
        r#"<tr><th>Domain</th><td>{}</td></tr>"#,
        html_escape(&link.domain)
    ));
    if let Some(ref last_archived) = link.last_archived_at {
        content.push_str(&format!(
            r#"<tr><th>Last Archived At</th><td>{}</td></tr>"#,
            html_escape(last_archived)
        ));
    }

    content.push_str("</tbody></table></details></section>");

    // Comparison form
    content.push_str(&format!(
        r#"<section class="comparison-form">
            <h2>Compare with Another Archive</h2>
            <form method="get" action="/compare/{}/{{}}" onsubmit="this.action = '/compare/{}/'+this.archiveId.value; return true;">
                <input type="number" name="archiveId" placeholder="Archive ID" required min="1" style="width: 150px;">
                <button type="submit">Compare</button>
            </form>
            <p><small>Enter an archive ID to compare content differences.</small></p>
        </section>"#,
        archive.id, archive.id
    ));

    base_layout(title, &content)
}

/// Convert artifact kind to human-readable display string.
fn artifact_kind_display(kind: &str) -> &'static str {
    match kind {
        "raw_html" => "HTML (Original)",
        "view_html" => "HTML (With Banner)",
        "complete_html" => "HTML (Full Archive)",
        "mhtml" => "MHTML Archive",
        "screenshot" => "Screenshot",
        "pdf" => "PDF",
        "video" => "Video",
        "thumb" => "Thumbnail",
        "metadata" => "Metadata",
        "image" => "Image",
        "subtitles" => "Subtitles",
        _ => "File",
    }
}

/// Return a view icon SVG.
fn view_icon() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7-11-7-11-7"/><circle cx="12" cy="12" r="3"/></svg>"#
}

/// Return a download icon SVG.
fn download_icon() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>"#
}

/// Check if a file can be viewed directly in a browser.
fn is_viewable_in_browser(filename: &str) -> bool {
    let filename_lower = filename.to_lowercase();
    // HTML files
    filename_lower.ends_with(".html")
        || filename_lower.ends_with(".htm")
        || filename_lower.ends_with(".mhtml")
        // PDF files
        || filename_lower.ends_with(".pdf")
        // Image files
        || filename_lower.ends_with(".jpg")
        || filename_lower.ends_with(".jpeg")
        || filename_lower.ends_with(".png")
        || filename_lower.ends_with(".gif")
        || filename_lower.ends_with(".webp")
        || filename_lower.ends_with(".svg")
        || filename_lower.ends_with(".bmp")
        // Video files (modern browsers support these)
        || filename_lower.ends_with(".mp4")
        || filename_lower.ends_with(".webm")
        // Audio files
        || filename_lower.ends_with(".mp3")
        || filename_lower.ends_with(".ogg")
        || filename_lower.ends_with(".wav")
        // Text files
        || filename_lower.ends_with(".txt")
        || filename_lower.ends_with(".json")
        || filename_lower.ends_with(".xml")
}

fn suggested_download_filename(domain: &str, archive_id: i64, s3_key: &str) -> String {
    let filename = s3_key.rsplit('/').next().unwrap_or(s3_key);
    let domain = sanitize_filename_component(&domain.to_lowercase());
    let filename = sanitize_filename_component(filename);

    if filename.is_empty() {
        format!("{domain}_archive_{archive_id}")
    } else {
        format!("{domain}_archive_{archive_id}_{filename}")
    }
}

fn sanitize_filename_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

/// Render site list page.
pub fn render_site_list(site: &str, archives: &[ArchiveDisplay], page: u32) -> String {
    let mut content = format!("<h1>Archives from {}</h1>", html_escape(site));

    if archives.is_empty() {
        content.push_str("<p>No archives from this site.</p>");
    } else {
        content.push_str(r#"<div class="archive-grid">"#);
        for archive in archives {
            content.push_str(&render_archive_card_display(archive));
        }
        content.push_str("</div>");

        if archives.len() >= 20 {
            content.push_str(&format!(
                r#"<nav>
                    <a href="/site/{}?page={}">Next page</a>
                </nav>"#,
                html_escape(site),
                page + 1
            ));
        }
    }

    base_layout(&format!("Archives from {site}"), &content)
}

/// Render stats page.
pub fn render_stats(status_counts: &[(String, i64)], link_count: i64, post_count: i64) -> String {
    let mut content = String::from("<h1>Statistics</h1>");

    content.push_str("<section><h2>Overview</h2>");
    content.push_str(&format!(
        "<p><strong>Total Posts:</strong> {post_count}</p>"
    ));
    content.push_str(&format!(
        "<p><strong>Total Links:</strong> {link_count}</p>"
    ));
    content.push_str("</section>");

    content.push_str("<section><h2>Archives by Status</h2><table><thead><tr><th>Status</th><th>Count</th></tr></thead><tbody>");

    for (status, count) in status_counts {
        content.push_str(&format!(
            "<tr><td class=\"status-{status}\">{status}</td><td>{count}</td></tr>"
        ));
    }

    content.push_str("</tbody></table></section>");

    base_layout("Statistics", &content)
}

/// Render post detail page showing all archives from a post.
pub fn render_post_detail(post: &Post, archives: &[ArchiveDisplay]) -> String {
    let title = post.title.as_deref().unwrap_or("Untitled Post");

    let mut content = format!(
        r#"<h1>{}</h1>
        <article>
            <header>
                <p class="meta">
                    <strong>Author:</strong> {}<br>
                    <strong>Published:</strong> {}<br>
                    <strong>Source:</strong> <a href="{}" target="_blank" rel="noopener">{}</a>
                </p>
            </header>"#,
        html_escape(title),
        html_escape(post.author.as_deref().unwrap_or("Unknown")),
        post.published_at.as_deref().unwrap_or("Unknown"),
        html_escape(&post.discourse_url),
        html_escape(&post.discourse_url)
    );

    content.push_str("<section><h2>Archived Links</h2>");

    if archives.is_empty() {
        content.push_str("<p>No archives from this post.</p>");
    } else {
        content.push_str(&format!(
            "<p>Found {} archived link(s) from this post.</p>",
            archives.len()
        ));
        content.push_str(r#"<div class="archive-grid">"#);
        for archive in archives {
            content.push_str(&render_archive_card_display(archive));
        }
        content.push_str("</div>");
    }

    content.push_str("</section></article>");

    base_layout(&format!("Post: {title}"), &content)
}

/// Render a thread detail page showing archives across all posts in the thread.
pub fn render_thread_detail(
    thread_key: &str,
    posts: &[Post],
    archives: &[ArchiveDisplay],
) -> String {
    let title = posts
        .iter()
        .find_map(|p| p.title.clone())
        .unwrap_or_else(|| "Untitled Thread".to_string());

    let author = posts
        .iter()
        .find_map(|p| p.author.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let published = posts
        .iter()
        .filter_map(|p| p.published_at.clone())
        .min()
        .unwrap_or_else(|| "Unknown".to_string());

    let discourse_url = posts.first().map_or("", |p| p.discourse_url.as_str());

    let last_activity = archives
        .iter()
        .filter_map(|a| a.archived_at.clone())
        .max()
        .unwrap_or_else(|| "No archives yet".to_string());

    let mut content = format!(
        r#"<h1>{}</h1>
        <article>
            <header>
                <p class="meta">
                    <strong>Author:</strong> {}<br>
                    <strong>Published:</strong> {}<br>
                    <strong>Posts in thread:</strong> {}<br>
                    <strong>Archives found:</strong> {}<br>
                    <strong>Last activity:</strong> {}<br>
                    <strong>Source:</strong> <a href="{}" target="_blank" rel="noopener">{}</a>
                </p>
                <p><small>Thread key: {}</small></p>
            </header>"#,
        html_escape(&title),
        html_escape(&author),
        published,
        posts.len(),
        archives.len(),
        last_activity,
        html_escape(discourse_url),
        html_escape(discourse_url),
        html_escape(thread_key)
    );

    // Optional quick list of posts in the thread for navigation context.
    content.push_str("<section><h2>Posts</h2><ul>");
    for post in posts {
        let post_title = post.title.as_deref().unwrap_or("Untitled Post");
        content.push_str(&format!(
            r#"<li><a href="/post/{guid}">{title}</a> — {published}</li>"#,
            guid = html_escape(&post.guid),
            title = html_escape(post_title),
            published = post.published_at.as_deref().unwrap_or("Unknown"),
        ));
    }
    content.push_str("</ul></section>");

    content.push_str("<section><h2>Archived Links</h2>");

    if archives.is_empty() {
        content.push_str("<p>No archives from this thread.</p>");
    } else {
        content.push_str(&format!(
            "<p>Found {} archived link(s) across the thread.</p>",
            archives.len()
        ));
        content.push_str(r#"<div class="archive-grid">"#);
        for archive in archives {
            content.push_str(&render_archive_card_display(archive));
        }
        content.push_str("</div>");
    }

    content.push_str("</section></article>");

    base_layout(&format!("Thread: {title}"), &content)
}

/// Render threads list page with sorting and pagination.
pub fn render_threads_list(threads: &[ThreadDisplay], sort_by: &str, page: u32) -> String {
    let mut content = String::from("<h1>Discourse Threads</h1>");

    // Sort navigation
    content.push_str(r#"<nav class="sort-nav" style="margin-bottom: 1.5rem;">"#);
    content.push_str("<span>Sort by: </span>");

    let sort_links = [
        ("created", "Most Recent"),
        ("updated", "Recently Updated"),
        ("name", "Name"),
    ];

    for (sort_key, label) in sort_links {
        if sort_key == sort_by {
            content.push_str(&format!(r#"<strong>{label}</strong> "#));
        } else {
            content.push_str(&format!(
                r#"<a href="/threads?sort={sort_key}">{label}</a> "#
            ));
        }
    }
    content.push_str("</nav>");

    if threads.is_empty() {
        content.push_str("<p>No threads found.</p>");
    } else {
        content.push_str(r#"<div class="archive-grid">"#);
        for thread in threads {
            content.push_str(&render_thread_card(thread));
        }
        content.push_str("</div>");

        // Pagination
        if threads.len() >= 20 {
            content.push_str(&format!(
                r#"<nav style="margin-top: 1.5rem;">
                    <a href="/threads?sort={sort_by}&page={}">Next page</a>
                </nav>"#,
                page + 1
            ));
        }
    }

    base_layout("Threads", &content)
}

/// Render a single thread card for the threads list.
fn render_thread_card(thread: &ThreadDisplay) -> String {
    let title = thread.title.as_deref().unwrap_or("Untitled Thread");
    let author = thread.author.as_deref().unwrap_or("Unknown");
    let published = thread.published_at.as_deref().unwrap_or("Unknown");
    let last_activity = thread
        .last_archived_at
        .as_deref()
        .unwrap_or("No archives yet");

    let thread_key = thread_key_from_url(&thread.discourse_url);
    let thread_key_encoded = encode(&thread_key);

    format!(
        r#"<article class="archive-card">
            <header>
                <h3><a href="/thread/{thread_key}">{title}</a></h3>
            </header>
            <div>
                <p><strong>Author:</strong> {author}</p>
                <p><strong>Published:</strong> {published}</p>
                <p><strong>Links:</strong> {link_count}</p>
                <p><strong>Archives:</strong> {archive_count}</p>
                <p><strong>Last Activity:</strong> {last_activity}</p>
                <p><a href="{discourse_url}" target="_blank" rel="noopener">View on Discourse →</a></p>
            </div>
        </article>"#,
        thread_key = thread_key_encoded,
        title = html_escape(title),
        author = html_escape(author),
        published = published,
        link_count = thread.link_count,
        archive_count = thread.archive_count,
        last_activity = last_activity,
        discourse_url = html_escape(&thread.discourse_url)
    )
}

/// Render an archive card with link info (for display in lists).
fn render_archive_card_display(archive: &ArchiveDisplay) -> String {
    let title = archive.content_title.as_deref().unwrap_or("Untitled");
    let content_type = archive.content_type.as_deref().unwrap_or("unknown");

    // NSFW data attribute for filtering
    let nsfw_attr = if archive.is_nsfw {
        r#" data-nsfw="true""#
    } else {
        ""
    };

    // NSFW badge
    let nsfw_badge = if archive.is_nsfw {
        r#"<span class="nsfw-badge">NSFW</span>"#
    } else {
        ""
    };

    // Content type badge with appropriate styling
    let type_badge = match content_type {
        "video" => r#"<span class="media-type-badge media-type-video">Video</span>"#.to_string(),
        "audio" => r#"<span class="media-type-badge media-type-audio">Audio</span>"#.to_string(),
        "image" | "gallery" => {
            r#"<span class="media-type-badge media-type-image">Image</span>"#.to_string()
        }
        "text" | "thread" => {
            r#"<span class="media-type-badge media-type-text">Text</span>"#.to_string()
        }
        _ => format!(
            r#"<span class="media-type-badge">{}</span>"#,
            html_escape(content_type)
        ),
    };

    // Format the archived timestamp for display
    let archived_time = archive.archived_at.as_deref().unwrap_or("pending");

    // Author line (if present)
    let author_line = archive
        .content_author
        .as_deref()
        .map(|author| format!(r#"<span class="author">by {}</span>"#, html_escape(author)))
        .unwrap_or_default();

    // Status display with success/failure indicators
    let status_display = match archive.status.as_str() {
        "complete" => format!(
            r#"<span class="status-{status}" title="Archive completed successfully">✓ {status}</span>"#,
            status = archive.status
        ),
        "failed" => {
            let error_hint = archive
                .error_message
                .as_deref()
                .map(|e| format!(r#" title="{}""#, html_escape(e)))
                .unwrap_or_default();
            format!(
                r#"<span class="status-{status}"{error_hint}>✗ {status}</span>"#,
                status = archive.status
            )
        }
        "pending" => format!(
            r#"<span class="status-{status}" title="Archive pending">⏳ {status}</span>"#,
            status = archive.status
        ),
        "processing" => format!(
            r#"<span class="status-{status}" title="Archive in progress">⟳ {status}</span>"#,
            status = archive.status
        ),
        "skipped" => format!(
            r#"<span class="status-{status}" title="Archive skipped">⊘ {status}</span>"#,
            status = archive.status
        ),
        _ => format!(
            r#"<span class="status-{status}">{status}</span>"#,
            status = archive.status
        ),
    };

    // Format size display
    let size_display = archive
        .total_size_bytes
        .filter(|&size| size > 0)
        .map(|size| {
            format!(
                r#"<span class="archive-size">{}</span>"#,
                format_bytes(size)
            )
        })
        .unwrap_or_default();

    format!(
        r#"<article class="archive-card"{nsfw_attr}>
            <h3><a href="/archive/{id}">{title}</a>{nsfw_badge}</h3>
            <p class="archive-url"><code class="url-display" title="Click to copy" onclick="navigator.clipboard.writeText('{url}'); this.title='Copied!'; setTimeout(() => this.title='Click to copy', 2000);">{url}</code></p>
            <p class="meta">
                {status_display}
                {type_badge}
                <a href="/site/{domain}" class="domain-badge">{domain}</a>
                {author_line}
                {size_display}
            </p>
            <p class="archive-time">{archived_time}</p>
        </article>"#,
        id = archive.id,
        title = html_escape(title),
        nsfw_badge = nsfw_badge,
        url = html_escape(&archive.original_url),
        status_display = status_display,
        type_badge = type_badge,
        domain = html_escape(&archive.domain),
        author_line = author_line,
        size_display = size_display,
        archived_time = archived_time
    )
}

/// Format bytes into human-readable format (e.g., "1.5 MB").
fn format_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

/// Render submission form page.
pub fn render_submit_form(error: Option<&str>, success: Option<&str>) -> String {
    let mut content = String::from("<h1>Submit URL for Archiving</h1>");

    content.push_str(r"
        <article>
            <p>Submit a URL to be archived. Supported sites include Reddit, Twitter/X, TikTok, YouTube, Instagram, Imgur, and more.</p>
            <p><strong>Rate limit:</strong> 60 submissions per hour per IP address.</p>
        </article>
    ");

    if let Some(err) = error {
        content.push_str(&format!(
            r#"<article class="error"><p><strong>Error:</strong> {}</p></article>"#,
            html_escape(err)
        ));
    }

    if let Some(msg) = success {
        content.push_str(&format!(
            r#"<article class="success"><p><strong>Success:</strong> {}</p></article>"#,
            html_escape(msg)
        ));
    }

    content.push_str(
        r#"
        <form method="post" action="/submit">
            <label for="url">URL to Archive</label>
            <input type="url" id="url" name="url" required
                   placeholder="https://reddit.com/r/..."
                   pattern="https?://.*">
            <small>Enter the full URL including https://</small>
            <button type="submit">Submit for Archiving</button>
        </form>
    "#,
    );

    base_layout("Submit URL", &content)
}

/// Render submission success page.
pub fn render_submit_success(submission_id: i64) -> String {
    let content = format!(
        r#"
        <h1>URL Submitted Successfully</h1>
        <article class="success">
            <p>Your URL has been queued for archiving.</p>
            <p><strong>Submission ID:</strong> {submission_id}</p>
            <p>The archive will be processed shortly. Check back later for results.</p>
        </article>
        <p><a href="/submit">Submit another URL</a></p>
        "#
    );

    base_layout("Submission Successful", &content)
}

/// Render submission error page.
pub fn render_submit_error(error: &str) -> String {
    let content = format!(
        r#"
        <h1>Submission Failed</h1>
        <article class="error">
            <p><strong>Error:</strong> {}</p>
        </article>
        <p><a href="/submit">Try again</a></p>
        "#,
        html_escape(error)
    );

    base_layout("Submission Failed", &content)
}

/// Render a media player based on content type and file extension.
fn render_media_player(
    s3_key: &str,
    content_type: Option<&str>,
    thumbnail: Option<&str>,
) -> String {
    let extension = s3_key.rsplit('.').next().unwrap_or("").to_lowercase();

    // Determine content type badge
    let type_badge = match content_type {
        Some("video") => r#"<span class="media-type-badge media-type-video">Video</span>"#,
        Some("audio") => r#"<span class="media-type-badge media-type-audio">Audio</span>"#,
        Some("image" | "gallery") => {
            r#"<span class="media-type-badge media-type-image">Image</span>"#
        }
        _ => r#"<span class="media-type-badge media-type-text">File</span>"#,
    };

    let media_url = format!("/s3/{}", html_escape(s3_key));

    // Video formats
    if matches!(
        extension.as_str(),
        "mp4" | "webm" | "mkv" | "mov" | "avi" | "m4v"
    ) || content_type == Some("video")
    {
        let poster = thumbnail
            .map(|t| format!(r#" poster="/s3/{}""#, html_escape(t)))
            .unwrap_or_default();

        return format!(
            r#"<div class="media-container">
                {type_badge}
                <div class="video-wrapper">
                    <video controls preload="metadata"{poster}>
                        <source src="{media_url}" type="video/{ext}">
                        Your browser does not support the video tag.
                    </video>
                </div>
            </div>"#,
            type_badge = type_badge,
            media_url = media_url,
            ext = if extension == "mkv" {
                "x-matroska"
            } else {
                &extension
            },
            poster = poster
        );
    }

    // Audio formats
    if matches!(
        extension.as_str(),
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac"
    ) || content_type == Some("audio")
    {
        return format!(
            r#"<div class="media-container">
                {type_badge}
                <div class="audio-wrapper">
                    <audio controls preload="metadata">
                        <source src="{media_url}" type="audio/{ext}">
                        Your browser does not support the audio element.
                    </audio>
                </div>
            </div>"#,
            type_badge = type_badge,
            media_url = media_url,
            ext = if extension == "m4a" {
                "mp4"
            } else {
                &extension
            }
        );
    }

    // Image formats
    if matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp"
    ) || content_type == Some("image")
        || content_type == Some("gallery")
    {
        return format!(
            r#"<div class="media-container">
                {type_badge}
                <img src="{media_url}" alt="Archived media" loading="lazy">
            </div>"#
        );
    }

    // Default: show thumbnail if available, otherwise just a download link
    if let Some(thumb) = thumbnail {
        format!(
            r#"<div class="media-container">
                {type_badge}
                <img src="/s3/{thumb}" alt="Thumbnail" class="archive-thumb">
                <p><small>Preview image shown. Full media available for download.</small></p>
            </div>"#,
            type_badge = type_badge,
            thumb = html_escape(thumb)
        )
    } else {
        format!(
            r#"<div class="media-container">
                {type_badge}
                <div class="media-error">
                    <p>Media preview not available</p>
                    <p><small>File: {}</small></p>
                </div>
            </div>"#,
            html_escape(s3_key)
        )
    }
}

/// Escape HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Render archive banner for HTML files (archive.today style).
/// Includes inline CSS for offline viewing.
pub fn render_archive_banner(archive: &Archive, link: &Link) -> String {
    // Inline CSS for the archive banner to ensure it renders correctly offline
    let inline_css = r#"<style>
.archive-banner {
    border: 2px solid #4a90e2;
    border-radius: 4px;
    margin: 0 0 1rem 0;
    padding: 0;
    background-color: #f8f9fa;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    font-size: 14px;
    line-height: 1.5;
    position: relative;
    z-index: 9999;
}
.archive-banner details { border: none; margin: 0; padding: 0; }
.archive-banner summary {
    padding: 10px 15px;
    cursor: pointer;
    list-style: none;
    display: flex;
    align-items: center;
    gap: 10px;
    user-select: none;
    background-color: #e8f0f8;
    border-bottom: 1px solid #4a90e2;
}
.archive-banner summary::-webkit-details-marker { display: none; }
.archive-banner summary::marker { display: none; }
.archive-banner .banner-icon { font-size: 16px; }
.archive-banner .banner-text { flex: 1; font-weight: 500; color: #333; }
.archive-banner .banner-toggle { font-size: 12px; color: #666; transition: transform 0.2s; }
.archive-banner details[open] .banner-toggle { transform: rotate(180deg); }
.archive-banner .banner-content { padding: 15px; background-color: #ffffff; }
.archive-banner .banner-row {
    margin-bottom: 10px;
    padding-bottom: 10px;
    border-bottom: 1px solid #e0e0e0;
    color: #333;
}
.archive-banner .banner-row:last-child { margin-bottom: 0; padding-bottom: 0; border-bottom: none; }
.archive-banner .banner-row strong { color: #333; margin-right: 8px; }
.archive-banner .banner-row a { color: #4a90e2; text-decoration: none; }
.archive-banner .banner-row a:hover { text-decoration: underline; }
.archive-banner .banner-links {
    margin-top: 15px;
    padding-top: 15px;
    border-top: 1px solid #e0e0e0;
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
}
.archive-banner .banner-link {
    display: inline-block;
    padding: 6px 12px;
    background-color: #4a90e2;
    color: white;
    border-radius: 3px;
    text-decoration: none;
    font-size: 13px;
    transition: background-color 0.2s;
}
.archive-banner .banner-link:hover { background-color: #357abd; text-decoration: none; }
@media (prefers-color-scheme: dark) {
    .archive-banner { background-color: #1a1a1a; border-color: #5a9ff0; }
    .archive-banner summary { background-color: #2a2a2a; border-bottom-color: #5a9ff0; }
    .archive-banner .banner-text { color: #e0e0e0; }
    .archive-banner .banner-toggle { color: #999; }
    .archive-banner .banner-content { background-color: #1a1a1a; }
    .archive-banner .banner-row { border-bottom-color: #333; color: #e0e0e0; }
    .archive-banner .banner-row strong { color: #e0e0e0; }
    .archive-banner .banner-links { border-top-color: #333; }
}
</style>"#;

    let mut banner = format!(
        r#"{inline_css}<div id="archive-banner" class="archive-banner">
            <details>
                <summary>
                    <span class="banner-icon">📦</span>
                    <span class="banner-text">This is an archived page</span>
                    <span class="banner-toggle">▼</span>
                </summary>
                <div class="banner-content">"#,
    );

    banner.push_str(&format!(
        r#"<div class="banner-row">
                <strong>Archived:</strong> {}
            </div>"#,
        html_escape(archive.archived_at.as_deref().unwrap_or("pending"))
    ));

    banner.push_str(&format!(
        r#"<div class="banner-row">
                <strong>Original URL:</strong> <a href="{}" target="_blank" rel="noopener">{}</a>
            </div>"#,
        html_escape(&link.normalized_url),
        html_escape(&link.normalized_url)
    ));

    banner.push_str(&format!(
        r#"<div class="banner-row">
                <strong>Domain:</strong> {}
            </div>"#,
        html_escape(&link.domain)
    ));

    banner.push_str(&format!(
        r#"<div class="banner-row">
                <strong>Archive ID:</strong> <a href="/archive/{}">#{}</a>
            </div>"#,
        archive.id, archive.id
    ));

    if let Some(ref title) = archive.content_title {
        banner.push_str(&format!(
            r#"<div class="banner-row">
                    <strong>Title:</strong> {}
                </div>"#,
            html_escape(title)
        ));
    }

    if let Some(ref author) = archive.content_author {
        banner.push_str(&format!(
            r#"<div class="banner-row">
                    <strong>Author:</strong> {}
                </div>"#,
            html_escape(author)
        ));
    }

    banner.push_str(r#"<div class="banner-links">"#);

    if let Some(ref wayback) = archive.wayback_url {
        banner.push_str(&format!(
            r#"<a href="{}" target="_blank" rel="noopener" class="banner-link">Wayback Machine</a>"#,
            html_escape(wayback)
        ));
    }

    if let Some(ref archive_today) = archive.archive_today_url {
        banner.push_str(&format!(
            r#"<a href="{}" target="_blank" rel="noopener" class="banner-link">Archive.today</a>"#,
            html_escape(archive_today)
        ));
    }

    banner.push_str("</div></div></details></div>");

    banner
}

/// Render archive comparison page with diff view.
pub fn render_comparison(
    archive1: &Archive,
    link1: &Link,
    archive2: &Archive,
    link2: &Link,
    diff_result: &DiffResult,
) -> String {
    let title1 = archive1
        .content_title
        .as_deref()
        .unwrap_or("Untitled Archive");
    let title2 = archive2
        .content_title
        .as_deref()
        .unwrap_or("Untitled Archive");

    let mut content = String::from("<h1>Archive Comparison</h1>");

    // Comparison header with both archives' info
    content.push_str(r#"<div class="comparison-header">"#);

    // Archive 1 info
    content.push_str(&format!(
        r#"<div class="comparison-archive">
            <h3><a href="/archive/{}">{}</a></h3>
            <p class="meta">
                <strong>URL:</strong> <a href="{}">{}</a><br>
                <strong>Archived:</strong> {}
            </p>
        </div>"#,
        archive1.id,
        html_escape(title1),
        html_escape(&link1.normalized_url),
        truncate_url(&link1.normalized_url, 50),
        archive1.archived_at.as_deref().unwrap_or("pending")
    ));

    // Archive 2 info
    content.push_str(&format!(
        r#"<div class="comparison-archive">
            <h3><a href="/archive/{}">{}</a></h3>
            <p class="meta">
                <strong>URL:</strong> <a href="{}">{}</a><br>
                <strong>Archived:</strong> {}
            </p>
        </div>"#,
        archive2.id,
        html_escape(title2),
        html_escape(&link2.normalized_url),
        truncate_url(&link2.normalized_url, 50),
        archive2.archived_at.as_deref().unwrap_or("pending")
    ));

    content.push_str("</div>");

    // Diff statistics
    content.push_str("<section><h2>Content Diff</h2>");

    if diff_result.is_identical {
        content.push_str("<p><em>The content text of these archives is identical.</em></p>");
    } else {
        content.push_str(&format!(
            r#"<div class="diff-stats">
                <span class="diff-stat-additions">+{} additions</span>
                <span class="diff-stat-deletions">-{} deletions</span>
            </div>"#,
            diff_result.additions, diff_result.deletions
        ));

        // Render diff lines
        content.push_str(r#"<div class="diff-container">"#);

        for line in &diff_result.lines {
            let css_class = line.change_type.css_class();
            let symbol = line.change_type.symbol();
            let line_num = match line.change_type {
                crate::web::diff::ChangeType::Added => {
                    line.new_line_num.map_or(String::new(), |n| n.to_string())
                }
                crate::web::diff::ChangeType::Removed => {
                    line.old_line_num.map_or(String::new(), |n| n.to_string())
                }
                crate::web::diff::ChangeType::Unchanged => {
                    line.old_line_num.map_or(String::new(), |n| n.to_string())
                }
            };

            content.push_str(&format!(
                r#"<div class="diff-line {}">
                    <span class="diff-line-num">{}</span>
                    <span class="diff-symbol">{}</span>
                    <span class="diff-line-content">{}</span>
                </div>"#,
                css_class,
                line_num,
                symbol,
                html_escape(&line.content)
            ));
        }

        content.push_str("</div>");
    }

    // Handle case where both archives have no content text
    if archive1.content_text.is_none() && archive2.content_text.is_none() {
        content.push_str(
            "<p><em>Neither archive has text content to compare. They may contain only media.</em></p>",
        );
    }

    content.push_str("</section>");

    base_layout("Archive Comparison", &content)
}

/// Truncate a URL for display.
fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        html_escape(url)
    } else {
        let truncated = &url[..max_len];
        format!("{}...", html_escape(truncated))
    }
}

/// Render debug queue status page.
pub fn render_debug_queue(stats: &QueueStats, recent_failures: &[Archive]) -> String {
    let mut content = String::from("<h1>Debug: Archive Queue Status</h1>");

    // Queue statistics
    content.push_str(r#"<section class="queue-stats"><h2>Queue Statistics</h2>"#);
    content.push_str(r#"<table class="stats-table"><tbody>"#);

    content.push_str(&format!(
        r#"<tr><th>Pending</th><td class="stat-pending">{}</td></tr>"#,
        stats.pending_count
    ));
    content.push_str(&format!(
        r#"<tr><th>Processing</th><td class="stat-processing">{}</td></tr>"#,
        stats.processing_count
    ));
    content.push_str(&format!(
        r#"<tr><th>Failed (awaiting retry)</th><td class="stat-failed">{}</td></tr>"#,
        stats.failed_awaiting_retry
    ));
    content.push_str(&format!(
        r#"<tr><th>Failed (max retries reached)</th><td class="stat-failed">{}</td></tr>"#,
        stats.failed_max_retries
    ));
    content.push_str(&format!(
        r#"<tr><th>Skipped</th><td class="stat-skipped">{}</td></tr>"#,
        stats.skipped_count
    ));
    content.push_str(&format!(
        r#"<tr><th>Complete</th><td class="stat-complete">{}</td></tr>"#,
        stats.complete_count
    ));

    if let Some(ref next_retry) = stats.next_retry_at {
        content.push_str(&format!(
            r#"<tr><th>Next Retry At</th><td>{}</td></tr>"#,
            html_escape(next_retry)
        ));
    }

    if let Some(ref oldest) = stats.oldest_pending_at {
        content.push_str(&format!(
            r#"<tr><th>Oldest Pending</th><td>{}</td></tr>"#,
            html_escape(oldest)
        ));
    }

    content.push_str("</tbody></table></section>");

    // Actions
    content.push_str(r#"<section class="queue-actions"><h2>Actions</h2>"#);

    if stats.skipped_count > 0 {
        content.push_str(&format!(
            r#"<form method="post" action="/debug/reset-skipped" style="display: inline;" onsubmit="return confirm('Reset all {} skipped archives for retry?');">
                <button type="submit" class="debug-button">🔁 Reset All Skipped ({} archives)</button>
            </form>"#,
            stats.skipped_count, stats.skipped_count
        ));
    } else {
        content.push_str("<p>No skipped archives to reset.</p>");
    }

    content.push_str("</section>");

    // Recent failures
    content.push_str(r#"<section class="recent-failures"><h2>Recent Failures</h2>"#);

    if recent_failures.is_empty() {
        content.push_str("<p>No recent failures.</p>");
    } else {
        content.push_str(r#"<table class="failures-table"><thead><tr><th>ID</th><th>Status</th><th>Retries</th><th>Last Attempt</th><th>Error</th><th>Actions</th></tr></thead><tbody>"#);

        for archive in recent_failures {
            let status_class = match archive.status.as_str() {
                "failed" => "status-failed",
                "skipped" => "status-skipped",
                _ => "",
            };
            let error_display = archive
                .error_message
                .as_deref()
                .map(|e| {
                    let truncated = if e.len() > 80 { &e[..80] } else { e };
                    html_escape(truncated)
                })
                .unwrap_or_else(|| "—".to_string());

            content.push_str(&format!(
                r#"<tr>
                    <td><a href="/archive/{id}">#{id}</a></td>
                    <td class="{status_class}">{status}</td>
                    <td>{retries}</td>
                    <td>{last_attempt}</td>
                    <td><code title="{full_error}">{error}</code></td>
                    <td>
                        <form method="post" action="/archive/{id}/rearchive" style="display: inline;">
                            <button type="submit" class="small-button">🔄</button>
                        </form>
                    </td>
                </tr>"#,
                id = archive.id,
                status_class = status_class,
                status = html_escape(&archive.status),
                retries = archive.retry_count,
                last_attempt = archive.last_attempt_at.as_deref().unwrap_or("—"),
                full_error = html_escape(archive.error_message.as_deref().unwrap_or("")),
                error = error_display
            ));
        }

        content.push_str("</tbody></table>");
    }

    content.push_str("</section>");

    // Navigation
    content.push_str(r#"<section class="debug-nav"><p><a href="/">← Back to Home</a> | <a href="/stats">View Stats</a></p></section>"#);

    base_layout("Debug: Queue Status", &content)
}
