/**
 * Platform Comments Loader and Renderer
 *
 * Handles fetching and displaying archived platform comments (TikTok, YouTube, etc.)
 * on archive detail pages.
 */

/**
 * Load and render platform comments when details element is opened
 */
document.addEventListener('DOMContentLoaded', () => {
    const detailsElements = document.querySelectorAll('.platform-comments-section details');

    detailsElements.forEach(details => {
        details.addEventListener('toggle', function() {
            if (this.open) {
                const container = this.querySelector('.comments-loading-container');
                if (container && !container.dataset.loaded) {
                    loadPlatformComments(container);
                }
            }
        });
    });
});

/**
 * Fetch and render comments from API
 */
async function loadPlatformComments(container) {
    const url = container.dataset.commentsUrl;
    container.dataset.loaded = 'true';

    try {
        const response = await fetch(url);
        if (!response.ok) {
            if (response.status === 404) {
                container.innerHTML = '<p class="comments-empty">No comments available.</p>';
            } else {
                throw new Error(`HTTP ${response.status}`);
            }
            return;
        }

        const data = await response.json();
        renderComments(container, data);
    } catch (error) {
        console.error('Error loading comments:', error);
        container.innerHTML = '<p class="comments-error">Failed to load comments. Please try again later.</p>';
    }
}

/**
 * Render comments JSON into HTML
 */
function renderComments(container, data) {
    const { platform, stats, comments, limited, limit_applied } = data;

    let html = '';

    // Stats header
    html += `<div class="comments-stats">`;
    html += `<p class="comments-summary">`;
    html += `Showing <strong>${stats.extracted_comments}</strong>`;
    if (stats.total_comments !== stats.extracted_comments) {
        html += ` of <strong>${stats.total_comments}</strong>`;
    }
    html += ` comments`;
    if (limited) {
        html += ` <span class="text-muted">(limited to ${limit_applied})</span>`;
    }
    html += `</p>`;
    html += `</div>`;

    // Comment list
    html += `<div class="comments-list">`;
    if (comments.length === 0) {
        html += `<p class="comments-empty">No comments found.</p>`;
    } else {
        comments.forEach(comment => {
            html += renderComment(comment, 0);
        });
    }
    html += `</div>`;

    container.innerHTML = html;
}

/**
 * Render a single comment
 */
function renderComment(comment, depth) {
    const indentClass = depth > 0 ? `comment-depth-${Math.min(depth, 3)}` : '';
    const pinnedClass = comment.is_pinned ? 'comment-pinned' : '';
    const creatorClass = comment.is_creator ? 'comment-creator' : '';

    let html = `
        <div class="platform-comment ${indentClass} ${pinnedClass} ${creatorClass}">
            <div class="comment-header">
                <span class="comment-author">${escapeHtml(comment.author)}</span>
                ${comment.is_creator ? '<span class="badge-creator">Creator</span>' : ''}
                ${comment.is_pinned ? '<span class="badge-pinned">Pinned</span>' : ''}
                ${renderTimestamp(comment.timestamp)}
            </div>
            <div class="comment-body">
                <p class="comment-text">${escapeHtml(comment.text)}</p>
            </div>
            <div class="comment-footer">
                <span class="comment-likes" title="${comment.likes} likes">
                    ❤️ ${formatNumber(comment.likes)}
                </span>
    `;

    if (comment.replies && comment.replies.length > 0 && depth < 3) {
        html += `<span class="comment-replies-count">${comment.replies.length} ${comment.replies.length === 1 ? 'reply' : 'replies'}</span>`;
    }

    html += `</div>`;

    // Render replies recursively
    if (comment.replies && comment.replies.length > 0 && depth < 3) {
        html += `<div class="comment-replies">`;
        comment.replies.forEach(reply => {
            html += renderComment(reply, depth + 1);
        });
        html += `</div>`;
    }

    html += `</div>`;
    return html;
}

/**
 * Format timestamp
 */
function renderTimestamp(timestamp) {
    if (!timestamp) return '';
    const date = new Date(timestamp * 1000);
    const relative = getRelativeTime(date);
    return `<span class="comment-timestamp" title="${date.toLocaleString()}">${relative}</span>`;
}

/**
 * Get relative time string
 */
function getRelativeTime(date) {
    const now = new Date();
    const diff = Math.floor((now - date) / 1000);

    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;
    return date.toLocaleDateString();
}

/**
 * Format numbers (1000 -> 1K)
 */
function formatNumber(num) {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toString();
}

/**
 * Escape HTML to prevent XSS
 */
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}
