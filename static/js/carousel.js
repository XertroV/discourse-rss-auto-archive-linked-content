/**
 * Carousel navigation functionality
 * Handles arrow button navigation, thumbnail clicks, and keyboard controls
 */

(function() {
    'use strict';

    /**
     * CarouselController manages a single carousel instance.
     * @param {HTMLElement} container - The carousel container element
     */
    function CarouselController(container) {
        this.container = container;
        this.track = container.querySelector('.carousel-track');
        this.items = Array.from(container.querySelectorAll('.carousel-item'));
        this.thumbnails = Array.from(container.querySelectorAll('.carousel-thumb'));
        this.prevBtn = container.querySelector('.carousel-nav-prev');
        this.nextBtn = container.querySelector('.carousel-nav-next');
        this.counter = container.querySelector('.carousel-counter');
        this.currentSpan = this.counter ? this.counter.querySelector('.carousel-current') : null;

        this.currentIndex = 0;
        this.totalImages = this.items.length;

        this.init();
    }

    CarouselController.prototype.init = function() {
        var self = this;

        // Navigation button handlers
        if (this.prevBtn) {
            this.prevBtn.addEventListener('click', function() {
                self.navigate(-1);
            });
        }

        if (this.nextBtn) {
            this.nextBtn.addEventListener('click', function() {
                self.navigate(1);
            });
        }

        // Thumbnail click handlers
        this.thumbnails.forEach(function(thumb, index) {
            thumb.addEventListener('click', function() {
                self.goToIndex(index);
            });
        });

        // Keyboard navigation (when carousel is focused)
        this.container.addEventListener('keydown', function(e) {
            if (e.key === 'ArrowLeft') {
                e.preventDefault();
                self.navigate(-1);
            } else if (e.key === 'ArrowRight') {
                e.preventDefault();
                self.navigate(1);
            }
        });

        // Sync scroll position with active state
        this.track.addEventListener('scroll', function() {
            self.updateIndexFromScroll();
        });

        // Initial state
        this.updateUI();
    };

    /**
     * Navigate by relative offset (-1 for previous, +1 for next).
     * @param {number} direction - Navigation direction
     */
    CarouselController.prototype.navigate = function(direction) {
        var newIndex = this.currentIndex + direction;
        if (newIndex >= 0 && newIndex < this.totalImages) {
            this.goToIndex(newIndex);
        }
    };

    /**
     * Jump to specific image index.
     * @param {number} index - Target image index
     */
    CarouselController.prototype.goToIndex = function(index) {
        if (index < 0 || index >= this.totalImages) {
            return;
        }

        this.currentIndex = index;

        // Scroll to item
        var targetItem = this.items[index];
        if (targetItem) {
            targetItem.scrollIntoView({
                behavior: 'smooth',
                block: 'nearest',
                inline: 'start'
            });
        }

        this.updateUI();
    };

    /**
     * Update current index based on scroll position.
     * Uses intersection observation approach for accuracy.
     */
    CarouselController.prototype.updateIndexFromScroll = function() {
        var self = this;
        var trackRect = this.track.getBoundingClientRect();
        var centerX = trackRect.left + (trackRect.width / 2);

        // Find which item is currently centered
        var closestIndex = 0;
        var closestDistance = Infinity;

        this.items.forEach(function(item, index) {
            var itemRect = item.getBoundingClientRect();
            var itemCenterX = itemRect.left + (itemRect.width / 2);
            var distance = Math.abs(centerX - itemCenterX);

            if (distance < closestDistance) {
                closestDistance = distance;
                closestIndex = index;
            }
        });

        if (this.currentIndex !== closestIndex) {
            this.currentIndex = closestIndex;
            this.updateUI();
        }
    };

    /**
     * Update UI to reflect current index.
     */
    CarouselController.prototype.updateUI = function() {
        // Update counter
        if (this.currentSpan) {
            this.currentSpan.textContent = this.currentIndex + 1;
        }

        // Update navigation button states
        if (this.prevBtn) {
            this.prevBtn.disabled = this.currentIndex === 0;
        }
        if (this.nextBtn) {
            this.nextBtn.disabled = this.currentIndex === this.totalImages - 1;
        }

        // Update thumbnail active states
        this.thumbnails.forEach(function(thumb, index) {
            if (index === this.currentIndex) {
                thumb.classList.add('active');
                thumb.setAttribute('aria-current', 'true');
            } else {
                thumb.classList.remove('active');
                thumb.removeAttribute('aria-current');
            }
        }, this);

        // Scroll active thumbnail into view
        var activeThumb = this.thumbnails[this.currentIndex];
        if (activeThumb) {
            activeThumb.scrollIntoView({
                behavior: 'smooth',
                block: 'nearest',
                inline: 'center'
            });
        }
    };

    /**
     * Initialize all carousels on the page.
     */
    function initCarousels() {
        var carousels = document.querySelectorAll('.carousel');
        carousels.forEach(function(carousel) {
            new CarouselController(carousel);
        });
    }

    // Initialize when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initCarousels);
    } else {
        initCarousels();
    }

    // Re-initialize if new carousels are added dynamically
    var observer = new MutationObserver(function(mutations) {
        mutations.forEach(function(mutation) {
            mutation.addedNodes.forEach(function(node) {
                if (node.nodeType === Node.ELEMENT_NODE) {
                    if (node.classList && node.classList.contains('carousel')) {
                        new CarouselController(node);
                    }
                    // Check for carousels inside added elements
                    var nestedCarousels = node.querySelectorAll && node.querySelectorAll('.carousel');
                    if (nestedCarousels) {
                        nestedCarousels.forEach(function(carousel) {
                            new CarouselController(carousel);
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
})();
