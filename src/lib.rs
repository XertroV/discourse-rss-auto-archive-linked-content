//! Discourse Link Archiver library.
//!
//! A service that monitors a Discourse forum RSS feed, archives linked content
//! to S3, and serves a public web UI for browsing archives.

// Allow raw string hashes for safety - they're harmless and prevent issues if content changes
#![allow(clippy::needless_raw_string_hashes)]

pub mod archive_today;
pub mod archiver;
pub mod backup;
pub mod config;
pub mod db;
pub mod handlers;
pub mod ipfs;
pub mod rss;
pub mod s3;
pub mod tls;
pub mod wayback;
pub mod web;
