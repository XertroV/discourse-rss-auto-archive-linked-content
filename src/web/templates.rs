use crate::db::{Archive, Link, Post};

/// Base HTML layout.
fn base_layout(title: &str, content: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="auto">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="color-scheme" content="light dark">
    <title>{title} - Discourse Link Archiver</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css">
    <link rel="stylesheet" href="/static/css/style.css">
    <link rel="alternate" type="application/rss+xml" title="Archive RSS Feed" href="/feed.rss">
    <link rel="alternate" type="application/atom+xml" title="Archive Atom Feed" href="/feed.atom">
    <script>
        (function() {{
            var theme = localStorage.getItem('theme');
            if (theme) {{
                document.documentElement.setAttribute('data-theme', theme);
            }} else if (window.matchMedia('(prefers-color-scheme: dark)').matches) {{
                document.documentElement.setAttribute('data-theme', 'dark');
            }}
        }})();
    </script>
</head>
<body>
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
                <li><button id="theme-toggle" class="theme-toggle outline" title="Toggle dark mode" aria-label="Toggle dark mode">🌓</button></li>
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
            var toggle = document.getElementById('theme-toggle');
            if (!toggle) return;
            toggle.addEventListener('click', function() {{
                var html = document.documentElement;
                var current = html.getAttribute('data-theme');
                var next = (current === 'dark') ? 'light' : 'dark';
                html.setAttribute('data-theme', next);
                localStorage.setItem('theme', next);
            }});
        }})();
    </script>
</body>
</html>"#
    )
}

/// Render home page with recent archives.
pub fn render_home(archives: &[Archive]) -> String {
    let mut content = String::from("<h1>Recent Archives</h1>");

    if archives.is_empty() {
        content.push_str("<p>No archives yet.</p>");
    } else {
        for archive in archives {
            content.push_str(&render_archive_card(archive));
        }
    }

    base_layout("Home", &content)
}

/// Render search results page.
pub fn render_search(query: &str, archives: &[Archive], page: u32) -> String {
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

    for archive in archives {
        content.push_str(&render_archive_card(archive));
    }

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

    let mut content = format!(
        r#"<h1>{}</h1>
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
    );

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
        content.push_str(&format!(
            "<section><h2>Media</h2><p>S3 Key: {}</p></section>",
            html_escape(primary_key)
        ));
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

    base_layout(title, &content)
}

/// Render site list page.
pub fn render_site_list(site: &str, archives: &[Archive], page: u32) -> String {
    let mut content = format!("<h1>Archives from {}</h1>", html_escape(site));

    if archives.is_empty() {
        content.push_str("<p>No archives from this site.</p>");
    } else {
        for archive in archives {
            content.push_str(&render_archive_card(archive));
        }

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
pub fn render_post_detail(post: &Post, archives: &[Archive]) -> String {
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
        for archive in archives {
            content.push_str(&render_archive_card(archive));
        }
    }

    content.push_str("</section></article>");

    base_layout(&format!("Post: {title}"), &content)
}

/// Render an archive card.
fn render_archive_card(archive: &Archive) -> String {
    let title = archive.content_title.as_deref().unwrap_or("Untitled");

    format!(
        r#"<article class="archive-card">
            <h3><a href="/archive/{}">{}</a></h3>
            <p class="meta">
                <span class="status-{}">{}</span>
                | {}
                | {}
            </p>
        </article>"#,
        archive.id,
        html_escape(title),
        archive.status,
        archive.status,
        archive.content_type.as_deref().unwrap_or("unknown"),
        archive.archived_at.as_deref().unwrap_or("pending")
    )
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

/// Escape HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
