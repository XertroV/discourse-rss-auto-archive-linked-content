//! Content deduplication using perceptual hashing.
//!
//! This module provides perceptual hashing for images and video thumbnails
//! to detect duplicate content even when files have different encodings,
//! resolutions, or minor edits.

use anyhow::{anyhow, Context, Result};
use img_hash::{HashAlg, HasherConfig, ImageHash};
use tracing::debug;

/// Hasher configuration for perceptual hashing.
const HASH_SIZE: u32 = 16;

/// Compute a perceptual hash for an image.
///
/// Returns a hex-encoded hash string that can be compared with other hashes
/// to detect visually similar images.
///
/// # Errors
///
/// Returns an error if the image cannot be decoded.
pub fn compute_image_hash(data: &[u8]) -> Result<String> {
    // Use img_hash's re-exported image crate for compatibility
    let img = img_hash::image::load_from_memory(data).context("Failed to decode image")?;
    let hash = compute_hash_from_image(&img);
    Ok(hash.to_base64())
}

/// Compute a perceptual hash from a decoded image.
fn compute_hash_from_image(img: &img_hash::image::DynamicImage) -> ImageHash {
    let hasher = HasherConfig::new()
        .hash_size(HASH_SIZE, HASH_SIZE)
        .hash_alg(HashAlg::DoubleGradient)
        .to_hasher();

    hasher.hash_image(img)
}

/// Compare two perceptual hashes and return the Hamming distance.
///
/// Lower distance means more similar images. A distance of 0 means
/// the images are perceptually identical.
///
/// # Errors
///
/// Returns an error if the hashes cannot be parsed.
pub fn hash_distance(hash1: &str, hash2: &str) -> Result<u32> {
    let h1: ImageHash<Box<[u8]>> =
        ImageHash::from_base64(hash1).map_err(|e| anyhow!("Failed to parse first hash: {e:?}"))?;
    let h2: ImageHash<Box<[u8]>> =
        ImageHash::from_base64(hash2).map_err(|e| anyhow!("Failed to parse second hash: {e:?}"))?;
    Ok(h1.dist(&h2))
}

/// Check if two hashes represent similar images.
///
/// Uses a threshold to determine similarity. Images with a Hamming
/// distance below the threshold are considered duplicates.
pub fn are_similar(hash1: &str, hash2: &str, threshold: u32) -> bool {
    match hash_distance(hash1, hash2) {
        Ok(dist) => {
            debug!(distance = dist, threshold = threshold, "Hash comparison");
            dist <= threshold
        }
        Err(e) => {
            debug!(error = %e, "Failed to compare hashes");
            false
        }
    }
}

/// Default threshold for duplicate detection.
///
/// This threshold allows for minor variations like:
/// - Different compression levels
/// - Small resolution differences
/// - Minor color corrections
pub const DEFAULT_SIMILARITY_THRESHOLD: u32 = 10;

/// Check if an image is a duplicate based on default threshold.
pub fn is_duplicate(hash1: &str, hash2: &str) -> bool {
    are_similar(hash1, hash2, DEFAULT_SIMILARITY_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash() {
        // Create a simple test image (1x1 white pixel PNG)
        let white_pixel = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59,
            0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let hash = compute_image_hash(&white_pixel);
        assert!(hash.is_ok());
        let hash_str = hash.unwrap();
        assert!(!hash_str.is_empty());
    }

    #[test]
    fn test_identical_hashes() {
        // Same hash should have distance 0
        let hash = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let distance = hash_distance(hash, hash);
        assert!(distance.is_ok());
        assert_eq!(distance.unwrap(), 0);
    }

    #[test]
    fn test_is_duplicate() {
        // Identical hashes should be duplicates
        let hash = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        assert!(is_duplicate(hash, hash));
    }
}
