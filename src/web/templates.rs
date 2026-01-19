use crate::db::{Archive, ArchiveDisplay, Link, Post};
use crate::web::diff::DiffResult;

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
pub fn render_home(archives: &[ArchiveDisplay]) -> String {
    let mut content = String::from("<h1>Recent Archives</h1>");

    if archives.is_empty() {
        content.push_str("<p>No archives yet.</p>");
    } else {
        content.push_str(r#"<div class="archive-grid">"#);
        for archive in archives {
            content.push_str(&render_archive_card_display(archive));
        }
        content.push_str("</div>");
    }

    base_layout("Home", &content)
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
pub fn render_archive_detail(archive: &Archive, link: &Link) -> String {
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
        content.push_str(r#"<div class="nsfw-warning"><strong>Warning:</strong> This archive contains content marked as NSFW (Not Safe For Work).</div>"#);
    }

    content.push_str(&format!(
        r#"<h1>{}{nsfw_badge}</h1>
        <article>
            <header>
                <p class="meta">
                    <strong>Status:</strong> <span class="status-{}">{}</span><br>
                    <strong>Original URL:</strong> <a href="{}">{}</a><br>
                    <strong>Domain:</strong> {}<br>
                    <strong>Archived:</strong> {}
                </p>
            </header>"#,
        html_escape(title),
        archive.status,
        archive.status,
        html_escape(&link.normalized_url),
        html_escape(&link.normalized_url),
        html_escape(&link.domain),
        archive.archived_at.as_deref().unwrap_or("pending")
    ));

    if let Some(ref author) = archive.content_author {
        content.push_str(&format!(
            "<p><strong>Author:</strong> {}</p>",
            html_escape(author)
        ));
    }

    if let Some(ref text) = archive.content_text {
        content.push_str(&format!(
            "<section><h2>Content</h2><p>{}</p></section>",
            html_escape(text)
        ));
    }

    if let Some(ref primary_key) = archive.s3_key_primary {
        content.push_str("<section><h2>Media</h2>");
        content.push_str(&render_media_player(
            primary_key,
            archive.content_type.as_deref(),
            archive.s3_key_thumb.as_deref(),
        ));
        content.push_str(&format!(
            r#"<p><a href="/s3/{}" class="media-download" download>
                <span>Download</span>
            </a></p>"#,
            html_escape(primary_key)
        ));
        content.push_str("</section>");
    }

    if let Some(ref wayback) = archive.wayback_url {
        content.push_str(&format!(
            "<section><h2>Wayback Machine</h2><p><a href=\"{}\">View on Wayback Machine</a></p></section>",
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
            "<li><a href=\"https://cloudflare-ipfs.com/ipfs/{cid}\" target=\"_blank\" rel=\"noopener\">Cloudflare IPFS</a></li>",
            cid = html_escape(ipfs_cid)
        ));
        content.push_str(&format!(
            "<li><a href=\"https://dweb.link/ipfs/{cid}\" target=\"_blank\" rel=\"noopener\">dweb.link</a></li>",
            cid = html_escape(ipfs_cid)
        ));
        content.push_str(&format!(
            "<li><a href=\"https://gateway.pinata.cloud/ipfs/{cid}\" target=\"_blank\" rel=\"noopener\">Pinata</a></li>",
            cid = html_escape(ipfs_cid)
        ));
        content.push_str("</ul></section>");
    }

    content.push_str("</article>");

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
                    <strong>Source:</strong> <a href="{}">{}</a>
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
        Some("image") | Some("gallery") => {
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
            </div>"#,
            type_badge = type_badge,
            media_url = media_url
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
pub fn render_archive_banner(archive: &Archive, link: &Link) -> String {
    let mut banner = String::from(
        r#"<div id="archive-banner" class="archive-banner">
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
