// audio_waveform.rs — Audio waveform extraction and visualization for movie fingerprints
//
// Extracts raw PCM audio from video via FFmpeg, downsamples to match strip width,
// and renders a waveform overlay image that can be composited with a fingerprint strip.

use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from audio waveform operations.
#[derive(Debug, thiserror::Error)]
pub enum AudioWaveformError {
    /// FFmpeg not found or failed.
    #[error("AUDIO_WAVEFORM_FFMPEG_ERROR -- {0}")]
    FfmpegError(String),
    /// I/O error.
    #[error("AUDIO_WAVEFORM_IO_ERROR -- {0}")]
    IoError(#[from] std::io::Error),
    /// Invalid input parameters.
    #[error("AUDIO_WAVEFORM_INVALID_INPUT -- {0}")]
    InvalidInput(String),
    /// Audio processing error.
    #[error("AUDIO_WAVEFORM_PROCESSING_ERROR -- {0}")]
    ProcessingError(String),
}

type Result<T> = std::result::Result<T, AudioWaveformError>;

// ---------------------------------------------------------------------------
// PCM extraction
// ---------------------------------------------------------------------------

/// Extract raw PCM audio from a video file using FFmpeg.
///
/// Extracts mono 16-bit PCM at the given sample rate.
/// Returns the raw PCM samples as i16 values.
///
/// # Arguments
/// * `video_path` — Path to the source video file.
/// * `sample_rate` — Target sample rate (default 8000).
pub fn extract_pcm_from_video(
    video_path: &Path,
    sample_rate: u32,
) -> Result<Vec<i16>> {
    if !video_path.exists() {
        return Err(AudioWaveformError::InvalidInput(format!(
            "video file not found: {}", video_path.display()
        )));
    }

    let output = Command::new("ffmpeg")
        .args([
            "-i", &video_path.to_string_lossy(),
            "-vn",                    // no video
            "-ac", "1",               // mono
            "-ar", &sample_rate.to_string(),
            "-f", "s16le",            // raw PCM signed 16-bit little-endian
            "-acodec", "pcm_s16le",
            "-",                      // stdout
        ])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AudioWaveformError::FfmpegError(
                    "ffmpeg not found — install FFmpeg and ensure it is on PATH".to_string(),
                )
            } else {
                AudioWaveformError::FfmpegError(format!("failed to run ffmpeg: {}", e))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AudioWaveformError::FfmpegError(format!(
            "ffmpeg exited with code {:?}: {}",
            output.status.code(),
            stderr.lines().take(5).collect::<Vec<_>>().join("; ")
        )));
    }

    // Convert raw bytes to i16 samples (little-endian)
    let bytes = &output.stdout;
    if bytes.len() < 2 {
        return Err(AudioWaveformError::ProcessingError(
            "no audio data extracted (possibly no audio track)".to_string(),
        ));
    }

    let samples: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    Ok(samples)
}

// ---------------------------------------------------------------------------
// Downsample
// ---------------------------------------------------------------------------

/// Downsample PCM samples to a target number of bins.
///
/// Each bin contains the maximum absolute amplitude in that range (envelope follower).
pub fn downsample_waveform(samples: &[i16], target_bins: usize) -> Vec<f64> {
    if samples.is_empty() || target_bins == 0 {
        return vec![0.0; target_bins.max(1)];
    }

    let samples_per_bin = samples.len() as f64 / target_bins as f64;
    let mut bins = Vec::with_capacity(target_bins);

    for i in 0..target_bins {
        let start = (i as f64 * samples_per_bin) as usize;
        let end = ((i + 1) as f64 * samples_per_bin) as usize;
        let end = end.min(samples.len());

        let peak = samples[start..end]
            .iter()
            .map(|&s| (s as f64 / i16::MAX as f64).abs())
            .fold(0.0_f64, f64::max);

        bins.push(peak);
    }

    bins
}

// ---------------------------------------------------------------------------
// Waveform rendering
// ---------------------------------------------------------------------------

/// Render a waveform as an RGB image strip.
///
/// The image height is `waveform_height`, width matches `target_bins`.
/// The waveform is drawn as a centered vertical bar chart with the given color.
///
/// # Arguments
/// * `waveform_bins` — Normalized amplitude values [0.0, 1.0], one per column.
/// * `waveform_height` — Height of the output image in pixels.
/// * `color` — RGB color as [R, G, B] in [0.0, 1.0].
///
/// # Returns
/// Interleaved RGB f64 data for the waveform image.
pub fn render_waveform(
    waveform_bins: &[f64],
    waveform_height: usize,
    color: [f64; 3],
) -> Vec<f64> {
    let width = waveform_bins.len();
    let mut pixels = vec![0.0_f64; width * waveform_height * 3];

    let mid_y = waveform_height as f64 / 2.0;

    for (x, &amp) in waveform_bins.iter().enumerate() {
        let bar_half = amp * mid_y;
        let y_start = (mid_y - bar_half).round() as usize;
        let y_end = (mid_y + bar_half).round() as usize;
        let y_end = y_end.min(waveform_height);

        for y in y_start..y_end {
            let idx = (y * width + x) * 3;
            pixels[idx] = color[0];
            pixels[idx + 1] = color[1];
            pixels[idx + 2] = color[2];
        }
    }

    pixels
}

