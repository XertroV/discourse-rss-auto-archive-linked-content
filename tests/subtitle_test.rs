//! Integration tests for subtitle and transcript functionality.

use discourse_link_archiver::archiver::transcript::{
    build_transcript_from_file, generate_transcript, parse_srt, parse_vtt, parse_vtt_content,
    SubtitleCue,
};
use discourse_link_archiver::archiver::ytdlp::parse_subtitle_info;
use discourse_link_archiver::db::{
    create_pending_archive, get_artifacts_for_archive, insert_artifact_with_metadata, insert_link,
    Database, NewLink,
};
use std::path::Path;
use tempfile::TempDir;
use tokio::fs;

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.sqlite");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

/// Create a sample VTT file for testing.
async fn create_test_vtt(path: &Path) {
    let vtt_content = r#"WEBVTT

00:00:00.000 --> 00:00:02.500
Hello, this is a test video.

00:00:02.500 --> 00:00:05.000
We are testing subtitle parsing.

00:00:35.000 --> 00:00:37.500
This should trigger a new timestamp section.
"#;
    fs::write(path, vtt_content)
        .await
        .expect("Failed to write VTT file");
}

/// Create a sample SRT file for testing.
async fn create_test_srt(path: &Path) {
    let srt_content = r#"1
00:00:00,000 --> 00:00:02,500
Hello from SRT format.

2
00:00:02,500 --> 00:00:05,000
<i>Testing italic tags</i>

3
00:00:35,000 --> 00:00:37,500
Another timestamp section here.
"#;
    fs::write(path, srt_content)
        .await
        .expect("Failed to write SRT file");
}

#[tokio::test]
async fn test_parse_vtt_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let vtt_path = temp_dir.path().join("test.vtt");
    create_test_vtt(&vtt_path).await;

    let cues = parse_vtt(&vtt_path)
        .await
        .expect("Failed to parse VTT file");

    assert_eq!(cues.len(), 3, "Should have 3 subtitle cues");
    assert_eq!(cues[0].start_time, 0.0);
    assert_eq!(cues[0].end_time, 2.5);
    assert!(cues[0].text.contains("Hello"));

    assert_eq!(cues[2].start_time, 35.0);
    assert!(cues[2].text.contains("timestamp section"));
}

#[tokio::test]
async fn test_parse_srt_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let srt_path = temp_dir.path().join("test.srt");
    create_test_srt(&srt_path).await;

    let cues = parse_srt(&srt_path)
        .await
        .expect("Failed to parse SRT file");

    assert_eq!(cues.len(), 3, "Should have 3 subtitle cues");
    assert_eq!(cues[0].start_time, 0.0);
    assert_eq!(cues[0].end_time, 2.5);
    assert!(cues[0].text.contains("Hello from SRT"));

    // HTML tags should be removed
    assert!(cues[1].text.contains("Testing italic tags"));
    assert!(!cues[1].text.contains("<i>"));
}

#[tokio::test]
async fn test_generate_transcript() {
    let cues = vec![
        SubtitleCue {
            start_time: 0.0,
            end_time: 2.0,
            text: "First sentence.".to_string(),
        },
        SubtitleCue {
            start_time: 2.0,
            end_time: 4.0,
            text: "Second sentence.".to_string(),
        },
        SubtitleCue {
            start_time: 35.0,
            end_time: 37.0,
            text: "After 30 seconds.".to_string(),
        },
    ];

    let transcript = generate_transcript(&cues);

    // Should contain all text
    assert!(transcript.contains("First sentence"));
    assert!(transcript.contains("Second sentence"));
    assert!(transcript.contains("After 30 seconds"));

    // Should have timestamp for 35 second mark (30+ seconds from start)
    assert!(transcript.contains("[0:35]"));
}

#[tokio::test]
async fn test_build_transcript_from_vtt_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let vtt_path = temp_dir.path().join("test.vtt");
    create_test_vtt(&vtt_path).await;

    let transcript = build_transcript_from_file(&vtt_path)
        .await
        .expect("Failed to build transcript");

    assert!(!transcript.is_empty());
    assert!(transcript.contains("Hello"));
    assert!(transcript.contains("subtitle parsing"));
    assert!(transcript.contains("[0:35]")); // Timestamp marker
}

#[tokio::test]
async fn test_build_transcript_from_srt_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let srt_path = temp_dir.path().join("test.srt");
    create_test_srt(&srt_path).await;

    let transcript = build_transcript_from_file(&srt_path)
        .await
        .expect("Failed to build transcript");

    assert!(!transcript.is_empty());
    assert!(transcript.contains("Hello from SRT"));
    assert!(transcript.contains("Testing italic tags"));
}

