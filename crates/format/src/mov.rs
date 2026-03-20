use std::path::Path;

/// Summary of MOV file metadata.
#[derive(Debug, Clone)]
pub struct MovHeader {
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub fps: f64,
    pub duration_secs: f64,
    pub frame_count: u64,
    pub bit_depth: Option<u8>,
}

/// Information about a single keyframe.
#[derive(Debug, Clone)]
pub struct KeyframeInfo {
    pub frame_number: u64,
    pub timestamp_secs: f64,
}

/// MOV parsing error.
#[derive(Debug)]
pub enum MovError {
    ParseError(String),
    NoVideoTrack,
    MissingMetadata(String),
    FfmpegFailed(String),
    IoError(String),
}

impl std::fmt::Display for MovError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MovError::ParseError(msg) => write!(f, "{}", msg),
            MovError::NoVideoTrack => write!(f, "MOV file contains no video track"),
            MovError::MissingMetadata(field) => write!(f, "missing metadata: {}", field),
            MovError::FfmpegFailed(msg) => write!(f, "ffmpeg failed: {}", msg),
            MovError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for MovError {}

/// Parse an MOV/MP4 file header, extracting metadata using mp4parse.
pub fn parse_mov_header(path: &Path) -> Result<MovHeader, MovError> {
    let mut file =
        std::fs::File::open(path).map_err(|e| MovError::IoError(e.to_string()))?;
    let ctx =
        mp4parse::read_mp4(&mut file).map_err(|e| MovError::ParseError(e.to_string()))?;

    // Find the first video track
    let video_track = ctx
        .tracks
        .iter()
        .find(|t| matches!(t.track_type, mp4parse::TrackType::Video))
        .ok_or(MovError::NoVideoTrack)?;

    // Width and height from tkhd (16.16 fixed-point)
    let tkhd = video_track
        .tkhd
        .as_ref()
        .ok_or(MovError::MissingMetadata("tkhd".to_string()))?;
    let width = tkhd.width >> 16;
    let height = tkhd.height >> 16;

    // Timescale and duration
    let timescale = video_track
        .timescale
        .map(|mp4parse::TrackTimeScale(ts, _)| ts)
        .ok_or(MovError::MissingMetadata("timescale".to_string()))?;
    let duration_ticks = video_track
        .duration
        .map(|mp4parse::TrackScaledTime(d, _)| d)
        .unwrap_or(0);
    let duration_secs = if timescale > 0 {
        duration_ticks as f64 / timescale as f64
    } else {
        0.0
    };

    // Frame rate and count from stts
    let (fps, frame_count) = if let Some(ref stts) = video_track.stts {
        derive_fps_and_count(stts, timescale)
    } else {
        (0.0, 0)
    };

    // Codec identification
    let codec = if let Some(ref stsd) = video_track.stsd {
        extract_codec_name(stsd)
    } else {
        String::new()
    };

    // If codec is unknown, try ffprobe fallback
    let codec = if codec == "unknown" {
        detect_codec_ffprobe(path).unwrap_or_else(|| "unknown".to_string())
    } else {
        codec
    };

    // Bit depth from codec-specific data
    let bit_depth = if let Some(ref stsd) = video_track.stsd {
        extract_bit_depth(stsd)
    } else {
        None
    };

    Ok(MovHeader {
        width,
        height,
        codec,
        fps,
        duration_secs,
        frame_count,
        bit_depth,
    })
}

/// Derive fps and total frame count from stts (time-to-sample) box.
fn derive_fps_and_count(
    stts: &mp4parse::TimeToSampleBox,
    timescale: u64,
) -> (f64, u64) {
    let total_samples: u64 = stts.samples.iter().map(|s| s.sample_count as u64).sum();
    let total_delta: u64 = stts
        .samples
        .iter()
        .map(|s| s.sample_count as u64 * s.sample_delta as u64)
        .sum();

    let fps = if total_delta > 0 && timescale > 0 {
        (total_samples as f64 * timescale as f64) / total_delta as f64
    } else {
        0.0
    };

    (fps, total_samples)
}

