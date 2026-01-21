//! Stats caching for Open Graph metadata generation.
//!
//! This module provides an in-memory cache for stats data used in social media
//! previews. The cache has a configurable TTL and refreshes automatically.

use std::sync::RwLock;
use std::time::{Duration, Instant};

use anyhow::Result;
use sqlx::SqlitePool;

use crate::db;

/// Cached statistics for OG metadata.
#[derive(Debug, Clone)]
pub struct CachedStats {
    /// Total number of completed archives
    pub total_archives: i64,
    /// Archive counts by content type
    pub content_type_counts: Vec<(String, i64)>,
    /// When this cache entry was created
    cached_at: Instant,
}

impl CachedStats {
    /// Check if this cache entry is still valid.
    pub fn is_valid(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() < ttl
    }

    /// Format content type breakdown as a human-readable string.
    pub fn format_breakdown(&self) -> String {
        let mut parts = Vec::new();

        for (content_type, count) in &self.content_type_counts {
            let type_name = match content_type.as_str() {
                "video" => "videos",
                "image" => "images",
                "text" => "text posts",
                "audio" => "audio clips",
                "gallery" => "galleries",
                "thread" => "threads",
                "playlist" => "playlists",
                "pdf" => "PDFs",
                "mixed" => "mixed media",
                _ => continue, // Skip unknown types
            };
            parts.push(format!("{} {}", count, type_name));
        }

        if parts.is_empty() {
            format!("{} archives preserved", self.total_archives)
        } else if parts.len() == 1 {
            format!("{} archives | {}", self.total_archives, parts[0])
        } else if parts.len() == 2 {
            format!(
                "{} archives | {} and {}",
                self.total_archives, parts[0], parts[1]
            )
        } else {
            // Show top 3 types
            let top_three = parts.iter().take(3).cloned().collect::<Vec<_>>();
            format!(
                "{} archives | {}",
                self.total_archives,
                top_three.join(", ")
            )
        }
    }
}

/// Global stats cache with TTL.
pub struct StatsCache {
    cache: RwLock<Option<CachedStats>>,
    ttl: Duration,
}

impl StatsCache {
    /// Create a new stats cache with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: RwLock::new(None),
            ttl,
        }
    }

    /// Get stats from cache or fetch fresh data if expired.
    pub async fn get_or_refresh(&self, pool: &SqlitePool) -> Result<CachedStats> {
        // Try to read from cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(ref stats) = *cache {
                if stats.is_valid(self.ttl) {
                    return Ok(stats.clone());
                }
            }
        }

        // Cache is expired or empty, fetch fresh data
        let fresh_stats = self.fetch_stats(pool).await?;

        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            *cache = Some(fresh_stats.clone());
        }

        Ok(fresh_stats)
    }

    /// Fetch fresh stats from database.
    async fn fetch_stats(&self, pool: &SqlitePool) -> Result<CachedStats> {
        let content_type_counts = db::count_archives_by_content_type(pool).await?;

        let total_archives: i64 = content_type_counts.iter().map(|(_, count)| count).sum();

        Ok(CachedStats {
            total_archives,
            content_type_counts,
            cached_at: Instant::now(),
        })
    }
}

impl Default for StatsCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(300)) // 5 minute TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_validity() {
        let stats = CachedStats {
            total_archives: 100,
            content_type_counts: vec![],
            cached_at: Instant::now(),
        };

        assert!(stats.is_valid(Duration::from_secs(60)));

        let old_stats = CachedStats {
            total_archives: 100,
            content_type_counts: vec![],
            cached_at: Instant::now() - Duration::from_secs(120),
        };

        assert!(!old_stats.is_valid(Duration::from_secs(60)));
    }

    #[test]
    fn test_format_breakdown_empty() {
        let stats = CachedStats {
            total_archives: 42,
            content_type_counts: vec![],
            cached_at: Instant::now(),
        };

        assert_eq!(stats.format_breakdown(), "42 archives preserved");
    }

    #[test]
    fn test_format_breakdown_single() {
        let stats = CachedStats {
            total_archives: 100,
            content_type_counts: vec![("video".to_string(), 100)],
            cached_at: Instant::now(),
        };

        assert_eq!(stats.format_breakdown(), "100 archives | 100 videos");
    }

    #[test]
    fn test_format_breakdown_multiple() {
        let stats = CachedStats {
            total_archives: 250,
            content_type_counts: vec![
                ("video".to_string(), 150),
                ("image".to_string(), 75),
                ("text".to_string(), 25),
            ],
            cached_at: Instant::now(),
        };

        let breakdown = stats.format_breakdown();
        assert!(breakdown.contains("250 archives"));
        assert!(breakdown.contains("150 videos"));
        assert!(breakdown.contains("75 images"));
    }

    #[test]
    fn test_format_breakdown_many_types() {
        let stats = CachedStats {
            total_archives: 500,
            content_type_counts: vec![
                ("video".to_string(), 200),
                ("image".to_string(), 150),
                ("text".to_string(), 100),
                ("audio".to_string(), 30),
                ("thread".to_string(), 20),
            ],
            cached_at: Instant::now(),
        };

        let breakdown = stats.format_breakdown();
        assert!(breakdown.contains("500 archives"));
        // Should show only top 3
        assert!(breakdown.contains("200 videos"));
        assert!(breakdown.contains("150 images"));
        assert!(breakdown.contains("100 text posts"));
    }

    #[test]
    fn test_stats_cache_creation() {
        let cache = StatsCache::new(Duration::from_secs(60));
        assert!(cache.cache.read().unwrap().is_none());
    }
}
