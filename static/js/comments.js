/**
 * Platform Comments Loader and Renderer
 *
 * Handles fetching and displaying archived platform comments (TikTok, YouTube, etc.)
 * on archive detail pages. Supports threaded view with collapsible replies.
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
 * Build a threaded comment tree from a flat list.
 *
 * Comments with parent_id === "root" (or null/missing) are top-level.
 * All others are keyed under their parent_id in childrenMap.
 *
 * @param {Array} comments - flat comment list
 * @returns {{ roots: Array, childrenMap: Map<string, Array> }}
 */
function buildCommentTree(comments) {
    const childrenMap = new Map();
    const roots = [];

    comments.forEach(comment => {
        const parentId = comment.parent_id;
        if (!parentId || parentId === 'root') {
            roots.push(comment);
        } else {
            if (!childrenMap.has(parentId)) {
                childrenMap.set(parentId, []);
            }
            childrenMap.get(parentId).push(comment);
        }
    });

    return { roots, childrenMap };
}

/**
 * Render comments JSON into HTML
 */
function renderComments(container, data) {
    const { platform, stats, comments, limited, limit_applied } = data;

    // Update the summary badge with actual count
    const details = container.closest('details');
    if (details) {
        const badge = details.querySelector('.comments-count-badge');
        if (badge) {
            badge.textContent = `${stats.extracted_comments} comments`;
        }
    }

    // Build thread tree from flat list (supports both flat parent_id and pre-nested replies)
    const { roots, childrenMap } = buildCommentTree(comments);

    // Merge pre-nested replies into childrenMap (for formats that already nest)
    comments.forEach(comment => {
        if (comment.replies && comment.replies.length > 0) {
            const existing = childrenMap.get(comment.id) || [];
            // Only add replies not already in childrenMap (avoid duplication)
            const existingIds = new Set(existing.map(r => r.id));
            comment.replies.forEach(reply => {
                if (!existingIds.has(reply.id)) {
                    existing.push(reply);
                }
            });
            if (existing.length > 0) {
                childrenMap.set(comment.id, existing);
            }
        }
    });

    // Store original data for filtering/sorting (only top-level roots)
    container.dataset.originalComments = JSON.stringify(roots);
    container.dataset.allComments = JSON.stringify(comments);
    container.dataset.platform = platform;

    let html = '';

    // Stats header
    const topLevelCount = roots.length;
    const replyCount = comments.length - topLevelCount;
    html += renderStatsHeader(stats, limited, limit_applied, topLevelCount, replyCount);

    // Controls bar
    html += `
    <div class="comments-controls">
        <div class="comments-search">
            <input type="text"
                   id="comment-search-input"
                   class="search-input"
                   placeholder="Search comments..."
                   autocomplete="off">
            <button id="comment-search-clear" class="btn-clear" style="display:none;">✕</button>
        </div>

        <div class="comments-filters">
            <label for="comment-sort">Sort:</label>
            <select id="comment-sort" class="filter-select">
                <option value="default">Default</option>
                <option value="likes-desc">Most Liked</option>
                <option value="likes-asc">Least Liked</option>
                <option value="newest">Newest First</option>
                <option value="oldest">Oldest First</option>
            </select>

            <label for="comment-filter">Filter:</label>
            <select id="comment-filter" class="filter-select">
                <option value="all">All Comments</option>
                <option value="pinned">Pinned Only</option>
                <option value="creator">Creator Only</option>
                <option value="popular">Popular (10+ likes)</option>
            </select>
        </div>
    </div>`;

    // Render list (pass childrenMap via container dataset)
    container.dataset.childrenMap = JSON.stringify(Object.fromEntries(childrenMap));
    renderCommentList(container, roots, platform, html);

    // Attach event listeners
    attachControlHandlers(container);
}

/**
 * Render stats header
 */
