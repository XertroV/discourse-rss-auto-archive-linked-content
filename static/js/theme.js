/**
 * Theme toggle functionality
 * Handles light/dark mode switching with localStorage persistence
 */

(function() {
    'use strict';

    /**
     * Initialize theme toggle button functionality.
     * Called when DOM is ready.
     */
    function initThemeToggle() {
        var themeToggle = document.getElementById('theme-toggle');
        if (!themeToggle) {
            return;
        }

        themeToggle.addEventListener('click', function() {
            var html = document.documentElement;
            var current = html.getAttribute('data-theme');
            var next = (current === 'dark') ? 'light' : 'dark';
            html.setAttribute('data-theme', next);
            localStorage.setItem('theme', next);
        });
    }

    // Initialize when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initThemeToggle);
    } else {
        initThemeToggle();
    }
})();