// ---------------------------------------------------------------------------
// Composite pipeline
// ---------------------------------------------------------------------------

/// Audio waveform output.
#[derive(Debug)]
pub struct AudioWaveformOutput {
    /// Path to the saved waveform PNG.
    pub waveform_path: PathBuf,
    /// Number of PCM samples extracted.
    pub sample_count: usize,
    /// Number of bins after downsampling.
    pub bin_count: usize,
}

/// Extract audio waveform from a video and save as PNG alongside a fingerprint strip.
///
/// # Arguments
/// * `video_path` — Source video file.
/// * `strip_width` — Width of the fingerprint strip (determines bin count).
/// * `output_path` — Where to save the waveform PNG.
/// * `waveform_height` — Height of the waveform image (default 64).
/// * `sample_rate` — PCM sample rate for extraction (default 8000).
pub fn generate_waveform_image(
    video_path: &Path,
    strip_width: usize,
    output_path: &Path,
    waveform_height: usize,
    sample_rate: u32,
) -> Result<AudioWaveformOutput> {
    if strip_width == 0 {
        return Err(AudioWaveformError::InvalidInput(
            "strip_width must be non-zero".to_string(),
        ));
    }
    if waveform_height == 0 {
        return Err(AudioWaveformError::InvalidInput(
            "waveform_height must be non-zero".to_string(),
        ));
    }

    let samples = extract_pcm_from_video(video_path, sample_rate)?;
    let bins = downsample_waveform(&samples, strip_width);

    // Render with a subtle cyan color
    let pixels = render_waveform(&bins, waveform_height, [0.3, 0.8, 0.9]);

    // Save via movie_fingerprint PNG helper
    crate::movie_fingerprint::save_fingerprint_png(&pixels, strip_width, waveform_height, output_path)
        .map_err(|e| AudioWaveformError::IoError(std::io::Error::other(
            format!("failed to save waveform PNG: {}", e),
        )))?;

    Ok(AudioWaveformOutput {
        waveform_path: output_path.to_path_buf(),
        sample_count: samples.len(),
        bin_count: bins.len(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_downsample_empty() {
        let result = downsample_waveform(&[], 10);
        assert_eq!(result.len(), 10);
        assert!(result.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_downsample_single_bin() {
        let samples: Vec<i16> = vec![1000, -2000, 3000, -4000];
        let bins = downsample_waveform(&samples, 1);
        assert_eq!(bins.len(), 1);
        // Peak should be max absolute value normalized
        let expected = 4000.0 / i16::MAX as f64;
        assert!((bins[0] - expected).abs() < 0.01);
    }

    #[test]
    fn test_downsample_multiple_bins() {
        let samples: Vec<i16> = (0..100).map(|i| (i * 100) as i16).collect();
        let bins = downsample_waveform(&samples, 5);
        assert_eq!(bins.len(), 5);
        // Each bin should have a non-negative value
        assert!(bins.iter().all(|&v| v >= 0.0));
    }

    #[test]
    fn test_render_waveform_dimensions() {
        let bins = vec![0.5, 0.8, 0.3, 0.6, 0.1];
        let pixels = render_waveform(&bins, 20, [1.0, 0.0, 0.0]);
        assert_eq!(pixels.len(), 5 * 20 * 3);
    }

    #[test]
    fn test_render_waveform_zero_amplitude() {
        let bins = vec![0.0; 10];
        let pixels = render_waveform(&bins, 30, [1.0, 1.0, 1.0]);
        // All pixels should be black (0.0) for zero amplitude
        assert!(pixels.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_render_waveform_full_amplitude() {
        let bins = vec![1.0];
        let pixels = render_waveform(&bins, 10, [0.5, 0.5, 0.5]);
        // With full amplitude, most pixels should be colored
        let colored_count = pixels.chunks_exact(3)
            .filter(|px| px[0] > 0.0 || px[1] > 0.0 || px[2] > 0.0)
            .count();
        assert!(colored_count > 5, "Full amplitude should color most pixels");
    }

    #[test]
    fn test_invalid_strip_width() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = generate_waveform_image(
            Path::new("nonexistent.mp4"), 0, dir.path().join("out.png").as_path(), 64, 8000,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("strip_width"));
    }

    #[test]
    fn test_extract_pcm_nonexistent_file() {
        let result = extract_pcm_from_video(Path::new("/nonexistent/video.mp4"), 8000);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_downsample_zero_target_bins() {
        let samples = vec![100i16; 100];
        let bins = downsample_waveform(&samples, 0);
        assert_eq!(bins.len(), 1); // min 1 bin
    }
}
