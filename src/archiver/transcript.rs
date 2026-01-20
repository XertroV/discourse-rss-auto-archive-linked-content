use std::path::Path;

use anyhow::{Context, Result};
use tracing::debug;

/// Represents a single subtitle cue/entry.
#[derive(Debug, Clone)]
pub struct SubtitleCue {
    /// Start time in seconds
    pub start_time: f64,
    /// End time in seconds
    pub end_time: f64,
    /// Text content
    pub text: String,
}

/// Parse a WebVTT (.vtt) subtitle file.
pub async fn parse_vtt(path: &Path) -> Result<Vec<SubtitleCue>> {
    let content = tokio::fs::read_to_string(path)
        .await
        .context("Failed to read VTT file")?;

    let mut cues = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    // Skip WEBVTT header and metadata
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            break;
        }
        if line.starts_with("WEBVTT") || line.contains("-->") {
            i += 1;
            continue;
        }
        i += 1;
    }

    // Parse cues
    while i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines and cue identifiers (numbers or identifiers)
        if line.is_empty() {
            i += 1;
            continue;
        }

        // Check if this is a timestamp line
        if line.contains("-->") {
            if let Some((start, end)) = parse_timestamp_line(line) {
                // Collect text lines until we hit an empty line
                let mut text_lines = Vec::new();
                i += 1;

                while i < lines.len() {
                    let text_line = lines[i].trim();
                    if text_line.is_empty() {
                        break;
                    }
                    // Remove VTT tags like <c>, <v>, etc.
                    let cleaned = remove_vtt_tags(text_line);
                    if !cleaned.is_empty() {
                        text_lines.push(cleaned);
                    }
                    i += 1;
                }

                if !text_lines.is_empty() {
                    cues.push(SubtitleCue {
                        start_time: start,
                        end_time: end,
                        text: text_lines.join(" "),
                    });
                }
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    debug!(path = %path.display(), cue_count = cues.len(), "Parsed VTT file");
    Ok(cues)
}

/// Parse an SRT (.srt) subtitle file.
pub async fn parse_srt(path: &Path) -> Result<Vec<SubtitleCue>> {
    let content = tokio::fs::read_to_string(path)
        .await
        .context("Failed to read SRT file")?;

    let mut cues = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines
        if line.is_empty() {
            i += 1;
            continue;
        }

        // Skip sequence number (should be a number)
        if line.chars().all(|c| c.is_ascii_digit()) {
            i += 1;
            if i >= lines.len() {
                break;
            }
            let timestamp_line = lines[i].trim();

            // Parse timestamp line
            if let Some((start, end)) = parse_srt_timestamp_line(timestamp_line) {
                // Collect text lines
                let mut text_lines = Vec::new();
                i += 1;

                while i < lines.len() {
                    let text_line = lines[i].trim();
                    if text_line.is_empty() {
                        break;
                    }
                    // Remove SRT tags like <i>, <b>, etc.
                    let cleaned = remove_html_tags(text_line);
                    if !cleaned.is_empty() {
                        text_lines.push(cleaned);
                    }
                    i += 1;
                }

                if !text_lines.is_empty() {
                    cues.push(SubtitleCue {
                        start_time: start,
                        end_time: end,
                        text: text_lines.join(" "),
                    });
                }
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    debug!(path = %path.display(), cue_count = cues.len(), "Parsed SRT file");
    Ok(cues)
}

/// Generate a readable transcript from subtitle cues.
///
/// Groups cues by time intervals and formats them with timestamps.
pub fn generate_transcript(cues: &[SubtitleCue]) -> String {
    if cues.is_empty() {
        return String::new();
    }

    let mut transcript = String::new();
    let mut last_timestamp = -60.0; // Force first timestamp to show

    for cue in cues {
        // Show timestamp every 30 seconds or at the start
        if cue.start_time - last_timestamp >= 30.0 {
            let timestamp = format_timestamp(cue.start_time);
            transcript.push_str(&format!("\n[{}]\n", timestamp));
            last_timestamp = cue.start_time;
        }

        transcript.push_str(&cue.text);
        transcript.push(' ');
    }

    transcript.trim().to_string()
}

/// Parse a VTT timestamp line like "00:00:10.500 --> 00:00:13.200".
///
/// Returns (start_time, end_time) in seconds.
fn parse_timestamp_line(line: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = line.split("-->").map(str::trim).collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parse_vtt_timestamp(parts[0])?;
    let end = parse_vtt_timestamp(parts[1])?;

    Some((start, end))
}

/// Parse a VTT timestamp like "00:00:10.500" to seconds.
fn parse_vtt_timestamp(timestamp: &str) -> Option<f64> {
    // VTT format: HH:MM:SS.mmm or MM:SS.mmm
    let parts: Vec<&str> = timestamp.split(':').collect();

    match parts.len() {
        2 => {
            // MM:SS.mmm
            let minutes: f64 = parts[0].parse().ok()?;
            let seconds: f64 = parts[1].parse().ok()?;
            Some(minutes * 60.0 + seconds)
        }
        3 => {
            // HH:MM:SS.mmm
            let hours: f64 = parts[0].parse().ok()?;
            let minutes: f64 = parts[1].parse().ok()?;
            let seconds: f64 = parts[2].parse().ok()?;
            Some(hours * 3600.0 + minutes * 60.0 + seconds)
        }
        _ => None,
    }
}

/// Parse an SRT timestamp line like "00:00:10,500 --> 00:00:13,200".
///
/// Returns (start_time, end_time) in seconds.
fn parse_srt_timestamp_line(line: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = line.split("-->").map(str::trim).collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parse_srt_timestamp(parts[0])?;
    let end = parse_srt_timestamp(parts[1])?;

    Some((start, end))
}

/// Parse an SRT timestamp like "00:00:10,500" to seconds.
fn parse_srt_timestamp(timestamp: &str) -> Option<f64> {
    // SRT format: HH:MM:SS,mmm (note comma instead of dot)
    let timestamp = timestamp.replace(',', ".");
    let parts: Vec<&str> = timestamp.split(':').collect();

    if parts.len() != 3 {
        return None;
    }

    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;

    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// Format seconds into a readable timestamp like "1:23:45".
fn format_timestamp(seconds: f64) -> String {
    let total_secs = seconds as i64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{}:{:02}", minutes, secs)
    }
}

/// Remove VTT tags like <c>, <v Speaker>, etc.
fn remove_vtt_tags(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result.trim().to_string()
}

/// Remove HTML tags like <i>, <b>, <font>, etc.
fn remove_html_tags(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result.trim().to_string()
}

/// Build a transcript from a subtitle file (VTT or SRT).
///
/// Returns the transcript as a String.
pub async fn build_transcript_from_file(path: &Path) -> Result<String> {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let cues = if filename.ends_with(".vtt") {
        parse_vtt(path).await?
    } else if filename.ends_with(".srt") {
        parse_srt(path).await?
    } else {
        anyhow::bail!("Unsupported subtitle format: {}", filename);
    };

    Ok(generate_transcript(&cues))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vtt_timestamp() {
        assert_eq!(parse_vtt_timestamp("00:00:10.500"), Some(10.5));
        assert_eq!(parse_vtt_timestamp("00:01:30.250"), Some(90.25));
        assert_eq!(parse_vtt_timestamp("01:23:45.000"), Some(5025.0));
        assert_eq!(parse_vtt_timestamp("10.500"), Some(10.5));
    }

    #[test]
    fn test_parse_srt_timestamp() {
        assert_eq!(parse_srt_timestamp("00:00:10,500"), Some(10.5));
        assert_eq!(parse_srt_timestamp("00:01:30,250"), Some(90.25));
        assert_eq!(parse_srt_timestamp("01:23:45,000"), Some(5025.0));
    }

    #[test]
    fn test_format_timestamp() {
        assert_eq!(format_timestamp(10.5), "0:10");
        assert_eq!(format_timestamp(90.25), "1:30");
        assert_eq!(format_timestamp(5025.0), "1:23:45");
    }

    #[test]
    fn test_remove_vtt_tags() {
        assert_eq!(remove_vtt_tags("<c>Hello</c> world"), "Hello world");
        assert_eq!(remove_vtt_tags("<v Speaker>Hello"), "Hello");
        assert_eq!(remove_vtt_tags("No tags here"), "No tags here");
    }

    #[test]
    fn test_remove_html_tags() {
        assert_eq!(remove_html_tags("<i>Hello</i> world"), "Hello world");
        assert_eq!(remove_html_tags("<b>Bold</b> text"), "Bold text");
        assert_eq!(remove_html_tags("No tags"), "No tags");
    }

    #[test]
    fn test_generate_transcript() {
        let cues = vec![
            SubtitleCue {
                start_time: 0.0,
                end_time: 2.0,
                text: "Hello".to_string(),
            },
            SubtitleCue {
                start_time: 2.0,
                end_time: 4.0,
                text: "world".to_string(),
            },
            SubtitleCue {
                start_time: 35.0,
                end_time: 37.0,
                text: "Next section".to_string(),
            },
        ];

        let transcript = generate_transcript(&cues);
        assert!(transcript.contains("Hello world"));
        assert!(transcript.contains("[0:35]"));
        assert!(transcript.contains("Next section"));
    }
}