#[tokio::test]
async fn test_parse_subtitle_info() {
    // Test English manual subtitle (2 parts: video.en)
    let (lang, is_auto, format) = parse_subtitle_info("video.en.vtt");
    assert_eq!(lang, "en");
    assert!(!is_auto);
    assert_eq!(format, "vtt");

    // Test pattern with -auto suffix (parts.len() = 2, so is_auto won't be detected)
    let (lang, is_auto, format) = parse_subtitle_info("video.en-auto.srt");
    assert_eq!(lang, "en-auto");
    // Note: Current implementation requires parts.len() > 2 for is_auto detection
    // This case has only 2 parts, so is_auto = false
    assert!(!is_auto);
    assert_eq!(format, "srt");

    // Test auto-generated pattern that would be detected (3+ parts with "auto" in middle)
    let (lang, is_auto, format) = parse_subtitle_info("title.auto.en.vtt");
    assert_eq!(lang, "en");
    assert!(is_auto); // parts.len() > 2 and parts[parts.len()-2] = "auto"
    assert_eq!(format, "vtt");

    // Test other language
    let (lang, is_auto, format) = parse_subtitle_info("video.es.vtt");
    assert_eq!(lang, "es");
    assert!(!is_auto);
    assert_eq!(format, "vtt");

    // Test file with no recognized subtitle extension
    // Since .txt isn't stripped, parts will be ["video", "unknown", "txt"]
    let (lang, is_auto, format) = parse_subtitle_info("video.unknown.txt");
    assert_eq!(lang, "txt"); // Last part after splitting by '.'
    assert!(!is_auto);
    assert_eq!(format, "srt"); // defaults to srt when not .vtt
}

#[tokio::test]
async fn test_subtitle_artifact_metadata_storage() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link and archive
    let new_link = NewLink {
        original_url: "https://youtube.com/watch?v=test".to_string(),
        normalized_url: "https://youtube.com/watch?v=test".to_string(),
        canonical_url: None,
        domain: "youtube.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");

    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .expect("Failed to create pending archive");

    // Insert subtitle artifact with metadata
    let metadata = r#"{"language":"en","is_auto":true,"format":"vtt"}"#;
    let artifact_id = insert_artifact_with_metadata(
        db.pool(),
        archive_id,
        "subtitles",
        "test/video.en-auto.vtt",
        Some("text/vtt"),
        Some(1024),
        None,
        Some(metadata),
    )
    .await
    .expect("Failed to insert artifact");

    assert!(artifact_id > 0);

    // Verify artifact was stored
    let artifacts = get_artifacts_for_archive(db.pool(), archive_id)
        .await
        .expect("Failed to get artifacts");

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, "subtitles");
    assert!(artifacts[0].metadata.is_some());

    let stored_metadata = artifacts[0].metadata.as_ref().unwrap();
    assert!(stored_metadata.contains("en"));
    assert!(stored_metadata.contains("true")); // is_auto
}

#[tokio::test]
async fn test_transcript_artifact_storage() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link and archive
    let new_link = NewLink {
        original_url: "https://youtube.com/watch?v=test".to_string(),
        normalized_url: "https://youtube.com/watch?v=test".to_string(),
        canonical_url: None,
        domain: "youtube.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");

    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .expect("Failed to create pending archive");

    // Insert transcript artifact
    let artifact_id = insert_artifact_with_metadata(
        db.pool(),
        archive_id,
        "transcript",
        "test/transcript.txt",
        Some("text/plain"),
        Some(2048),
        None,
        None, // Transcripts don't need metadata
    )
    .await
    .expect("Failed to insert artifact");

    assert!(artifact_id > 0);

    // Verify artifact was stored
    let artifacts = get_artifacts_for_archive(db.pool(), archive_id)
        .await
        .expect("Failed to get artifacts");

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, "transcript");
}

#[tokio::test]
async fn test_multiple_subtitle_artifacts() {
    let (db, _temp_dir) = setup_db().await;

    let new_link = NewLink {
        original_url: "https://youtube.com/watch?v=test".to_string(),
        normalized_url: "https://youtube.com/watch?v=test".to_string(),
        canonical_url: None,
        domain: "youtube.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");

    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .expect("Failed to create pending archive");

    // Insert multiple subtitle files
    let subtitles = vec![
        ("en", true, "vtt"),
        ("en", false, "srt"),
        ("es", false, "vtt"),
    ];

    for (lang, is_auto, format) in subtitles {
        let metadata = format!(
            r#"{{"language":"{}","is_auto":{},"format":"{}"}}"#,
            lang, is_auto, format
        );
        insert_artifact_with_metadata(
            db.pool(),
            archive_id,
            "subtitles",
            &format!("test/video.{}.{}", lang, format),
            Some(&format!("text/{}", format)),
            Some(1024),
            None,
            Some(&metadata),
        )
        .await
        .expect("Failed to insert subtitle");
    }

    // Verify all subtitles were stored
    let artifacts = get_artifacts_for_archive(db.pool(), archive_id)
        .await
        .expect("Failed to get artifacts");

    assert_eq!(artifacts.len(), 3);
    assert!(artifacts.iter().all(|a| a.kind == "subtitles"));
    assert!(artifacts.iter().all(|a| a.metadata.is_some()));
}

