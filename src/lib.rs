//! Discourse Link Archiver library.
//!
//! A service that monitors a Discourse forum RSS feed, archives linked content
//! to S3, and serves a public web UI for browsing archives.

// Allow raw string hashes for safety - they're harmless and prevent issues if content changes
#![allow(clippy::needless_raw_string_hashes)]
// Allow pedantic warnings that are too noisy for this project
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::format_push_string)]
#![allow(clippy::unused_async)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::case_sensitive_file_extension_comparisons)]
#![allow(clippy::missing_fields_in_debug)]

pub mod archive_today;
pub mod archiver;
pub mod backup;
pub mod config;
pub mod constants;
pub mod db;
pub mod handlers;
pub mod ipfs;
pub mod rss;
pub mod s3;
pub mod tls;
pub mod wayback;
pub mod web;
