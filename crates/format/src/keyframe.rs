use std::path::Path;

/// Extract a keyframe image from an MOV file at a specific timestamp using FFmpeg.
pub fn extract_keyframe_image(
    mov_path: &Path,
    timestamp_secs: f64,
    output_path: &Path,
) -> Result<(), String> {
    let ts = format!("{:.3}", timestamp_secs);

    let output = std::process::Command::new("ffmpeg")
        .arg("-ss")
        .arg(&ts)
        .arg("-i")
        .arg(mov_path)
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("2")
        .arg("-y")
        .arg(output_path)
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    Ok(())
}

/// Check if FFmpeg is available on the system.
pub fn check_ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Extract keyframe images for all keyframes from an MOV file.
/// Returns paths to extracted JPEG files.
pub fn extract_keyframe_images(
    mov_path: &Path,
    keyframes: &[(u64, f64)], // (frame_number, timestamp_secs)
    output_dir: &Path,
) -> Result<Vec<std::path::PathBuf>, String> {
    if !check_ffmpeg_available() {
        return Err("ffmpeg not found on system".to_string());
    }

    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("failed to create dir: {}", e))?;

    let filename_stem = mov_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut extracted = Vec::new();
    for (i, (_frame_num, timestamp)) in keyframes.iter().enumerate() {
        let output_name = format!("{}_keyframe_{:04}.jpg", filename_stem, i + 1);
        let output_path = output_dir.join(&output_name);

        extract_keyframe_image(mov_path, *timestamp, &output_path)?;
        extracted.push(output_path);
    }

    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_ffmpeg_available() {
        // This test passes regardless of whether ffmpeg is installed
        // It just verifies the function doesn't panic
        let _result = check_ffmpeg_available();
    }
}
