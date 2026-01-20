/**
 * Interactive transcript functionality
 * Provides search, highlighting, and video navigation features
 */

document.addEventListener('DOMContentLoaded', function() {
    initializeTranscriptInteractivity();
});

function initializeTranscriptInteractivity() {
    const searchBox = document.getElementById('transcript-search');
    const transcriptContent = document.getElementById('transcript-content');
    const matchCounter = document.getElementById('match-counter');

    if (!searchBox || !transcriptContent) {
        return; // Not on a page with transcript
    }

    // Search functionality
    let searchTimeout;
    searchBox.addEventListener('input', function() {
        clearTimeout(searchTimeout);
        searchTimeout = setTimeout(function() {
            performSearch(searchBox.value, transcriptContent, matchCounter);
        }, 300);
    });

    // Make timestamps clickable for video navigation
    makeTimestampsClickable(transcriptContent);
}

function performSearch(query, transcriptContent, matchCounter) {
    // Remove previous highlights
    const originalText = transcriptContent.getAttribute('data-original-text') || transcriptContent.textContent;

    if (!transcriptContent.hasAttribute('data-original-text')) {
        transcriptContent.setAttribute('data-original-text', originalText);
    }

    if (!query.trim()) {
        // No search query - restore original with clickable timestamps
        transcriptContent.innerHTML = makeTimestampsClickableInHTML(originalText);
        if (matchCounter) {
            matchCounter.textContent = '';
        }
        return;
    }

    // Perform case-insensitive search and highlight
    const regex = new RegExp('(' + escapeRegex(query) + ')', 'gi');
    let matchCount = 0;
    const highlighted = originalText.replace(regex, function(match) {
        matchCount++;
        return '<mark class="highlight">' + escapeHtml(match) + '</mark>';
    });

    transcriptContent.innerHTML = makeTimestampsClickableInHTML(highlighted);

    if (matchCounter) {
        if (matchCount > 0) {
            matchCounter.textContent = matchCount + ' match' + (matchCount !== 1 ? 'es' : '');
            matchCounter.style.color = '#059669'; // green
        } else {
            matchCounter.textContent = 'No matches';
            matchCounter.style.color = '#dc2626'; // red
        }
    }
}

function makeTimestampsClickable(container) {
    const text = container.textContent;
    const html = makeTimestampsClickableInHTML(text);
    container.innerHTML = html;
}

function makeTimestampsClickableInHTML(text) {
    // Match timestamps like [1:23] or [1:23:45]
    const timestampRegex = /\[(\d{1,2}):(\d{2})(?::(\d{2}))?\]/g;

    return text.replace(timestampRegex, function(match, h_or_m, m_or_s, s) {
        let seconds;
        if (s !== undefined) {
            // Format: [H:MM:SS]
            seconds = parseInt(h_or_m) * 3600 + parseInt(m_or_s) * 60 + parseInt(s);
        } else {
            // Format: [M:SS]
            seconds = parseInt(h_or_m) * 60 + parseInt(m_or_s);
        }

        return '<a href="#" class="timestamp-link" data-seconds="' + seconds +
               '" onclick="seekVideo(' + seconds + '); return false;">' +
               escapeHtml(match) + '</a>';
    });
}

function seekVideo(seconds) {
    // Find video element on page
    const video = document.querySelector('video');

    if (video) {
        video.currentTime = seconds;
        video.play();

        // Scroll video into view
        video.scrollIntoView({ behavior: 'smooth', block: 'center' });

        // Flash effect to indicate seeking
        video.style.outline = '3px solid #3b82f6';
        setTimeout(function() {
            video.style.outline = '';
        }, 1000);
    } else {
        alert('No video found on page. Timestamp: ' + formatTime(seconds));
    }
}

function formatTime(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = Math.floor(seconds % 60);

    if (h > 0) {
        return h + ':' + pad(m) + ':' + pad(s);
    } else {
        return m + ':' + pad(s);
    }
}

function pad(num) {
    return num < 10 ? '0' + num : '' + num;
}

function escapeRegex(str) {
    return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}