#[tokio::test]
async fn test_youtube_rolling_subtitle_deduplication() {
    // Real YouTube rolling subtitle content (from sample VTT file)
    let content = r#"WEBVTT
Kind: captions
Language: en

00:00:04.799 --> 00:00:06.390 align:start position:0%

We<00:00:05.040><c> have</c><00:00:05.200><c> to</c><00:00:05.359><c> dive</c><00:00:05.680><c> straight</c><00:00:05.920><c> in</c><00:00:06.080><c> with</c><00:00:06.240><c> our</c>

00:00:06.390 --> 00:00:06.400 align:start position:0%
We have to dive straight in with our

00:00:06.400 --> 00:00:08.150 align:start position:0%
We have to dive straight in with our
main<00:00:06.720><c> story</c><00:00:06.879><c> this</c><00:00:07.120><c> week,</c><00:00:07.279><c> which</c><00:00:07.520><c> concerns</c>

00:00:08.150 --> 00:00:08.160 align:start position:0%
main story this week, which concerns

00:00:08.160 --> 00:00:09.830 align:start position:0%
main story this week, which concerns
what's<00:00:08.480><c> been</c><00:00:08.639><c> happening</c><00:00:08.880><c> in</c><00:00:09.200><c> Minnesota.</c><00:00:09.679><c> From</c>

00:00:09.830 --> 00:00:09.840 align:start position:0%
what's been happening in Minnesota. From

00:00:09.840 --> 00:00:11.589 align:start position:0%
what's been happening in Minnesota. From
the<00:00:10.000><c> surge</c><00:00:10.240><c> in</c><00:00:10.400><c> immigration</c><00:00:10.800><c> raids</c><00:00:11.200><c> to</c><00:00:11.360><c> the</c>

00:00:11.589 --> 00:00:11.599 align:start position:0%
the surge in immigration raids to the

00:00:11.599 --> 00:00:13.749 align:start position:0%
the surge in immigration raids to the
killings<00:00:11.920><c> of</c><00:00:12.000><c> Renee</c><00:00:12.320><c> Good</c><00:00:12.559><c> and</c><00:00:12.719><c> Alex</c><00:00:12.960><c> Prey</c><00:00:13.519><c> to</c>

00:00:13.749 --> 00:00:13.759 align:start position:0%
killings of Renee Good and Alex Prey to

00:00:13.759 --> 00:00:15.589 align:start position:0%
killings of Renee Good and Alex Prey to
the<00:00:14.000><c> massive</c><00:00:14.480><c> protests</c><00:00:14.880><c> in</c><00:00:15.040><c> the</c><00:00:15.200><c> streets.</c><00:00:15.440><c> And</c>

00:00:15.589 --> 00:00:15.599 align:start position:0%
the massive protests in the streets. And

00:00:15.599 --> 00:00:17.269 align:start position:0%
the massive protests in the streets. And
to<00:00:15.759><c> be</c><00:00:15.839><c> clear,</c><00:00:16.640><c> for</c><00:00:16.880><c> all</c><00:00:17.039><c> this</c>

00:00:17.269 --> 00:00:17.279 align:start position:0%
to be clear, for all this

00:00:17.279 --> 00:00:19.830 align:start position:0%
to be clear, for all this
administration's<00:00:18.000><c> talk</c><00:00:18.160><c> of</c><00:00:18.400><c> paid</c><00:00:18.720><c> agitators,</c>

00:00:19.830 --> 00:00:19.840 align:start position:0%
administration's talk of paid agitators,

00:00:19.840 --> 00:00:22.230 align:start position:0%
administration's talk of paid agitators,
the<00:00:20.080><c> protests</c><00:00:20.640><c> there</c><00:00:21.039><c> came</c><00:00:21.199><c> from</c><00:00:21.439><c> justifiably</c>
"#;

    let cues = parse_vtt_content(content);

    // 9 rolling cues, 8 snapshot cues → should get 9 cues (snapshots skipped)
    assert_eq!(cues.len(), 9, "Expected 9 cues, got {}", cues.len());

    // Generate transcript and verify no triplication
    let transcript = generate_transcript(&cues);

    // Each phrase should appear exactly once
    // "We have to dive straight in with our" appears in rolling cue 1's tagged line
    let count = transcript
        .matches("We have to dive straight in with our")
        .count();
    assert_eq!(count, 1, "Expected 'We have to dive...' once, got {count}");

    let count = transcript
        .matches("main story this week, which concerns")
        .count();
    assert_eq!(count, 1, "Expected 'main story...' once, got {count}");

    let count = transcript
        .matches("what's been happening in Minnesota.")
        .count();
    assert_eq!(count, 1, "Expected 'Minnesota' once, got {count}");

    let count = transcript
        .matches("the surge in immigration raids to the")
        .count();
    assert_eq!(
        count, 1,
        "Expected 'surge in immigration...' once, got {count}"
    );
}