function renderStatsHeader(stats, limited, limit_applied, topLevelCount, replyCount) {
    let html = `<div class="comments-stats">`;
    html += `<p class="comments-summary">`;
    html += `Showing <strong>${stats.extracted_comments}</strong>`;
    if (stats.total_comments !== stats.extracted_comments) {
        html += ` of <strong>${stats.total_comments}</strong>`;
    }
    html += ` comments`;
    if (topLevelCount !== undefined && replyCount !== undefined && replyCount > 0) {
        html += ` <span class="text-muted">(${topLevelCount} top-level, ${replyCount} replies)</span>`;
    }
    if (limited) {
        html += ` <span class="text-muted">(limited to ${limit_applied})</span>`;
    }
    html += `</p>`;
    html += `</div>`;
    return html;
}

/**
 * Render comment list (extracted for reuse)
 */
function renderCommentList(container, comments, platform, headerHtml = '') {
    // Restore childrenMap from dataset
    const childrenMap = restoreChildrenMap(container);

    let html = headerHtml;

    if (comments.length > 200) {
        // Virtual scroll for large lists
        html += `<div class="comments-list virtual-scroll" style="max-height: 600px; overflow-y: auto;"></div>`;
        container.innerHTML = html;
        const listContainer = container.querySelector('.virtual-scroll');
        new VirtualCommentScroll(listContainer, comments, platform, childrenMap);
    } else {
        // Standard render for small lists
        html += `<div class="comments-list">`;
        if (comments.length === 0) {
            html += `<p class="comments-empty">No comments found matching your filters.</p>`;
        } else {
            comments.forEach(comment => {
                html += renderComment(comment, 0, platform, childrenMap);
            });
        }
        html += `</div>`;
        container.innerHTML = html;
    }

    // Attach reply toggle handlers after DOM update
    attachReplyToggleHandlers(container);
}

/**
 * Restore childrenMap from container dataset
 */
function restoreChildrenMap(container) {
    try {
        const raw = container.dataset.childrenMap;
        if (!raw) return new Map();
        const obj = JSON.parse(raw);
        return new Map(Object.entries(obj));
    } catch (e) {
        return new Map();
    }
}

/**
 * Render a single comment with collapsible replies
 */
function renderComment(comment, depth, platform, childrenMap) {
    const indentClass = depth > 0 ? `comment-depth-${Math.min(depth, 3)}` : '';
    const pinnedClass = comment.is_pinned ? 'comment-pinned' : '';
    const creatorClass = comment.is_creator ? 'comment-creator' : '';

    // Generate avatar initial
    const avatarInitial = comment.author ? comment.author.charAt(0).toUpperCase() : '?';
    const commentId = comment.id || `${comment.author}-${comment.timestamp || Date.now()}`;

    // Gather replies from both childrenMap and pre-nested replies array
    const replies = (childrenMap && childrenMap.get(comment.id)) || comment.replies || [];

    let html = `
        <div class="platform-comment ${indentClass} ${pinnedClass} ${creatorClass}"
             data-platform="${platform}"
             data-comment-id="${escapeHtml(String(commentId))}">
            <div class="comment-header">
                <div class="comment-author-wrapper">
                    <div class="comment-avatar">${avatarInitial}</div>
                    <span class="comment-author">${escapeHtml(comment.author || '')}</span>
                </div>
                ${comment.is_creator ? '<span class="badge-creator">Creator</span>' : ''}
                ${comment.is_pinned ? '<span class="badge-pinned">Pinned</span>' : ''}
                ${renderTimestamp(comment.timestamp)}
            </div>
            <div class="comment-body">
                <p class="comment-text">${escapeHtml(comment.text || '')}</p>
            </div>
            <div class="comment-footer">
                <div class="comment-engagement">
                    <span class="comment-likes" title="${comment.likes} likes">
                        ❤️ ${formatNumber(comment.likes || 0)}
                    </span>
    `;

    if (replies.length > 0 && depth < 3) {
        html += `
                    <button class="comment-replies-toggle"
                            data-comment-id="${escapeHtml(String(commentId))}"
                            aria-expanded="false">
                        💬 ${replies.length} ${replies.length === 1 ? 'reply' : 'replies'}
                    </button>`;
    }

    html += `
                </div>
            </div>`;

    // Collapsible replies block (hidden by default)
    if (replies.length > 0 && depth < 3) {
        html += `<div class="comment-replies" data-parent-id="${escapeHtml(String(commentId))}" hidden>`;
        replies.forEach(reply => {
            html += renderComment(reply, depth + 1, platform, childrenMap);
        });
        html += `</div>`;
    }

    html += `</div>`;
    return html;
}

