/**
 * Copy-to-clipboard functionality with visual feedback
 * Shows a floating indicator near the mouse cursor when URLs are copied
 */

(function() {
    'use strict';

    // Track current indicator to prevent duplicates
    var currentIndicator = null;

    /**
     * Truncate long URLs intelligently
     * Shows start and end portions with ellipsis in middle
     * @param {string} url - The URL to truncate
     * @param {number} maxLength - Maximum length (default 60)
     * @returns {string} Truncated URL
     */
    function truncateUrl(url, maxLength) {
        maxLength = maxLength || 60;
        if (url.length <= maxLength) return url;

        var start = Math.floor((maxLength - 3) / 2);
        var end = maxLength - 3 - start;
        return url.substring(0, start) + '...' + url.substring(url.length - end);
    }

    /**
     * Show copy indicator near the cursor
     * @param {string} url - The copied URL
     * @param {number} clientX - Mouse X position
     * @param {number} clientY - Mouse Y position
     */
    function showCopyIndicator(url, clientX, clientY) {
        // Remove existing indicator if present
        if (currentIndicator && currentIndicator.parentNode) {
            currentIndicator.remove();
        }

        var truncated = truncateUrl(url);
        var indicator = document.createElement('div');
        indicator.className = 'copy-indicator';
        indicator.textContent = 'copied: ' + truncated;

        // Position near cursor with offset to avoid blocking click target
        indicator.style.left = clientX + 'px';
        indicator.style.top = (clientY + 20) + 'px';

        document.body.appendChild(indicator);
        currentIndicator = indicator;

        // Trigger animation on next frame
        requestAnimationFrame(function() {
            indicator.classList.add('show');
        });

        // Auto-remove after display duration
        setTimeout(function() {
            indicator.classList.add('hide');
            indicator.classList.remove('show');

            // Remove from DOM after fade animation completes
            setTimeout(function() {
                if (indicator.parentNode) {
                    indicator.remove();
                }
                if (currentIndicator === indicator) {
                    currentIndicator = null;
                }
            }, 300);
        }, 2000);
    }

    /**
     * Handle clicks on copyable elements
     * Uses event delegation for efficiency
     * @param {Event} event - Click event
     */
    function handleCopyClick(event) {
        var target = event.target.closest('[data-copy-url]');
        if (!target) return;

        var url = target.getAttribute('data-copy-url');
        if (!url) return;

        // Copy to clipboard
        navigator.clipboard.writeText(url).then(function() {
            showCopyIndicator(url, event.clientX, event.clientY);
        }).catch(function(err) {
            console.error('Failed to copy URL:', err);
        });

        // Prevent default only for non-link elements to allow navigation
        // Links with data-copy-url will both copy and navigate
        if (target.tagName !== 'A') {
            event.preventDefault();
        }
    }

    /**
     * Initialize copy handler
     */
    function init() {
        document.body.addEventListener('click', handleCopyClick);
    }

    // Initialize when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