#[tokio::test]
async fn test_empty_vtt_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let vtt_path = temp_dir.path().join("empty.vtt");

    fs::write(&vtt_path, "WEBVTT\n\n")
        .await
        .expect("Failed to write empty VTT");

    let cues = parse_vtt(&vtt_path)
        .await
        .expect("Failed to parse empty VTT");

    assert!(cues.is_empty(), "Empty VTT should have no cues");
}

#[tokio::test]
async fn test_vtt_with_tags() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let vtt_path = temp_dir.path().join("tagged.vtt");

    let content = r#"WEBVTT

00:00:00.000 --> 00:00:02.000
<c>Colored text</c>

00:00:02.000 --> 00:00:04.000
<v Speaker>Speaker name</v>
"#;

    fs::write(&vtt_path, content)
        .await
        .expect("Failed to write VTT");

    let cues = parse_vtt(&vtt_path).await.expect("Failed to parse VTT");

    assert_eq!(cues.len(), 2);
    // Tags should be removed
    assert_eq!(cues[0].text, "Colored text");
    assert_eq!(cues[1].text, "Speaker name");
}

#[tokio::test]
async fn test_vtt_consecutive_timestamps_no_text() {
    // Reproduces bug where consecutive timestamp lines with no text between them
    // cause timestamp lines to be parsed as text content
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let vtt_path = temp_dir.path().join("consecutive.vtt");

    let content = r#"WEBVTT
Kind: captions
Language: en

00:00:05.600 --> 00:00:11.509 align:start position:0%

Edward<00:00:06.000><c> Teller,</c><00:00:06.879><c> friend</c><00:00:07.200><c> and</c><00:00:07.520><c> colleague.</c>

00:00:11.519 --> 00:00:15.589 align:start position:0%

00:00:15.599 --> 00:00:19.429 align:start position:0%

Many<00:00:11.840><c> people</c><00:00:12.240><c> have</c><00:00:12.480><c> wondered</c>

00:00:19.439 --> 00:00:23.990 align:start position:0%
how Jennifer Nyman

00:00:24.000 --> 00:00:29.029 align:start position:0%
could think so fast
"#;

    fs::write(&vtt_path, content)
        .await
        .expect("Failed to write VTT");

    let cues = parse_vtt(&vtt_path).await.expect("Failed to parse VTT");

    // Should only parse actual text cues, not timestamp-only lines
    assert_eq!(cues.len(), 4, "Should have 4 text cues");

    // First cue
    assert!((cues[0].start_time - 5.6).abs() < 0.001);
    assert!(cues[0].text.contains("Edward"));
    assert!(cues[0].text.contains("Teller"));
    assert!(
        !cues[0].text.contains("-->"),
        "Should not contain timestamp arrows"
    );
    assert!(
        !cues[0].text.contains("00:00"),
        "Should not contain timestamp"
    );

    // Third cue (after skipping the two timestamp-only lines)
    assert!((cues[1].start_time - 15.599).abs() < 0.001);
    assert!(cues[1].text.contains("Many"));
    assert!(cues[1].text.contains("people"));
    assert!(
        !cues[1].text.contains("-->"),
        "Should not contain timestamp arrows"
    );

    // Fourth cue
    assert!((cues[2].start_time - 19.439).abs() < 0.001);
    assert!(cues[2].text.contains("Jennifer Nyman"));

    // Fifth cue
    assert!((cues[3].start_time - 24.0).abs() < 0.001);
    assert!(cues[3].text.contains("could think so fast"));
}

#[tokio::test]
async fn test_vtt_empty_cues_between_text() {
    // Test VTT with timestamp lines that have no associated text
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let vtt_path = temp_dir.path().join("empty_cues.vtt");

    let content = r#"WEBVTT

00:00:00.000 --> 00:00:02.000
First text

00:00:02.000 --> 00:00:03.000

00:00:03.000 --> 00:00:04.000

00:00:04.000 --> 00:00:06.000
Second text
"#;

    fs::write(&vtt_path, content)
        .await
        .expect("Failed to write VTT");

    let cues = parse_vtt(&vtt_path).await.expect("Failed to parse VTT");

    // Should only have cues with actual text
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].text, "First text");
    assert_eq!(cues[1].text, "Second text");
}