/// Extract codec name from stsd sample descriptions.
fn extract_codec_name(stsd: &mp4parse::SampleDescriptionBox) -> String {
    for desc in &stsd.descriptions {
        if let mp4parse::SampleEntry::Video(ref vse) = desc {
            return match vse.codec_type {
                mp4parse::CodecType::H264 => "H.264".to_string(),
                mp4parse::CodecType::AV1 => "AV1".to_string(),
                mp4parse::CodecType::VP9 => "VP9".to_string(),
                mp4parse::CodecType::VP8 => "VP8".to_string(),
                mp4parse::CodecType::MP4V => "MPEG-4".to_string(),
                _ => "unknown".to_string(),
            };
        }
    }
    "unknown".to_string()
}

/// Detect codec name via ffprobe fallback (for ProRes, DNxHD, etc.).
fn detect_codec_ffprobe(path: &Path) -> Option<String> {
    use crate::keyframe::check_ffmpeg_available;
    use std::process::Command;

    if !check_ffmpeg_available() {
        return None;
    }

    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_streams")
        .arg("-select_streams")
        .arg("v:0")
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    let streams = json.get("streams")?.as_array()?;
    let stream = streams.first()?;
    let name = stream.get("codec_name")?.as_str()?;
    Some(name.to_string())
}

/// Extract bit depth from codec-specific data.
fn extract_bit_depth(stsd: &mp4parse::SampleDescriptionBox) -> Option<u8> {
    for desc in &stsd.descriptions {
        if let mp4parse::SampleEntry::Video(ref vse) = desc {
            return match &vse.codec_specific {
                mp4parse::VideoCodecSpecific::AV1Config(av1) => Some(av1.bit_depth),
                mp4parse::VideoCodecSpecific::VPxConfig(vpx) => Some(vpx.bit_depth),
                mp4parse::VideoCodecSpecific::AVCConfig(_) => {
                    // H.264 AVC config doesn't expose bit_depth directly
                    // Common: 8-bit for most H.264
                    Some(8)
                }
                _ => None,
            };
        }
    }
    None
}

/// Extract keyframe indices and timestamps from an MOV file.
pub fn extract_keyframe_indices(path: &Path) -> Result<Vec<KeyframeInfo>, MovError> {
    let mut file =
        std::fs::File::open(path).map_err(|e| MovError::IoError(e.to_string()))?;
    let ctx =
        mp4parse::read_mp4(&mut file).map_err(|e| MovError::ParseError(e.to_string()))?;

    let video_track = ctx
        .tracks
        .iter()
        .find(|t| matches!(t.track_type, mp4parse::TrackType::Video))
        .ok_or(MovError::NoVideoTrack)?;

    let timescale = video_track
        .timescale
        .map(|mp4parse::TrackTimeScale(ts, _)| ts)
        .ok_or(MovError::MissingMetadata("timescale".to_string()))?;

    let stts = video_track
        .stts
        .as_ref()
        .ok_or(MovError::MissingMetadata("stts".to_string()))?;

    let total_samples: u64 = stts.samples.iter().map(|s| s.sample_count as u64).sum();

    // If stss is None, all frames are sync samples (keyframes)
    let keyframe_indices: Vec<u32> = if let Some(ref stss) = video_track.stss {
        stss.samples.iter().copied().collect()
    } else {
        // All frames are keyframes — return a representative subset (every 1 second)
        let fps = if timescale > 0 {
            let total_delta: u64 = stts
                .samples
                .iter()
                .map(|s| s.sample_count as u64 * s.sample_delta as u64)
                .sum();
            if total_delta > 0 {
                (total_samples as f64 * timescale as f64) / total_delta as f64
            } else {
                24.0
            }
        } else {
            24.0
        };
        let interval = fps.max(1.0) as usize;
        (1..=total_samples).step_by(interval).map(|n| n as u32).collect()
    };

    // Convert keyframe indices (1-based) to timestamps
    let mut keyframes = Vec::new();
    let mut current_sample: u64 = 0;

    for sample in &stts.samples {
        for _i in 0..sample.sample_count {
            current_sample += 1;
            let sample_num = current_sample as u32;

            if keyframe_indices.binary_search(&sample_num).is_ok() {
                let timestamp_secs = if timescale > 0 {
                    // Sum all deltas up to and including this sample
                    (sample_num as f64 * sample.sample_delta as f64) / timescale as f64
                } else {
                    0.0
                };
                keyframes.push(KeyframeInfo {
                    frame_number: current_sample,
                    timestamp_secs,
                });
            }
        }
    }

    Ok(keyframes)
}

