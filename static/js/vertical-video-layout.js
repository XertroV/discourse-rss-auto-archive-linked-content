/**
 * Vertical video layout detection
 * Detects vertical videos and enables side-by-side layout with transcript
 */

(function() {
    'use strict';

    /**
     * Check if video has vertical aspect ratio (portrait orientation).
     * @param {HTMLVideoElement} video
     * @returns {boolean}
     */
    function isVerticalVideo(video) {
        return video.videoWidth > 0 &&
               video.videoHeight > 0 &&
               video.videoWidth < video.videoHeight;
    }

    /**
     * Apply vertical layout class to container if video is vertical.
     * Also sets CSS variable with the media section's rendered height.
     * @param {HTMLElement} container
     * @param {HTMLVideoElement} video
     */
    function applyLayoutIfVertical(container, video) {
        if (isVerticalVideo(video)) {
            container.classList.add('vertical-layout');
            console.log('[VerticalLayout] Vertical video detected:',
                        video.videoWidth + 'x' + video.videoHeight);

            // Wait for next frame to get accurate rendered height
            requestAnimationFrame(function() {
                // Measure the entire media section, not just the video
                var mediaSection = container.querySelector('.media-column section');
                var sectionHeight = 0;

                if (mediaSection) {
                    sectionHeight = mediaSection.offsetHeight;
                    container.style.setProperty('--video-rendered-height', sectionHeight + 'px');
                    console.log('[VerticalLayout] Media section rendered height:', sectionHeight + 'px');
                } else {
                    // Fallback to video height
                    sectionHeight = video.offsetHeight;
                    container.style.setProperty('--video-rendered-height', sectionHeight + 'px');
                    console.log('[VerticalLayout] Video rendered height:', sectionHeight + 'px');
                }

                // Set transcript content height to 70% of video height using CSS calc
                if (sectionHeight > 0) {
                    // Use CSS calc() to compute 70% of --video-rendered-height
                    container.style.setProperty('--max-transcript-content-height', 'calc(var(--video-rendered-height) * 0.7)');
                    console.log('[VerticalLayout] Set transcript content to calc(var(--video-rendered-height) * 0.7)');
                }
            });
        } else {
            container.classList.remove('vertical-layout');
            container.style.removeProperty('--video-rendered-height');
            container.style.removeProperty('--max-transcript-content-height');
            console.log('[VerticalLayout] Horizontal video, using stacked layout:',
                        video.videoWidth + 'x' + video.videoHeight);
        }
    }

    /**
     * Set up vertical layout detection for a container.
     * @param {HTMLElement} container
     */
    function setupVerticalLayoutDetection(container) {
        // Find video element within the media column
        var video = container.querySelector('.media-column video');

        if (!video) {
            console.log('[VerticalLayout] No video found in container');
            return;
        }

        // Check if metadata is already loaded
        if (video.readyState >= 1) { // HAVE_METADATA or higher
            applyLayoutIfVertical(container, video);
        } else {
            // Wait for metadata to load
            video.addEventListener('loadedmetadata', function() {
                applyLayoutIfVertical(container, video);
            }, { once: true });
        }

        // Handle video source changes (edge case)
        video.addEventListener('loadeddata', function() {
            applyLayoutIfVertical(container, video);
        });
    }

    /**
     * Initialize vertical layout detection on page load.
     */
    function initVerticalLayout() {
        var container = document.getElementById('media-transcript-container');

        if (!container) {
            // No container on this page
            return;
        }

        // Check if this container is marked as a candidate for vertical layout
        if (container.getAttribute('data-vertical-layout-candidate') !== 'true') {
            return;
        }

        setupVerticalLayoutDetection(container);
    }

    // Initialize when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initVerticalLayout);
    } else {
        initVerticalLayout();
    }
})();
