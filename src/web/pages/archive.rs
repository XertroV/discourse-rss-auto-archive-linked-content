//! Archive detail page rendering using maud templates.
//!
//! This module provides the archive detail page which displays comprehensive
//! information about a single archive including:
//! - Archive status and metadata
//! - NSFW warnings
//! - Media players (video/audio/image)
//! - Embedded HTML preview (for webpage archives)
//! - Page captures (screenshot, PDF, MHTML)
//! - Artifacts table with view/download links
//! - Archive jobs history
//! - Quote/reply chain (for Twitter/X)
//! - Plaintext content
//! - Transcript (for YouTube videos)
//! - Playlist content
//! - Debug/metadata section

use maud::{html, Markup, PreEscaped, Render};

use crate::components::{
    render_media_player, BaseLayout, Button, KeyValueTable, NsfwBadge, NsfwWarning,
    OpenGraphMetadata, StatusBadge, Table, TableRow, TableVariant,
};
use crate::db::{Archive, ArchiveArtifact, ArchiveJob, Link, LinkOccurrenceWithPost, User};

/// Parameters for rendering the archive detail page.
#[derive(Debug)]
pub struct ArchiveDetailParams<'a> {
    /// The archive being displayed.
    pub archive: &'a Archive,
    /// The link associated with this archive.
    pub link: &'a Link,
    /// All artifacts for this archive.
    pub artifacts: &'a [ArchiveArtifact],
    /// Link occurrences with post information.
    pub occurrences: &'a [LinkOccurrenceWithPost],
    /// Archive jobs history.
    pub jobs: &'a [ArchiveJob],
    /// Related archives in quote/reply chain (for Twitter).
    pub quote_reply_chain: &'a [Archive],
    /// Current authenticated user (if any).
    pub user: Option<&'a User>,
    /// Whether the archive has missing artifacts (e.g., subtitles, transcripts).
    pub has_missing_artifacts: bool,
    /// Optional Open Graph metadata for social media previews.
    pub og_metadata: Option<OpenGraphMetadata>,
}

