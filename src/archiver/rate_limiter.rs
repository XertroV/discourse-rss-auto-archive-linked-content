//! Per-domain rate limiting for archive workers.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{RwLock, Semaphore};

/// Per-domain rate limiter using semaphores.
///
/// Ensures that at most `per_domain_concurrency` archive jobs run concurrently
/// for any given domain. This prevents overwhelming individual sites.
#[derive(Debug)]
pub struct DomainRateLimiter {
    per_domain_limit: usize,
    semaphores: RwLock<HashMap<String, Arc<Semaphore>>>,
}

impl DomainRateLimiter {
    /// Create a new domain rate limiter.
    ///
    /// # Arguments
    ///
    /// * `per_domain_limit` - Maximum concurrent requests per domain
    #[must_use]
    pub fn new(per_domain_limit: usize) -> Self {
        Self {
            per_domain_limit,
            semaphores: RwLock::new(HashMap::new()),
        }
    }

    /// Acquire a permit for the given domain.
    ///
    /// This will block until a permit is available for the domain.
    pub async fn acquire(&self, domain: &str) -> DomainPermit {
        let semaphore = self.get_or_create_semaphore(domain).await;
        // Use acquire_owned to get an owned permit that's not tied to the semaphore reference
        let permit = semaphore
            .acquire_owned()
            .await
            .expect("Semaphore closed unexpectedly");

        DomainPermit {
            domain: domain.to_string(),
            _permit: permit,
        }
    }

    /// Try to acquire a permit for the given domain without blocking.
    ///
    /// Returns `None` if no permit is immediately available.
    #[allow(dead_code)]
    pub async fn try_acquire(&self, domain: &str) -> Option<DomainPermit> {
        let semaphore = self.get_or_create_semaphore(domain).await;
        semaphore.try_acquire_owned().ok().map(|permit| DomainPermit {
            domain: domain.to_string(),
            _permit: permit,
        })
    }

    async fn get_or_create_semaphore(&self, domain: &str) -> Arc<Semaphore> {
        // Fast path: check if semaphore exists
        {
            let read_guard = self.semaphores.read().await;
            if let Some(sem) = read_guard.get(domain) {
                return Arc::clone(sem);
            }
        }

        // Slow path: create semaphore
        let mut write_guard = self.semaphores.write().await;
        // Double-check pattern to avoid race condition
        if let Some(sem) = write_guard.get(domain) {
            return Arc::clone(sem);
        }

        let semaphore = Arc::new(Semaphore::new(self.per_domain_limit));
        write_guard.insert(domain.to_string(), Arc::clone(&semaphore));
        semaphore
    }

    /// Get the number of domains currently being tracked.
    #[allow(dead_code)]
    pub async fn domain_count(&self) -> usize {
        self.semaphores.read().await.len()
    }
}

/// A permit to make requests to a specific domain.
///
/// The permit is automatically released when dropped.
#[derive(Debug)]
pub struct DomainPermit {
    #[allow(dead_code)]
    domain: String,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_permit() {
        let limiter = DomainRateLimiter::new(2);

        let permit1 = limiter.acquire("example.com").await;
        let permit2 = limiter.acquire("example.com").await;

        // Should have both permits
        assert_eq!(limiter.domain_count().await, 1);

        drop(permit1);
        drop(permit2);
    }

    #[tokio::test]
    async fn test_try_acquire_exhausted() {
        let limiter = DomainRateLimiter::new(1);

        let _permit1 = limiter.acquire("example.com").await;

        // Second permit should fail with try_acquire
        assert!(limiter.try_acquire("example.com").await.is_none());

        // Different domain should succeed
        let permit2 = limiter.try_acquire("other.com").await;
        assert!(permit2.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_domains() {
        let limiter = DomainRateLimiter::new(1);

        let _permit1 = limiter.acquire("domain1.com").await;
        let _permit2 = limiter.acquire("domain2.com").await;
        let _permit3 = limiter.acquire("domain3.com").await;

        assert_eq!(limiter.domain_count().await, 3);
    }

    #[tokio::test]
    async fn test_permit_release() {
        let limiter = DomainRateLimiter::new(1);

        {
            let _permit = limiter.acquire("example.com").await;
            assert!(limiter.try_acquire("example.com").await.is_none());
        }

        // Permit released, should be able to acquire again
        let permit = limiter.try_acquire("example.com").await;
        assert!(permit.is_some());
    }
}
