/**
 * Video volume persistence
 * Syncs video volume to localStorage and restores it on page load
 */

(function() {
    'use strict';

    var STORAGE_KEY = 'video-volume';
    var DEFAULT_VOLUME = 1.0;

    /**
     * Get saved volume from localStorage.
     * @returns {number} Volume between 0 and 1
     */
    function getSavedVolume() {
        try {
            var saved = localStorage.getItem(STORAGE_KEY);
            if (saved !== null) {
                var volume = parseFloat(saved);
                if (!isNaN(volume) && volume >= 0 && volume <= 1) {
                    return volume;
                }
            }
        } catch (e) {
            // localStorage not available
        }
        return DEFAULT_VOLUME;
    }

    /**
     * Save volume to localStorage.
     * @param {number} volume - Volume between 0 and 1
     */
    function saveVolume(volume) {
        try {
            localStorage.setItem(STORAGE_KEY, volume.toString());
        } catch (e) {
            // localStorage not available
        }
    }

    /**
     * Apply saved volume to a video element.
     * @param {HTMLVideoElement} video
     */
    function applyVolumeToVideo(video) {
        video.volume = getSavedVolume();
    }

    /**
     * Set up volume change listener on a video element.
     * @param {HTMLVideoElement} video
     */
    function setupVolumeListener(video) {
        video.addEventListener('volumechange', function() {
            saveVolume(video.volume);
        });
    }

    /**
     * Initialize volume sync for all videos on the page.
     */
    function initVideoVolume() {
        var videos = document.querySelectorAll('video');
        videos.forEach(function(video) {
            applyVolumeToVideo(video);
            setupVolumeListener(video);
        });

        // Watch for dynamically added videos
        var observer = new MutationObserver(function(mutations) {
            mutations.forEach(function(mutation) {
                mutation.addedNodes.forEach(function(node) {
                    if (node.nodeType === Node.ELEMENT_NODE) {
                        if (node.tagName === 'VIDEO') {
                            applyVolumeToVideo(node);
                            setupVolumeListener(node);
                        }
                        // Check for videos inside added elements
                        var nestedVideos = node.querySelectorAll && node.querySelectorAll('video');
                        if (nestedVideos) {
                            nestedVideos.forEach(function(video) {
                                applyVolumeToVideo(video);
                                setupVolumeListener(video);
                            });
                        }
                    }
                });
            });
        });

        observer.observe(document.body, {
            childList: true,
            subtree: true
        });
    }

    // Initialize when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initVideoVolume);
    } else {
        initVideoVolume();
    }
})();
