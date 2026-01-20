/**
 * NSFW content filter functionality
 * Handles showing/hiding NSFW-tagged content with localStorage persistence
 */

(function() {
    'use strict';

    /**
     * Check if NSFW content is currently enabled (visible).
     * @returns {boolean}
     */
    function getNsfwEnabled() {
        return localStorage.getItem('nsfw_enabled') === 'true';
    }

    /**
     * Count NSFW items on the current page.
     * @returns {number}
     */
    function countNsfwItems() {
        return document.querySelectorAll('article[data-nsfw="true"]').length;
    }

    /**
     * Update the NSFW toggle button tooltip with current state and count.
     * @param {HTMLElement} nsfwToggle - The toggle button element
     * @param {boolean} isEnabled - Whether NSFW content is currently visible
     */
    function updateNsfwTooltip(nsfwToggle, isEnabled) {
        var count = countNsfwItems();
        var countText = count === 0
            ? 'no NSFW items on this page'
            : (count === 1
                ? '1 NSFW item on this page'
                : count + ' NSFW items on this page');
        var actionText = isEnabled ? 'Hide NSFW items' : 'Show NSFW items';
        var label = actionText + ' (' + countText + ')';
        nsfwToggle.title = label;
        nsfwToggle.setAttribute('aria-label', label);
    }

    /**
     * Apply the NSFW visibility state to the page.
     * @param {HTMLElement} nsfwToggle - The toggle button element
     * @param {boolean} isEnabled - Whether NSFW content should be visible
     */
    function applyNsfwState(nsfwToggle, isEnabled) {
        if (isEnabled) {
            document.body.classList.remove('nsfw-hidden');
            nsfwToggle.classList.add('active');
        } else {
            document.body.classList.add('nsfw-hidden');
            nsfwToggle.classList.remove('active');
        }

        updateNsfwTooltip(nsfwToggle, isEnabled);
    }

    /**
     * Initialize NSFW toggle button functionality.
     * Called when DOM is ready.
     */
    function initNsfwToggle() {
        var nsfwToggle = document.getElementById('nsfw-toggle');
        if (!nsfwToggle) {
            return;
        }

        // Initialize button state and tooltip
        applyNsfwState(nsfwToggle, getNsfwEnabled());

        // Handle toggle clicks
        nsfwToggle.addEventListener('click', function() {
            var nextEnabled = !getNsfwEnabled();
            localStorage.setItem('nsfw_enabled', nextEnabled ? 'true' : 'false');
            applyNsfwState(nsfwToggle, nextEnabled);
        });

        // Update tooltip dynamically when page content changes
        var tooltipUpdateScheduled = false;
        var scheduleTooltipUpdate = function() {
            if (tooltipUpdateScheduled) {
                return;
            }
            tooltipUpdateScheduled = true;
            var scheduleFn = window.requestAnimationFrame || function(cb) { return window.setTimeout(cb, 0); };
            scheduleFn(function() {
                tooltipUpdateScheduled = false;
                updateNsfwTooltip(nsfwToggle, getNsfwEnabled());
            });
        };

        // Observe DOM changes to update the NSFW item count
        var nsfwObserver = new MutationObserver(function(mutationsList) {
            for (var i = 0; i < mutationsList.length; i++) {
                var mutation = mutationsList[i];
                if (mutation.type === 'childList' || mutation.type === 'attributes') {
                    scheduleTooltipUpdate();
                    break;
                }
            }
        });

        nsfwObserver.observe(document.body, {
            childList: true,
            subtree: true,
            attributes: true,
            attributeFilter: ['data-nsfw']
        });
    }

    // Initialize when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initNsfwToggle);
    } else {
        initNsfwToggle();
    }
})();