/// Render the archive detail page.
#[must_use]
pub fn render_archive_detail_page(params: &ArchiveDetailParams<'_>) -> Markup {
    let archive = params.archive;
    let link = params.link;
    let title = archive
        .content_title
        .as_deref()
        .unwrap_or("Untitled Archive");

    let content = html! {
        // NSFW warning if applicable
        @if archive.is_nsfw {
            (NsfwWarning::new())
        }

        // Page header with title and NSFW badge
        h1 {
            (title)
            @if archive.is_nsfw {
                " " (NsfwBadge::new())
            }
        }

        // Main article content with NSFW data attribute
        article data-nsfw=[archive.is_nsfw.then_some("true")] {
            // Archive header metadata
            header {
                (render_archive_header(archive, link))
            }

            // Auto-refresh notification for pending/processing archives
            @if archive.status == "pending" || archive.status == "processing" {
                (render_auto_refresh_status_box())
            }

            // Error details for failed/skipped archives
            @if archive.status == "failed" || archive.status == "skipped" {
                (render_error_section(archive))
            }

            // Archive actions section (for authorized users)
            (render_actions_section(archive, params.user, params.has_missing_artifacts))

            // Quote/reply chain section (for Twitter)
            @if archive.quoted_archive_id.is_some() || archive.reply_to_archive_id.is_some() {
                (render_quote_reply_chain(archive, params.quote_reply_chain))
            }

            // Content display sections
            (render_content_sections(archive, link, params.artifacts))

            // Media section (for non-HTML archives)
            (render_media_section(archive, link, params.artifacts))

            // Embedded HTML preview (for webpage archives)
            (render_html_embed_section(archive, params.artifacts))

            // Page captures (screenshot, PDF, MHTML)
            (render_captures_section(archive, link, params.artifacts))

            // Platform comments section (if available)
            (render_platform_comments_section(archive, params.artifacts))

            // Artifacts table
            (render_artifacts_section(archive, link, params.artifacts))

            // External archive links (Wayback, Archive.today, IPFS)
            (render_external_archives_section(archive))
        }

        // Link occurrences section
        @if !params.occurrences.is_empty() {
            (render_occurrences_section(params.occurrences))
        }

        // Archive jobs section (collapsible)
        @if !params.jobs.is_empty() {
            (render_jobs_section(params.jobs))
        }

        // Archive metadata section (collapsible)
        (render_metadata_section(archive, link))

        // Comparison form
        (render_comparison_form(archive))

        // Auto-refresh page every 5 seconds for pending/processing archives
        @if archive.status == "pending" || archive.status == "processing" {
            script {
                (PreEscaped(r#"
                (function() {
                    setTimeout(function() {
                        window.location.reload();
                    }, 5000);
                })();
                "#))
            }
        }
    };

    let mut layout = BaseLayout::new(title, params.user);

    if let Some(ref og) = params.og_metadata {
        layout = layout.with_og_metadata(og.clone());
    }

    layout.render(content)
}

// =============================================================================
// Helper Components
// =============================================================================

/// Render the archive header metadata.
fn render_archive_header(archive: &Archive, link: &Link) -> Markup {
    let status_badge =
        StatusBadge::from_status(&archive.status).with_error(archive.error_message.as_deref());

    html! {
        div class="archive-header-grid" {
            // Status card
            div class="archive-info-card" {
                div class="info-label" { "Status" }
                div class="info-value" { (status_badge) }

                // Progress bar for active downloads
                @if archive.status == "processing" && archive.progress_percent.is_some() {
                    (render_progress_bar(archive))
                }
            }

            // Domain card
            div class="archive-info-card" {
                div class="info-label" { "Domain" }
                div class="info-value" { (link.domain) }
            }

            // Archived date card
            div class="archive-info-card" {
                div class="info-label" { "Archived" }
                div class="info-value" {
                    (archive.archived_at.as_deref().unwrap_or("pending"))
                }
            }

            // HTTP Status card (if present)
            @if let Some(code) = archive.http_status_code {
                (render_http_status_card(code))
            }

            // Author card (if present)
            @if let Some(ref author) = archive.content_author {
                div class="archive-info-card" {
                    div class="info-label" { "Author" }
                    div class="info-value" { (author) }
                }
            }
        }

        // Original URL - full width below the cards
        div class="archive-url-section" {
            div class="info-label" { "Original URL" }
            div class="info-value" {
                a href=(link.normalized_url) target="_blank" rel="noopener" class="archive-url-link" {
                    (link.normalized_url)
                }
            }
        }
    }
}

/// Render progress bar for active downloads.
fn render_progress_bar(archive: &Archive) -> Markup {
    let percent = archive.progress_percent.unwrap_or(0.0);
    let details = archive
        .progress_details
        .as_ref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    html! {
        div class="download-progress" id="download-progress" {
            div class="progress-bar" {
                div class="progress-fill" style=(format!("width: {:.1}%", percent)) {
                    span class="progress-text" { (format!("{:.1}%", percent)) }
                }
            }
            @if let Some(d) = details {
                div class="progress-details" {
                    @if let Some(speed) = d.get("speed").and_then(|v| v.as_str()) {
                        span class="progress-speed" { (speed) }
                    }
                    @if let Some(eta) = d.get("eta").and_then(|v| v.as_str()) {
                        " • ETA: "
                        span class="progress-eta" { (eta) }
                    }
                }
            }
        }
        br;
    }
}

/// Render HTTP status code as a card.
fn render_http_status_card(code: i32) -> Markup {
    let (class, emoji) = match code {
        200..=299 => ("http-status-success", "\u{2713}"), // ✓
        301 | 302 | 303 | 307 | 308 => ("http-status-redirect", "\u{21BB}"), // ↻
        400..=499 => ("http-status-client-error", "\u{2717}"), // ✗
        500..=599 => ("http-status-server-error", "\u{26A0}"), // ⚠
        _ => ("http-status-unknown", "?"),
    };

    html! {
        div class="archive-info-card" {
            div class="info-label" { "HTTP Status" }
            div class="info-value" {
                span class=(class) { (emoji) " " (code) }
            }
        }
    }
}

/// Render error details section for failed/skipped archives.
fn render_error_section(archive: &Archive) -> Markup {
    html! {
        section class="archive-error" {
            h2 { "Archive Result" }

            @if let Some(ref error) = archive.error_message {
                p class="error-message" {
                    strong { "Error:" } " "
                    code { (error) }
                }
            }

            @if let Some(ref last_attempt) = archive.last_attempt_at {
                p {
                    strong { "Last Attempt:" } " " (last_attempt)
                }
            }

            p {
                strong { "Retry Count:" } " " (archive.retry_count)
            }
        }
    }
}

/// Render archive actions section for authorized users.
fn render_actions_section(
    archive: &Archive,
    user: Option<&User>,
    has_missing_artifacts: bool,
) -> Markup {
    let is_admin = user.map(|u| u.is_admin).unwrap_or(false);
    let is_approved = user.map(|u| u.is_approved).unwrap_or(false);

    if !is_admin && !is_approved {
        return html! {};
    }

    html! {
        section class="debug-actions" {
            details {
                summary {
                    h2 style="display: inline;" { "Archive Actions" }
                }
                div class="debug-buttons" {
                    // Re-archive button - admins only
                    @if is_admin {
                        form method="post" action=(format!("/archive/{}/rearchive", archive.id))
                             style="display: inline;" {
                            button type="submit" class="debug-button"
                                   title="Re-run the full archive pipeline including redirect handling" {
                                "\u{1F504} Re-archive"  // 🔄
                            }
                        }
                    }

                    // Get missing artifacts button - admins only, only if artifacts are missing
                    @if is_admin && has_missing_artifacts {
                        form method="post" action=(format!("/archive/{}/get-missing-artifacts", archive.id))
                             style="display: inline;" {
                            button type="submit" class="debug-button"
                                   title="Download missing supplementary artifacts (e.g., subtitles, transcripts) without re-archiving" {
                                "\u{1F4E5} Get Missing Artifacts"  // 📥
                            }
                        }
                    }

                    // Toggle NSFW button - approved users and admins
                    @if is_approved || is_admin {
                        form method="post" action=(format!("/archive/{}/toggle-nsfw", archive.id))
                             style="display: inline;" {
                            button type="submit" class="debug-button" title="Toggle NSFW status" {
                                @if archive.is_nsfw {
                                    "\u{1F513} Toggle NSFW"  // 🔓
                                } @else {
                                    "\u{1F51E} Toggle NSFW"  // 🔞
                                }
                            }
                        }
                    }

                    // Retry skipped button - admins only, only for skipped archives
                    @if is_admin && archive.status == "skipped" {
                        form method="post" action=(format!("/archive/{}/retry-skipped", archive.id))
                             style="display: inline;" {
                            button type="submit" class="debug-button"
                                   title="Reset skipped archive for retry" {
                                "\u{1F501} Retry Skipped"  // 🔁
                            }
                        }
                    }

                    // Delete button - admins only
                    @if is_admin {
                        form method="post" action=(format!("/archive/{}/delete", archive.id))
                             style="display: inline;"
                             onsubmit="return confirm('Are you sure you want to delete this archive? This cannot be undone.');" {
                            button type="submit" class="debug-button debug-button-danger"
                                   title="Delete archive and all artifacts" {
                                "\u{1F5D1}\u{FE0F} Delete"  // 🗑️
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render quote/reply chain section for Twitter/X archives.
fn render_quote_reply_chain(archive: &Archive, chain: &[Archive]) -> Markup {
    html! {
        section class="quote-reply-chain" {
            h2 { "Thread Context" }

            // Direct quoted tweet
            @if let Some(quoted_id) = archive.quoted_archive_id {
                (render_chain_item(quoted_id, chain, "quote"))
            }

            // Direct reply-to tweet
            @if let Some(reply_to_id) = archive.reply_to_archive_id {
                (render_chain_item(reply_to_id, chain, "reply"))
            }

            // Full chain if more than the direct links
            @if chain.len() > 2 {
                details class="thread-chain" {
                    summary { "View full thread chain" }
                    ol class="chain-list" {
                        @for chain_archive in chain.iter().skip(1) {
                            li {
                                @let chain_title = chain_archive.content_title.as_deref().unwrap_or("Tweet");
                                @let chain_author = chain_archive.content_author.as_deref()
                                    .map(|a| format!(" (@{})", a))
                                    .unwrap_or_default();
                                a href=(format!("/archive/{}", chain_archive.id)) {
                                    (chain_title) (chain_author)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a single item in the quote/reply chain.
fn render_chain_item(target_id: i64, chain: &[Archive], item_type: &str) -> Markup {
    let (label, icon, class) = if item_type == "quote" {
        ("Quotes:", "\u{1F4AC}", "quote-item") // 💬
    } else {
        ("Reply to:", "\u{21A9}\u{FE0F}", "reply-item") // ↩️
    };

    let target_archive = chain.iter().find(|a| a.id == target_id);

    html! {
        div class=(format!("quote-reply-item {}", class)) {
            span class="quote-reply-label" { (icon) " " (label) }
            @if let Some(target) = target_archive {
                @let target_title = target.content_title.as_deref()
                    .unwrap_or(if item_type == "quote" { "Quoted Tweet" } else { "Parent Tweet" });
                @let target_author = target.content_author.as_deref()
                    .map(|a| format!(" by @{}", a))
                    .unwrap_or_default();
                a href=(format!("/archive/{}", target_id)) {
                    (target_title) (target_author)
                }
            } @else {
                a href=(format!("/archive/{}", target_id)) {
                    "Archive #" (target_id)
                }
            }
        }
    }
}

/// Render content sections (plaintext, transcript, playlist).
fn render_content_sections(
    archive: &Archive,
    link: &Link,
    artifacts: &[ArchiveArtifact],
) -> Markup {
    html! {
        // Playlist content
        @if archive.content_type.as_deref() == Some("playlist") {
            @if let Some(ref metadata_json) = archive.content_text {
                (render_playlist_content(metadata_json))
            }
        } @else if let Some(ref text) = archive.content_text {
            // Plaintext content (collapsible)
            (render_plaintext_section(text))
        }

        // Transcript section (for videos with subtitles)
        @if let Some(transcript) = artifacts.iter().find(|a| a.kind == "transcript") {
            (render_transcript_section(transcript, artifacts, link, archive.id))
        }
    }
}

/// Render plaintext content section.
fn render_plaintext_section(text: &str) -> Markup {
    let text_size = text.len();
    let size_display = if text_size >= 1024 {
        format!("{:.1} KB", text_size as f64 / 1024.0)
    } else {
        format!("{} bytes", text_size)
    };

    html! {
        section class="content-text-section" {
            details {
                summary {
                    h2 style="display: inline;" { "Plaintext Content" }
                    " "
                    span class="text-size" { "(" (size_display) ")" }
                }
                pre class="content-text" { (text) }
            }
        }
    }
}

/// Render interactive transcript section.
fn render_transcript_section(
    transcript: &ArchiveArtifact,
    artifacts: &[ArchiveArtifact],
    link: &Link,
    archive_id: i64,
) -> Markup {
    let transcript_size = transcript.size_bytes.map_or_else(
        || "Unknown".to_string(),
        |size| {
            if size >= 1024 {
                format!("{:.1} KB", size as f64 / 1024.0)
            } else {
                format!("{} bytes", size)
            }
        },
    );

    let subtitle_artifacts: Vec<&ArchiveArtifact> =
        artifacts.iter().filter(|a| a.kind == "subtitles").collect();

    let download_name = suggested_download_filename(&link.domain, archive_id, &transcript.s3_key);

    html! {
        section class="transcript-section" {
            h2 { "Video Transcript" }

            div style="margin-bottom: 1rem;" {
                input type="text" id="transcript-search" placeholder="Search transcript..."
                      style="width: 100%; max-width: 400px; padding: 0.5rem; border: 1px solid var(--border-color); background-color: var(--bg-primary); color: var(--text-primary);";
                span id="match-counter"
                     style="margin-left: 0.5rem; font-size: 0.875rem; color: var(--text-muted);" {}
            }

            details open {
                summary style="cursor: pointer; font-weight: 500; margin-bottom: 0.5rem;" {
                    "Transcript (" (transcript_size) ")"
                }

                div id="transcript-content"
                    data-transcript-url=(format!("/s3/{}", transcript.s3_key))
                    style="max-height: 400px; overflow-y: auto; padding: 1rem; background: var(--bg-tertiary); border: 1px solid var(--border-color); font-family: monospace; white-space: pre-wrap; line-height: 1.6; color: var(--text-primary);" {
                    "Loading transcript..."
                }

                p style="margin-top: 0.5rem;" {
                    (Button::secondary("Download Transcript")
                        .href(&format!("/s3/{}", transcript.s3_key))
                        .download(&download_name))
                }

                // Subtitle files
                @if !subtitle_artifacts.is_empty() {
                    p {
                        strong { "Available subtitle files:" } " "
                        @for (i, sub) in subtitle_artifacts.iter().enumerate() {
                            @if i > 0 {
                                " \u{2022} "  // •
                            }
                            @let sub_download = suggested_download_filename(&link.domain, archive_id, &sub.s3_key);
                            @let lang_info = parse_subtitle_info(sub);
                            a href=(format!("/s3/{}", sub.s3_key))
                              download=(sub_download)
                              title="Download subtitle file" {
                                (lang_info)
                            }
                        }
                    }
                }
            }

            script src="/static/js/transcript.js" {}
            script {
                (PreEscaped(format!(r#"
                    fetch('/s3/{}')
                        .then(response => response.text())
                        .then(text => {{
                            const container = document.getElementById('transcript-content');
                            container.textContent = text;
                            container.setAttribute('data-original-text', text);
                            makeTimestampsClickable(container);
                        }})
                        .catch(err => {{
                            document.getElementById('transcript-content').textContent = 'Failed to load transcript: ' + err.message;
                        }});
                "#, html_escape(&transcript.s3_key))))
            }
            style {
                (PreEscaped(r#"
                    .timestamp-link {
                        color: var(--primary);
                        text-decoration: none;
                        font-weight: 500;
                        cursor: pointer;
                    }
                    .timestamp-link:hover {
                        color: var(--primary-hover);
                        text-decoration: underline;
                    }
                    .highlight {
                        background-color: var(--highlight);
                        color: var(--text-primary);
                        padding: 0.125rem 0.25rem;
                    }
                "#))
            }
        }
    }
}

/// Parse subtitle metadata for display.
fn parse_subtitle_info(sub: &ArchiveArtifact) -> String {
    let filename = sub.s3_key.rsplit('/').next().unwrap_or(&sub.s3_key);

    if let Some(ref metadata) = sub.metadata {
        if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(metadata) {
            let lang = meta_json["language"].as_str().unwrap_or("unknown");
            let is_auto = meta_json["is_auto"].as_bool().unwrap_or(false);
            let format = meta_json["format"].as_str().unwrap_or("vtt");
            let auto_label = if is_auto { " (auto)" } else { "" };
            return format!("{}{} ({})", lang, auto_label, format);
        }
    }

    filename.to_string()
}

/// Render playlist content from JSON metadata.
fn render_playlist_content(metadata_json: &str) -> Markup {
    let playlist_info: Result<serde_json::Value, _> = serde_json::from_str(metadata_json);

    match playlist_info {
        Ok(info) => {
            let title = info.get("title").and_then(|v| v.as_str());
            let video_count = info.get("video_count").and_then(|v| v.as_i64());
            let uploader = info.get("uploader").and_then(|v| v.as_str());
            let videos = info.get("videos").and_then(|v| v.as_array());

            html! {
                section class="playlist-section" {
                    h2 { "Playlist Videos" }

                    div class="playlist-info" {
                        @if let Some(t) = title {
                            p { strong { "Title:" } " " (t) }
                        }
                        @if let Some(count) = video_count {
                            p { strong { "Videos:" } " " (count) }
                        }
                        @if let Some(channel) = uploader {
                            p { strong { "Channel:" } " " (channel) }
                        }
                    }

                    @if let Some(video_list) = videos {
                        table class="playlist-table" {
                            thead {
                                tr {
                                    th { "Position" }
                                    th { "Video Title" }
                                    th { "Channel" }
                                    th { "Date" }
                                    th { "Duration" }
                                    th { "Link" }
                                }
                            }
                            tbody {
                                @for (idx, video) in video_list.iter().enumerate() {
                                    @let position = idx + 1;
                                    @let vtitle = video.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
                                    @let url = video.get("url").and_then(|v| v.as_str()).unwrap_or("#");
                                    @let vuploader = video.get("uploader").and_then(|v| v.as_str()).unwrap_or("Unknown");
                                    @let upload_date = video.get("upload_date").and_then(|v| v.as_str()).unwrap_or("\u{2014}");
                                    @let duration = video.get("duration").and_then(|v| v.as_i64());
                                    @let duration_str = duration.map(|d| format_duration_seconds(d as u64)).unwrap_or_else(|| "\u{2014}".to_string());

                                    tr {
                                        td class="position" { (position) }
                                        td { strong { (vtitle) } }
                                        td { (vuploader) }
                                        td { (upload_date) }
                                        td class="duration" { (duration_str) }
                                        td {
                                            a href=(url) target="_blank" rel="noopener"
                                              class="video-link" title="Watch on YouTube" {
                                                "\u{1F517}"  // 🔗
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(_) => {
            // Fallback to plaintext if JSON parsing fails
            html! {
                section class="content-text-section" {
                    details {
                        summary {
                            h2 style="display: inline;" { "Playlist Metadata" }
                        }
                        pre class="content-text" { (metadata_json) }
                    }
                }
            }
        }
    }
}

/// Render media section for non-HTML archives.
fn render_media_section(archive: &Archive, link: &Link, artifacts: &[ArchiveArtifact]) -> Markup {
    let thumb_key = archive.s3_key_thumb.as_deref().or_else(|| {
        artifacts
            .iter()
            .find(|a| a.kind == "thumb")
            .map(|a| a.s3_key.as_str())
    });

    let primary_key_opt = archive.s3_key_primary.as_deref();
    let is_html_archive = primary_key_opt.is_some_and(|pk| {
        pk.ends_with(".html") || archive.content_type.as_deref() == Some("thread")
    });

    // Don't show media section for HTML archives
    if is_html_archive {
        return html! {};
    }

    // Show primary media if available
    if let Some(primary_key) = primary_key_opt {
        let download_name = suggested_download_filename(&link.domain, archive.id, primary_key);
        return html! {
            section {
                h2 { "Media" }
                (render_media_player(primary_key, archive.content_type.as_deref(), thumb_key))
                p {
                    a href=(format!("/s3/{}", html_escape(primary_key)))
                      class="media-download"
                      download=(download_name)
                      target="_blank" rel="noopener" {
                        span { "Download" }
                    }
                }
            }
        };
    }

    // Fallback: show first video artifact
    if let Some(video_artifact) = artifacts.iter().find(|a| a.kind == "video") {
        let download_name =
            suggested_download_filename(&link.domain, archive.id, &video_artifact.s3_key);
        return html! {
            section {
                h2 { "Media" }
                (render_media_player(&video_artifact.s3_key, Some("video"), thumb_key))
                p {
                    a href=(format!("/s3/{}", html_escape(&video_artifact.s3_key)))
                      class="media-download"
                      download=(download_name)
                      target="_blank" rel="noopener" {
                        span { "Download" }
                    }
                }
            }
        };
    }

    html! {}
}

/// Render embedded HTML preview for webpage archives.
fn render_html_embed_section(archive: &Archive, artifacts: &[ArchiveArtifact]) -> Markup {
    let primary_key_opt = archive.s3_key_primary.as_deref();
    let is_html_archive = primary_key_opt.is_some_and(|pk| {
        pk.ends_with(".html") || archive.content_type.as_deref() == Some("thread")
    });

    if archive.s3_key_primary.is_none() || !is_html_archive {
        return html! {};
    }

    // Find the best HTML artifact: complete.html > view.html > raw.html
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

    let preview_key = match preview_key {
        Some(k) => k,
        None => return html! {},
    };

    let preview_type = if preview_key.ends_with("complete.html") {
        " (Full Archive)"
    } else if preview_key.ends_with("view.html") {
        " (With Banner)"
    } else {
        " (Original)"
    };

    html! {
        section class="embedded-preview" {
            details open {
                summary {
                    h2 style="display: inline;" { "Archived Page Preview" (preview_type) }
                }
                div class="iframe-container" {
                    iframe src=(format!("/s3/{}", html_escape(preview_key)))
                           sandbox="allow-same-origin"
                           loading="lazy"
                           title="Archived webpage preview" {}
                }
            }
        }
    }
}

/// Render page captures section (screenshot, PDF, MHTML).
fn render_captures_section(
    archive: &Archive,
    link: &Link,
    artifacts: &[ArchiveArtifact],
) -> Markup {
    let screenshot = artifacts.iter().find(|a| a.kind == "screenshot");
    let pdf = artifacts.iter().find(|a| a.kind == "pdf");
    let mhtml = artifacts.iter().find(|a| a.kind == "mhtml");
    let nsfw_attr = if archive.is_nsfw { Some("true") } else { None };

    // Show captures section if any capture artifacts exist
    if screenshot.is_some() || pdf.is_some() || mhtml.is_some() {
        html! {
            section class="captures-section" data-nsfw=[nsfw_attr] {
                h2 { "Page Captures" }
                div class="captures-grid" {
                    // Screenshot preview
                    @if let Some(ss) = screenshot {
                        div class="capture-item" {
                            h4 { "Screenshot" }
                            a href=(format!("/s3/{}", ss.s3_key)) target="_blank" rel="noopener" {
                                img src=(format!("/s3/{}", ss.s3_key))
                                    alt="Page screenshot"
                                    class="screenshot-preview"
                                    loading="lazy";
                            }
                            p class="capture-meta" {
                                (ss.size_bytes.map(format_bytes).unwrap_or_else(|| "Unknown size".to_string()))
                            }
                        }
                    }

                    // PDF link
                    @if let Some(pdf_artifact) = pdf {
                        div class="capture-item" {
                            h4 { "PDF Document" }
                            a href=(format!("/s3/{}", pdf_artifact.s3_key))
                              target="_blank" rel="noopener"
                              class="capture-link" {
                                span class="capture-icon" { "\u{1F4C4}" }  // 📄
                                span { "View PDF" }
                            }
                            p class="capture-meta" {
                                (pdf_artifact.size_bytes.map(format_bytes).unwrap_or_else(|| "Unknown size".to_string()))
                            }
                        }
                    }

                    // MHTML link
                    @if let Some(mhtml_artifact) = mhtml {
                        @let download_name = suggested_download_filename(&link.domain, archive.id, &mhtml_artifact.s3_key);
                        div class="capture-item" {
                            h4 { "MHTML Archive" }
                            a href=(format!("/s3/{}", mhtml_artifact.s3_key))
                              download=(download_name)
                              class="capture-link" {
                                span class="capture-icon" { "\u{1F4E6}" }  // 📦
                                span { "Download MHTML" }
                            }
                            p class="capture-meta" {
                                (mhtml_artifact.size_bytes.map(format_bytes).unwrap_or_else(|| "Unknown size".to_string()))
                            }
                        }
                    }
                }
            }
        }
    } else if archive.status == "complete" {
        // Archive completed but no captures
        html! {
            section class="captures-section captures-missing" {
                h2 { "Page Captures" }
                p class="captures-note" {
                    "No screenshot, PDF, or MHTML captures were generated for this archive. "
                    "These captures may be disabled in the server configuration, or the page may have failed to render."
                }
            }
        }
    } else {
        html! {}
    }
}

/// Render artifacts table section.
fn render_artifacts_section(
    archive: &Archive,
    link: &Link,
    artifacts: &[ArchiveArtifact],
) -> Markup {
    if artifacts.is_empty() {
        return html! {};
    }

    let rows: Vec<Markup> = artifacts
        .iter()
        .map(|artifact| render_artifact_row(artifact, link, archive.id))
        .collect();

    let total_size: i64 = artifacts.iter().filter_map(|a| a.size_bytes).sum();

    html! {
        section {
            h2 { "Archived Files" }
            (Table::new(vec!["Type", "File", "Size", "Dedup", "Actions"])
                .variant(TableVariant::Artifacts)
                .rows(rows))

            @if total_size > 0 {
                p class="total-size" {
                    strong { "Total Size:" } " " (format_bytes(total_size))
                }
            }
        }
    }
}

/// Render a single artifact row.
fn render_artifact_row(artifact: &ArchiveArtifact, link: &Link, archive_id: i64) -> Markup {
    let kind_display = artifact_kind_display(&artifact.kind);
    let filename = artifact
        .s3_key
        .rsplit('/')
        .next()
        .unwrap_or(&artifact.s3_key);
    let download_name = suggested_download_filename(&link.domain, archive_id, &artifact.s3_key);
    let size_display = artifact
        .size_bytes
        .map(format_bytes)
        .unwrap_or_else(|| "Unknown".to_string());

    // Dedup info
    let dedup_markup = if let Some(dup_id) = artifact.duplicate_of_artifact_id {
        html! {
            span class="dedup-dup" title=(format!("Duplicate of artifact #{}", dup_id)) {
                "\u{1F517} #" (dup_id)  // 🔗
            }
        }
    } else if artifact.perceptual_hash.is_some() {
        html! {
            span class="dedup-hash" title=(format!("Hash: {}", artifact.perceptual_hash.as_deref().unwrap_or(""))) {
                "\u{2713}"  // ✓
            }
        }
    } else {
        html! { "\u{2014}" } // —
    };

    let is_viewable = is_viewable_in_browser(filename);
    let s3_url = format!("/s3/{}", artifact.s3_key);

    let actions_markup = if is_viewable {
        html! {
            a href=(s3_url) target="_blank" title=(format!("View {}", filename))
              aria-label=(format!("View {}", filename)) class="action-link" {
                (PreEscaped(view_icon()))
            }
            " "
            a href=(s3_url) download=(download_name) title=(format!("Download {}", filename))
              aria-label=(format!("Download {}", filename)) class="action-link" {
                (PreEscaped(download_icon()))
            }
        }
    } else {
        html! {
            a href=(s3_url) download=(download_name) title=(format!("Download {}", filename))
              aria-label=(format!("Download {}", filename)) class="action-link" {
                (PreEscaped(download_icon()))
            }
        }
    };

    TableRow::new()
        .cell_markup(html! {
            span class=(format!("artifact-kind artifact-kind-{}", artifact.kind)) {
                (kind_display)
            }
        })
        .cell_markup(html! { code { (filename) } })
        .cell(&size_display)
        .cell_markup(dedup_markup)
        .cell_markup(actions_markup)
        .render()
}

/// Render external archive links section.
fn render_external_archives_section(archive: &Archive) -> Markup {
    html! {
        @if let Some(ref wayback) = archive.wayback_url {
            section {
                h2 { "Wayback Machine" }
                p {
                    a href=(wayback) target="_blank" rel="noopener" {
                        "View on Wayback Machine"
                    }
                }
            }
        }

        @if let Some(ref archive_today) = archive.archive_today_url {
            section {
                h2 { "Archive.today" }
                p {
                    a href=(archive_today) target="_blank" rel="noopener" {
                        "View on Archive.today"
                    }
                }
            }
        }

        @if let Some(ref ipfs_cid) = archive.ipfs_cid {
            section {
                h2 { "IPFS" }
                p { strong { "CID:" } " " code { (ipfs_cid) } }
                p { strong { "Public Gateways:" } }
                ul {
                    li {
                        a href=(format!("https://ipfs.io/ipfs/{}", ipfs_cid))
                          target="_blank" rel="noopener" { "ipfs.io" }
                    }
                    li {
                        a href=(format!("https://dweb.link/ipfs/{}", ipfs_cid))
                          target="_blank" rel="noopener" { "dweb.link" }
                    }
                    li {
                        a href=(format!("https://gateway.pinata.cloud/ipfs/{}", ipfs_cid))
                          target="_blank" rel="noopener" { "gateway.pinata.cloud" }
                    }
                }
            }
        }
    }
}

/// Render link occurrences section.
fn render_occurrences_section(occurrences: &[LinkOccurrenceWithPost]) -> Markup {
    let rows: Vec<Markup> = occurrences
        .iter()
        .map(|occ| {
            let post_title = occ.post_title.as_deref().unwrap_or("Untitled Post");
            let author = occ.post_author.as_deref().unwrap_or("Unknown");
            let in_quote = if occ.in_quote { "Yes" } else { "No" };

            TableRow::new()
                .cell_markup(html! {
                    a href=(format!("/post/{}", occ.post_guid))
                      title="View all archives from this post" {
                        (post_title) " \u{2192}"  // →
                    }
                })
                .cell(author)
                .cell(in_quote)
                .cell(&occ.seen_at)
                .render()
        })
        .collect();

    html! {
        section class="link-occurrences" {
            h2 { "Link Occurrences" }
            p { "This link has been found in " (occurrences.len()) " post(s):" }
            (Table::new(vec!["Post", "Author", "In Quote", "Seen At"])
                .class("occurrences-table")
                .rows(rows))
        }
    }
}

/// Render archive jobs section (collapsible).
fn render_jobs_section(jobs: &[ArchiveJob]) -> Markup {
    let all_succeeded = jobs
        .iter()
        .all(|j| j.status == "completed" || j.status == "skipped");

    let rows: Vec<Markup> = jobs.iter().map(render_job_row).collect();

    html! {
        section class="archive-jobs" {
            details open[!all_succeeded] {
                summary {
                    h2 style="display: inline;" { "Archive Jobs (" (jobs.len()) ")" }
                }
                (Table::new(vec!["Job", "Status", "Started", "Completed", "Duration", "Details"])
                    .variant(TableVariant::Jobs)
                    .rows(rows))
            }
        }
    }
}

/// Render a single job row.
fn render_job_row(job: &ArchiveJob) -> Markup {
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
        "supplementary_artifacts" => "Supplementary Artifacts",
        other => other,
    };

    let (status_class, status_icon) = match job.status.as_str() {
        "completed" => ("job-completed", "\u{2713}"), // ✓
        "failed" => ("job-failed", "\u{2717}"),       // ✗
        "running" => ("job-running", "\u{27F3}"),     // ⟳
        "pending" => ("job-pending", "\u{23F3}"),     // ⏳
        "skipped" => ("job-skipped", "\u{2298}"),     // ⊘
        _ => ("", ""),
    };

    let started = job.started_at.as_deref().unwrap_or("\u{2014}");
    let completed = job.completed_at.as_deref().unwrap_or("\u{2014}");

    let duration = format_duration(job.duration_seconds);

    let details_markup = if let Some(ref error) = job.error_message {
        let truncated: String = error.chars().take(50).collect();
        html! {
            span class="job-error" title=(error) { (truncated) }
        }
    } else if let Some(ref meta) = job.metadata {
        html! {
            code class="job-meta" { (meta) }
        }
    } else {
        html! { "\u{2014}" } // —
    };

    TableRow::new()
        .cell(job_type_display)
        .cell_markup_with_class(html! { (status_icon) " " (job.status) }, status_class)
        .cell(started)
        .cell(completed)
        .cell(&duration)
        .cell_markup(details_markup)
        .render()
}

/// Format duration in seconds to a human-readable string.
fn format_duration(duration_seconds: Option<f64>) -> String {
    match duration_seconds {
        None => "\u{2014}".to_string(), // —
        Some(secs) if secs < 0.0 => "\u{2014}".to_string(),
        Some(secs) if secs < 1.0 => format!("{:.3}s", secs),
        Some(secs) if secs < 60.0 => format!("{:.1}s", secs),
        Some(secs) if secs < 3600.0 => {
            let mins = (secs / 60.0).floor() as u64;
            let remaining_secs = secs % 60.0;
            format!("{}m {:.1}s", mins, remaining_secs)
        }
        Some(secs) => {
            let hours = (secs / 3600.0).floor() as u64;
            let mins = ((secs % 3600.0) / 60.0).floor() as u64;
            let remaining_secs = secs % 60.0;
            format!("{}h {}m {:.1}s", hours, mins, remaining_secs)
        }
    }
}

/// Render archive metadata section (collapsible).
fn render_metadata_section(archive: &Archive, link: &Link) -> Markup {
    let mut table = KeyValueTable::new()
        .variant(TableVariant::Debug)
        .item("Archive ID", &archive.id.to_string())
        .item("Link ID", &archive.link_id.to_string())
        .item("Created At", &archive.created_at)
        .item("Status", &archive.status)
        .item("Retry Count", &archive.retry_count.to_string());

    if let Some(ref next_retry) = archive.next_retry_at {
        table = table.item("Next Retry At", next_retry);
    }
    if let Some(ref last_attempt) = archive.last_attempt_at {
        table = table.item("Last Attempt At", last_attempt);
    }
    if let Some(ref error) = archive.error_message {
        table = table.item_markup("Error Message", html! { code { (error) } });
    }
    table = table.item("Is NSFW", &archive.is_nsfw.to_string());
    if let Some(ref nsfw_source) = archive.nsfw_source {
        table = table.item("NSFW Source", nsfw_source);
    }
    if let Some(ref content_type) = archive.content_type {
        table = table.item("Content Type", content_type);
    }

    // Link info section header
    let link_section = KeyValueTable::new()
        .variant(TableVariant::Debug)
        .item_markup("Original URL", html! { code { (link.original_url) } })
        .item_markup("Normalized URL", html! { code { (link.normalized_url) } });

    let mut link_table = link_section;
    if let Some(ref canonical) = link.canonical_url {
        link_table = link_table.item_markup("Canonical URL", html! { code { (canonical) } });
    }
    if let Some(ref final_url) = link.final_url {
        link_table = link_table.item_markup(
            "Final URL (after redirects)",
            html! { code { (final_url) } },
        );
    }
    link_table = link_table.item("Domain", &link.domain);
    if let Some(ref last_archived) = link.last_archived_at {
        link_table = link_table.item("Last Archived At", last_archived);
    }

    html! {
        section class="debug-info" {
            details {
                summary {
                    h2 style="display: inline;" { "Archive Metadata" }
                }
                (table)
                // Link info header row
                table class="debug-table" {
                    tbody {
                        tr {
                            th colspan="2" style="background: var(--border-color);" { "Link Info" }
                        }
                    }
                }
                (link_table)
            }
        }
    }
}

/// Render comparison form.
fn render_comparison_form(archive: &Archive) -> Markup {
    let archive_id = archive.id;

    html! {
        section class="comparison-form" {
            h2 { "Compare with Another Archive" }
            form method="get" action=(format!("/compare/{}/{{}}", archive_id))
                 onsubmit=(format!("this.action = '/compare/{}/'+this.archiveId.value; return true;", archive_id)) {
                input type="number" name="archiveId" placeholder="Archive ID"
                      required min="1" style="width: 150px;";
                button type="submit" { "Compare" }
            }
            p {
                small { "Enter an archive ID to compare content differences." }
            }
        }
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

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
        "transcript" => "Transcript",
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
        // Video files
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
        // Subtitle files
        || filename_lower.ends_with(".vtt")
        || filename_lower.ends_with(".srt")
}

/// Generate a suggested download filename.
fn suggested_download_filename(domain: &str, archive_id: i64, s3_key: &str) -> String {
    let filename = s3_key.rsplit('/').next().unwrap_or(s3_key);
    let domain = sanitize_filename_component(&domain.to_lowercase());
    let filename = sanitize_filename_component(filename);

    if filename.is_empty() {
        format!("{}_archive_{}", domain, archive_id)
    } else {
        format!("{}_archive_{}_{}", domain, archive_id, filename)
    }
}

/// Sanitize a string for use in filenames.
fn sanitize_filename_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Format bytes into human-readable format.
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

/// Format duration in seconds to HH:MM:SS or MM:SS.
fn format_duration_seconds(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

/// HTML-escape a string for safe inclusion in HTML.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Render platform comments section (collapsed by default).
fn render_platform_comments_section(archive: &Archive, artifacts: &[ArchiveArtifact]) -> Markup {
    // Find comments artifact
    let comments_artifact = match artifacts.iter().find(|a| a.kind == "comments") {
        Some(a) => a,
        None => return html! {}, // No comments artifact, render nothing
    };

    // Extract comment count from metadata if available
    let comment_count = comments_artifact
        .metadata
        .as_ref()
        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .and_then(|json| {
            json.get("stats")
                .and_then(|s| s.get("extracted_comments"))
                .and_then(|c| c.as_i64())
        })
        .unwrap_or(0);

    html! {
        section class="platform-comments-section" id="platform-comments" {
            details {
                summary {
                    h2 style="display: inline;" { "Comments" }
                    " "
                    span class="comments-count-badge" {
                        (comment_count) " comments"
                    }
                }

                div class="comments-loading-container"
                    data-comments-url=(format!("/api/archive/{}/comments", archive.id))
                    data-archive-id=(archive.id) {
                    p class="loading-message" { "Loading comments..." }
                }
            }

            script src="/static/js/comments.js" {}
        }
    }
}

/// Render status box for auto-refresh notification
fn render_auto_refresh_status_box() -> Markup {
    html! {
        div class="alert alert-info" role="alert" style="margin-bottom: 1rem;" {
            span class="alert-icon" { "🔄" }
            span class="alert-message" {
                "This page will automatically refresh every 5 seconds while the archive is processing."
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_archive() -> Archive {
        Archive {
            id: 1,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15 12:00:00".to_string()),
            content_title: Some("Test Archive Title".to_string()),
            content_author: Some("testauthor".to_string()),
            content_text: Some("Sample content text".to_string()),
            content_type: Some("video".to_string()),
            s3_key_primary: Some("archives/1/video.mp4".to_string()),
            s3_key_thumb: Some("archives/1/thumb.jpg".to_string()),
            s3_keys_extra: None,
            wayback_url: Some("https://web.archive.org/web/example".to_string()),
            archive_today_url: None,
            ipfs_cid: None,
            error_message: None,
            retry_count: 0,
            created_at: "2024-01-15 10:00:00".to_string(),
            is_nsfw: false,
            nsfw_source: None,
            next_retry_at: None,
            last_attempt_at: Some("2024-01-15 11:00:00".to_string()),
            http_status_code: Some(200),
            post_date: None,
            quoted_archive_id: None,
            reply_to_archive_id: None,
            submitted_by_user_id: None,
            progress_percent: None,
            progress_details: None,
            last_progress_update: None,
            og_title: None,
            og_description: None,
            og_image: None,
            og_type: None,
            og_extracted_at: None,
            og_extraction_attempted: false,
        }
    }

    fn sample_link() -> Link {
        Link {
            id: 1,
            original_url: "https://example.com/video/123".to_string(),
            normalized_url: "https://example.com/video/123".to_string(),
            canonical_url: None,
            final_url: None,
            domain: "example.com".to_string(),
            first_seen_at: "2024-01-15 09:00:00".to_string(),
            last_archived_at: Some("2024-01-15 12:00:00".to_string()),
        }
    }

    fn sample_artifact() -> ArchiveArtifact {
        ArchiveArtifact {
            id: 1,
            archive_id: 1,
            kind: "video".to_string(),
            s3_key: "archives/1/video.mp4".to_string(),
            content_type: Some("video/mp4".to_string()),
            size_bytes: Some(1048576),
            sha256: Some("abc123".to_string()),
            created_at: "2024-01-15 12:00:00".to_string(),
            perceptual_hash: None,
            duplicate_of_artifact_id: None,
            video_file_id: None,
            metadata: None,
        }
    }

    fn sample_job() -> ArchiveJob {
        ArchiveJob {
            id: 1,
            archive_id: 1,
            job_type: "yt_dlp".to_string(),
            status: "completed".to_string(),
            started_at: Some("2024-01-15 11:00:00".to_string()),
            completed_at: Some("2024-01-15 11:30:00".to_string()),
            error_message: None,
            metadata: None,
            created_at: "2024-01-15 11:00:00".to_string(),
            duration_seconds: Some(1800.0),
        }
    }

    #[test]
    fn test_render_archive_detail_page_basic() {
        let archive = sample_archive();
        let link = sample_link();
        let artifacts = vec![sample_artifact()];
        let jobs = vec![sample_job()];

        let params = ArchiveDetailParams {
            archive: &archive,
            link: &link,
            artifacts: &artifacts,
            occurrences: &[],
            jobs: &jobs,
            quote_reply_chain: &[],
            user: None,
            has_missing_artifacts: false,
            og_metadata: None,
        };

        let html = render_archive_detail_page(&params).into_string();

        // Check basic structure
        assert!(html.contains("Test Archive Title"));
        assert!(html.contains("example.com"));
        assert!(html.contains("status-complete"));
        assert!(html.contains("Wayback Machine"));
    }

    #[test]
    fn test_render_archive_detail_page_nsfw() {
        let mut archive = sample_archive();
        archive.is_nsfw = true;
        let link = sample_link();

        let params = ArchiveDetailParams {
            archive: &archive,
            link: &link,
            artifacts: &[],
            occurrences: &[],
            jobs: &[],
            quote_reply_chain: &[],
            user: None,
            has_missing_artifacts: false,
            og_metadata: None,
        };

        let html = render_archive_detail_page(&params).into_string();

        assert!(html.contains("nsfw-warning"));
        assert!(html.contains("nsfw-badge"));
        assert!(html.contains("NSFW"));
    }

    #[test]
    fn test_render_archive_detail_page_failed() {
        let mut archive = sample_archive();
        archive.status = "failed".to_string();
        archive.error_message = Some("Connection timeout".to_string());
        let link = sample_link();

        let params = ArchiveDetailParams {
            archive: &archive,
            link: &link,
            artifacts: &[],
            occurrences: &[],
            jobs: &[],
            quote_reply_chain: &[],
            user: None,
            has_missing_artifacts: false,
            og_metadata: None,
        };

        let html = render_archive_detail_page(&params).into_string();

        assert!(html.contains("archive-error"));
        assert!(html.contains("Connection timeout"));
        assert!(html.contains("status-failed"));
    }

    #[test]
    fn test_render_archive_header() {
        let archive = sample_archive();
        let link = sample_link();

        let html = render_archive_header(&archive, &link).into_string();

        assert!(html.contains("Status:"));
        assert!(html.contains("Original URL:"));
        assert!(html.contains("Domain:"));
        assert!(html.contains("example.com"));
        assert!(html.contains("HTTP Status:"));
        assert!(html.contains("200"));
    }

    #[test]
    fn test_render_artifact_row() {
        let artifact = sample_artifact();
        let link = sample_link();

        let html = render_artifact_row(&artifact, &link, 1).into_string();

        assert!(html.contains("Video"));
        assert!(html.contains("video.mp4"));
        assert!(html.contains("1.0 MB"));
    }

    #[test]
    fn test_render_job_row() {
        let job = sample_job();
        let html = render_job_row(&job).into_string();

        assert!(html.contains("yt-dlp"));
        assert!(html.contains("job-completed"));
        assert!(html.contains("2024-01-15 11:00:00"));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration_seconds(45), "00:45");
        assert_eq!(format_duration_seconds(125), "02:05");
        assert_eq!(format_duration_seconds(3661), "01:01:01");
    }

    #[test]
    fn test_suggested_download_filename() {
        assert_eq!(
            suggested_download_filename("example.com", 123, "archives/123/video.mp4"),
            "example.com_archive_123_video.mp4"
        );
    }

    #[test]
    fn test_is_viewable_in_browser() {
        assert!(is_viewable_in_browser("file.html"));
        assert!(is_viewable_in_browser("file.HTML"));
        assert!(is_viewable_in_browser("file.pdf"));
        assert!(is_viewable_in_browser("file.jpg"));
        assert!(is_viewable_in_browser("file.mp4"));
        assert!(!is_viewable_in_browser("file.mkv"));
        assert!(!is_viewable_in_browser("file.zip"));
    }

    #[test]
    fn test_artifact_kind_display() {
        assert_eq!(artifact_kind_display("raw_html"), "HTML (Original)");
        assert_eq!(
            artifact_kind_display("complete_html"),
            "HTML (Full Archive)"
        );
        assert_eq!(artifact_kind_display("video"), "Video");
        assert_eq!(artifact_kind_display("unknown"), "File");
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn test_render_plaintext_section() {
        let html = render_plaintext_section("Hello, world!").into_string();

        assert!(html.contains("content-text-section"));
        assert!(html.contains("Plaintext Content"));
        assert!(html.contains("Hello, world!"));
        assert!(html.contains("13 bytes"));
    }

    #[test]
    fn test_render_playlist_content_valid() {
        let json = r#"{"title": "Test Playlist", "video_count": 5, "uploader": "TestChannel", "videos": [{"title": "Video 1", "url": "https://example.com/1", "uploader": "TestChannel", "upload_date": "2024-01-01", "duration": 120}]}"#;
        let html = render_playlist_content(json).into_string();

        assert!(html.contains("playlist-section"));
        assert!(html.contains("Test Playlist"));
        assert!(html.contains("TestChannel"));
        assert!(html.contains("Video 1"));
    }

    #[test]
    fn test_render_playlist_content_invalid() {
        let html = render_playlist_content("not valid json").into_string();

        assert!(html.contains("content-text-section"));
        assert!(html.contains("Playlist Metadata"));
    }

    #[test]
    fn test_render_actions_section_no_user() {
        let archive = sample_archive();
        let html = render_actions_section(&archive, None, false).into_string();

        assert!(html.is_empty());
    }

    #[test]
    fn test_render_actions_section_admin() {
        let archive = sample_archive();
        let user = User {
            id: 1,
            username: "admin".to_string(),
            password_hash: "hash".to_string(),
            email: None,
            display_name: None,
            is_approved: true,
            is_admin: true,
            is_active: true,
            failed_login_attempts: 0,
            locked_until: None,
            password_updated_at: "2024-01-01".to_string(),
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        };

        let html = render_actions_section(&archive, Some(&user), false).into_string();

        assert!(html.contains("debug-actions"));
        assert!(html.contains("Re-archive"));
        assert!(html.contains("Toggle NSFW"));
        assert!(html.contains("Delete"));
    }
}