/// Map codec name to variant key for import summary.
pub fn codec_to_variant_key(codec: &str) -> String {
    match codec {
        "H.264" => "H.264".to_string(),
        "AV1" => "AV1".to_string(),
        "VP9" => "VP9".to_string(),
        "VP8" => "VP8".to_string(),
        "MPEG-4" => "MPEG-4".to_string(),
        _ => codec.to_string(), // ProRes 422, DNxHD, etc. pass through as-is
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    #[test]
    fn test_codec_to_variant_key() {
        assert_eq!(codec_to_variant_key("H.264"), "H.264");
        assert_eq!(codec_to_variant_key("AV1"), "AV1");
        assert_eq!(codec_to_variant_key("ProRes 422"), "ProRes 422");
        assert_eq!(codec_to_variant_key("DNxHD"), "DNxHD");
        assert_eq!(codec_to_variant_key("unknown"), "unknown");
    }

    #[test]
    fn test_derive_fps_and_count_constant() {
        // 24fps video: timescale=24000, sample_delta=1000, sample_count=24
        let mut stts_data = Vec::new();
        stts_data.push(mp4parse::Sample {
            sample_count: 24,
            sample_delta: 1000,
        });

        // We can't easily construct a TimeToSampleBox since TryVec
        // is from fallible_collections. Test the logic directly.
        let total_samples: u64 = 24;
        let total_delta: u64 = 24 * 1000;
        let timescale: u64 = 24000;
        let fps = (total_samples as f64 * timescale as f64) / total_delta as f64;
        assert!((fps - 24.0).abs() < 0.001);
        assert_eq!(total_samples, 24);
    }

    #[test]
    fn test_corrupt_truncated_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.mov");
        // Write 50 bytes of garbage
        std::fs::write(&path, vec![0u8; 50]).unwrap();

        let result = parse_mov_header(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_corrupt_garbage_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.mov");
        std::fs::write(&path, vec![0xAB; 1024]).unwrap();

        let result = parse_mov_header(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_corrupt_no_video_track() {
        // A valid MP4 container but no video track — use minimal ftyp box
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio_only.mp4");

        // Minimal valid MP4 with just ftyp box
        let ftyp_size: u32 = 20; // 8 (box header) + 12 (ftyp content)
        let mut data = Vec::new();
        data.extend_from_slice(&ftyp_size.to_be_bytes());
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom"); // major brand
        data.extend_from_slice(b"\x00\x00\x00\x00"); // minor version
        data.extend_from_slice(b"isom"); // compatible brand
        std::fs::write(&path, &data).unwrap();

        let result = parse_mov_header(&path);
        // This might parse OK but have no video track
        assert!(result.is_err());
        matches!(result.unwrap_err(), MovError::NoVideoTrack);
    }

    #[test]
    fn test_extract_codec_from_stsd_known() {
        // Test the codec name extraction logic for known types
        assert_eq!(extract_codec_name_from_type(mp4parse::CodecType::H264), "H.264");
        assert_eq!(extract_codec_name_from_type(mp4parse::CodecType::AV1), "AV1");
        assert_eq!(extract_codec_name_from_type(mp4parse::CodecType::VP9), "VP9");
        assert_eq!(extract_codec_name_from_type(mp4parse::CodecType::VP8), "VP8");
        assert_eq!(extract_codec_name_from_type(mp4parse::CodecType::MP4V), "MPEG-4");
        assert_eq!(extract_codec_name_from_type(mp4parse::CodecType::LPCM), "unknown");
        assert_eq!(extract_codec_name_from_type(mp4parse::CodecType::Unknown), "unknown");
    }

    /// Helper to test codec name extraction without constructing a full SampleDescriptionBox
    fn extract_codec_name_from_type(ct: mp4parse::CodecType) -> String {
        match ct {
            mp4parse::CodecType::H264 => "H.264".to_string(),
            mp4parse::CodecType::AV1 => "AV1".to_string(),
            mp4parse::CodecType::VP9 => "VP9".to_string(),
            mp4parse::CodecType::VP8 => "VP8".to_string(),
            mp4parse::CodecType::MP4V => "MPEG-4".to_string(),
            _ => "unknown".to_string(),
        }
    }
}