/**
 * Attach click handlers for reply toggle buttons
 */
function attachReplyToggleHandlers(container) {
    container.querySelectorAll('.comment-replies-toggle').forEach(btn => {
        // Remove any prior listener by cloning
        const fresh = btn.cloneNode(true);
        btn.parentNode.replaceChild(fresh, btn);

        fresh.addEventListener('click', function() {
            const commentId = this.dataset.commentId;
            const repliesDiv = this.closest('.platform-comment')
                                   .querySelector(`.comment-replies[data-parent-id="${CSS.escape(commentId)}"]`);
            if (!repliesDiv) return;

            const isOpen = !repliesDiv.hidden;
            repliesDiv.hidden = isOpen;
            this.setAttribute('aria-expanded', String(!isOpen));
            this.classList.toggle('replies-open', !isOpen);
        });
    });
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

/**
 * Virtual scroll manager for large comment lists
 */
class VirtualCommentScroll {
    constructor(container, comments, platform, childrenMap, renderHeight = 150) {
        this.container = container;
        this.comments = comments;
        this.platform = platform;
        this.childrenMap = childrenMap;
        this.renderHeight = renderHeight;  // Estimated comment height
        this.visibleCount = 50;  // Render 50 at a time
        this.bufferCount = 10;   // Pre-render buffer
        this.scrollTop = 0;
        this.renderedComments = new Map();  // Cache rendered HTML

        this.init();
    }

    init() {
        // Create virtual scroll wrapper
        const totalHeight = this.comments.length * this.renderHeight;
        this.container.style.height = `${totalHeight}px`;
        this.container.style.position = 'relative';

        // Create viewport
        this.viewport = document.createElement('div');
        this.viewport.className = 'virtual-scroll-viewport';
        this.viewport.style.position = 'absolute';
        this.viewport.style.top = '0';
        this.viewport.style.left = '0';
        this.viewport.style.right = '0';

        this.container.appendChild(this.viewport);

        // Initial render
        this.render();

        // Scroll listener with throttle
        let scrollTimeout;
        this.container.addEventListener('scroll', () => {
            clearTimeout(scrollTimeout);
            scrollTimeout = setTimeout(() => this.render(), 100);
        });
    }

    render() {
        const scrollTop = this.container.scrollTop || 0;
        const startIndex = Math.max(0, Math.floor(scrollTop / this.renderHeight) - this.bufferCount);
        const endIndex = Math.min(
            this.comments.length,
            startIndex + this.visibleCount + (this.bufferCount * 2)
        );

        // Build HTML for visible range
        let html = '';
        for (let i = startIndex; i < endIndex; i++) {
            const comment = this.comments[i];

            // Use cached HTML if available
            if (!this.renderedComments.has(i)) {
                this.renderedComments.set(i, renderComment(comment, 0, this.platform, this.childrenMap));
            }

            html += this.renderedComments.get(i);
        }

        // Update viewport position and content
        this.viewport.style.transform = `translateY(${startIndex * this.renderHeight}px)`;
        this.viewport.innerHTML = html;

        // Re-attach toggle handlers for newly rendered comments
        attachReplyToggleHandlers(this.viewport);
    }
}

/**
 * Apply search, filter, and sort to comments
 */
function applyFilters(container) {
    const originalComments = JSON.parse(container.dataset.originalComments);
    const allComments = JSON.parse(container.dataset.allComments || '[]');
    const platform = container.dataset.platform;

    const searchQuery = document.getElementById('comment-search-input').value.toLowerCase();
    const sortBy = document.getElementById('comment-sort').value;
    const filterBy = document.getElementById('comment-filter').value;

    let filtered = [...originalComments];

    // When searching, also include replies that match (promote them to top level)
    if (searchQuery) {
        const matchesSearch = c =>
            (c.text || '').toLowerCase().includes(searchQuery) ||
            (c.author || '').toLowerCase().includes(searchQuery);

        // Include top-level comments that match, plus any replies that match
        const matchedIds = new Set();
        filtered = [];
        allComments.forEach(comment => {
            if (matchesSearch(comment)) {
                // Show as top-level even if it's a reply
                if (!matchedIds.has(comment.id)) {
                    filtered.push(comment);
                    matchedIds.add(comment.id);
                }
            }
        });

        // Rebuild childrenMap for filtered set - don't show reply threads for search results
        container.dataset.childrenMap = JSON.stringify({});
    } else {
        // Restore full childrenMap
        const { childrenMap } = buildCommentTree(allComments);
        allComments.forEach(comment => {
            if (comment.replies && comment.replies.length > 0) {
                const existing = childrenMap.get(comment.id) || [];
                const existingIds = new Set(existing.map(r => r.id));
                comment.replies.forEach(reply => {
                    if (!existingIds.has(reply.id)) existing.push(reply);
                });
                if (existing.length > 0) childrenMap.set(comment.id, existing);
            }
        });
        container.dataset.childrenMap = JSON.stringify(Object.fromEntries(childrenMap));
    }

    // Apply filter (on top-level / search results)
    switch (filterBy) {
        case 'pinned':
            filtered = filtered.filter(c => c.is_pinned);
            break;
        case 'creator':
            filtered = filtered.filter(c => c.is_creator);
            break;
        case 'popular':
            filtered = filtered.filter(c => c.likes >= 10);
            break;
    }

    // Apply sort
    switch (sortBy) {
        case 'likes-desc':
            filtered.sort((a, b) => b.likes - a.likes);
            break;
        case 'likes-asc':
            filtered.sort((a, b) => a.likes - b.likes);
            break;
        case 'newest':
            filtered.sort((a, b) => (b.timestamp || 0) - (a.timestamp || 0));
            break;
        case 'oldest':
            filtered.sort((a, b) => (a.timestamp || 0) - (b.timestamp || 0));
            break;
    }

    // Re-render with filtered results
    const headerHtml = container.querySelector('.comments-stats').outerHTML +
                       container.querySelector('.comments-controls').outerHTML;
    const tempContainer = container.cloneNode(false);
    // Copy dataset to temp container for childrenMap access
    tempContainer.dataset.childrenMap = container.dataset.childrenMap;
    renderCommentList(tempContainer, filtered, platform, headerHtml);

    // Replace only the comments list
    const oldList = container.querySelector('.comments-list, .virtual-scroll');
    const newList = tempContainer.querySelector('.comments-list, .virtual-scroll');
    if (oldList && newList) {
        oldList.replaceWith(newList);
    }

    // Re-attach toggle handlers
    attachReplyToggleHandlers(container);

    // Update count
    const summary = container.querySelector('.comments-summary');
    if (summary) {
        const total = JSON.parse(container.dataset.originalComments).length;
        summary.innerHTML = `Showing <strong>${filtered.length}</strong> of <strong>${total}</strong> top-level comments`;
    }
}

/**
 * Attach event handlers for controls
 */
function attachControlHandlers(container) {
    const searchInput = document.getElementById('comment-search-input');
    const searchClear = document.getElementById('comment-search-clear');
    const sortSelect = document.getElementById('comment-sort');
    const filterSelect = document.getElementById('comment-filter');

    // Debounced search
    let searchTimeout;
    searchInput.addEventListener('input', (e) => {
        clearTimeout(searchTimeout);
        searchTimeout = setTimeout(() => {
            applyFilters(container);
            searchClear.style.display = e.target.value ? 'inline-block' : 'none';
        }, 300);
    });

    searchClear.addEventListener('click', () => {
        searchInput.value = '';
        searchClear.style.display = 'none';
        applyFilters(container);
    });

    sortSelect.addEventListener('change', () => applyFilters(container));
    filterSelect.addEventListener('change', () => applyFilters(container));
}
