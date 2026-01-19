//! Discourse Link Archiver library.
//!
//! A service that monitors a Discourse forum RSS feed, archives linked content
//! to S3, and serves a public web UI for browsing archives.

pub mod archiver;
pub mod backup;
pub mod config;
pub mod db;
pub mod handlers;
pub mod ipfs;
pub mod rss;
pub mod s3;
pub mod wayback;
pub mod web;
