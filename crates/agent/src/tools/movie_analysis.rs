// tools/movie_analysis.rs — Movie fingerprint analysis tools for the agent

use async_trait::async_trait;
use mengxi_core::color_dna::{self, ColorDna, ColorDnaComparison};
use mengxi_core::color_mood::{self, MoodCategory, MoodSegment};
use mengxi_core::movie_fingerprint;
use mengxi_core::scene_boundary::{self, SceneBoundary};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::tool::{Tool, ToolError, ToolResult};

// --- AnalyzeMovieColorTool ---

pub struct AnalyzeMovieColorTool;

#[async_trait]
impl Tool for AnalyzeMovieColorTool {
    fn name(&self) -> &str {
        "analyze_movie_color"
    }
    fn description(&self) -> &str {
        "Analyze the color DNA of a movie fingerprint strip. Extracts Oklab color statistics \
         (average L/a/b, contrast, warmth, saturation) and a 12-bin hue distribution. \
         Input is the path to a fingerprint strip PNG image."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "strip_path": { "type": "string", "description": "Path to fingerprint strip PNG" }
            },
            "required": ["strip_path"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let strip_path = params
            .get("strip_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: strip_path".into()))?;

        let path = PathBuf::from(strip_path);
        if !path.exists() {
            return Err(ToolError::ExecutionError(format!(
                "FILE_NOT_FOUND -- strip image not found: {}", strip_path
            )));
        }

        let (w, h, data) = movie_fingerprint::read_strip_png(&path)
            .map_err(|e| ToolError::ExecutionError(format!("READ_ERROR -- {}", e)))?;

        let dna = color_dna::extract_color_dna(&data, w, h)
            .map_err(|e| ToolError::ExecutionError(format!("DNA_EXTRACT_ERROR -- {}", e)))?;

        let display = color_dna_to_json(&dna, w, h);
        let summary = format_color_dna(&dna, w, h);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- DetectMovieScenesTool ---

pub struct DetectMovieScenesTool;

#[async_trait]
impl Tool for DetectMovieScenesTool {
    fn name(&self) -> &str {
        "detect_movie_scenes"
    }
    fn description(&self) -> &str {
        "Detect scene boundaries in a movie fingerprint strip. Identifies abrupt visual changes \
         between consecutive frames using color histogram distance. Returns frame indices and \
         confidence scores for each detected boundary."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "strip_path": { "type": "string", "description": "Path to fingerprint strip PNG" },
                "threshold": { "type": "number", "description": "Detection sensitivity (0.0-1.0, default 0.3)" },
                "min_scene_length": { "type": "integer", "description": "Minimum frames between boundaries (default 5)" },
                "max_boundaries": { "type": "integer", "description": "Maximum number of boundaries to return (default 50)" }
            },
            "required": ["strip_path"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let strip_path = params
            .get("strip_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: strip_path".into()))?;

        let threshold = params
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.3);
        let min_scene = params
            .get("min_scene_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;
        let max_bounds = params
            .get("max_boundaries")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let path = PathBuf::from(strip_path);
        if !path.exists() {
            return Err(ToolError::ExecutionError(format!(
                "FILE_NOT_FOUND -- strip image not found: {}", strip_path
            )));
        }

        let (w, h, data) = movie_fingerprint::read_strip_png(&path)
            .map_err(|e| ToolError::ExecutionError(format!("READ_ERROR -- {}", e)))?;

        let boundaries = scene_boundary::detect_scene_boundaries(
            &data, w, h, threshold, min_scene, max_bounds,
        )
        .map_err(|e| ToolError::ExecutionError(format!("SCENE_DETECT_ERROR -- {}", e)))?;

        let display = boundaries_to_json(&boundaries, w, h);
        let summary = format_boundaries(&boundaries, w, h);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- GetMovieMoodTimelineTool ---

pub struct GetMovieMoodTimelineTool;

#[async_trait]
impl Tool for GetMovieMoodTimelineTool {
    fn name(&self) -> &str {
        "get_movie_mood_timeline"
    }
    fn description(&self) -> &str {
        "Compute a color mood timeline for a movie fingerprint strip. Segments the strip \
         into mood categories (Dark, Vivid, Warm, Cool, Neutral) based on Oklab color analysis. \
         Optionally provide scene boundary frame indices for accurate segmentation."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "strip_path": { "type": "string", "description": "Path to fingerprint strip PNG" },
                "boundaries": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Optional frame indices for scene boundaries"
                }
            },
            "required": ["strip_path"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let strip_path = params
            .get("strip_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: strip_path".into()))?;

        let boundary_frames: Vec<usize> = params
            .get("boundaries")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();

        let path = PathBuf::from(strip_path);
        if !path.exists() {
            return Err(ToolError::ExecutionError(format!(
                "FILE_NOT_FOUND -- strip image not found: {}", strip_path
            )));
        }

        let (w, h, data) = movie_fingerprint::read_strip_png(&path)
            .map_err(|e| ToolError::ExecutionError(format!("READ_ERROR -- {}", e)))?;

        let segments = color_mood::compute_mood_timeline(&data, w, h, &boundary_frames)
            .map_err(|e| ToolError::ExecutionError(format!("MOOD_TIMELINE_ERROR -- {}", e)))?;

        let display = mood_segments_to_json(&segments, w, h);
        let summary = format_mood_segments(&segments, w, h);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- CompareMoviesTool ---

pub struct CompareMoviesTool;

#[async_trait]
impl Tool for CompareMoviesTool {
    fn name(&self) -> &str {
        "compare_movies"
    }
    fn description(&self) -> &str {
        "Compare two movies by their color DNA fingerprints. Computes overall similarity, \
         hue distribution similarity, contrast difference, and warmth difference. \
         Returns scores from 0.0 (completely different) to 1.0 (identical)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "strip_a": { "type": "string", "description": "Path to first movie's fingerprint strip PNG" },
                "strip_b": { "type": "string", "description": "Path to second movie's fingerprint strip PNG" }
            },
            "required": ["strip_a", "strip_b"]
        })
    }
    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let strip_a = params
            .get("strip_a")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: strip_a".into()))?;
        let strip_b = params
            .get("strip_b")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: strip_b".into()))?;

        let path_a = PathBuf::from(strip_a);
        let path_b = PathBuf::from(strip_b);

        for (label, path) in [("first", &path_a), ("second", &path_b)] {
            if !path.exists() {
                return Err(ToolError::ExecutionError(format!(
                    "FILE_NOT_FOUND -- {} strip not found: {}", label, path.display()
                )));
            }
        }

        let (wa, ha, data_a) = movie_fingerprint::read_strip_png(&path_a)
            .map_err(|e| ToolError::ExecutionError(format!("READ_ERROR_A -- {}", e)))?;
        let (wb, hb, data_b) = movie_fingerprint::read_strip_png(&path_b)
            .map_err(|e| ToolError::ExecutionError(format!("READ_ERROR_B -- {}", e)))?;

        let dna_a = color_dna::extract_color_dna(&data_a, wa, ha)
            .map_err(|e| ToolError::ExecutionError(format!("DNA_EXTRACT_A_ERROR -- {}", e)))?;
        let dna_b = color_dna::extract_color_dna(&data_b, wb, hb)
            .map_err(|e| ToolError::ExecutionError(format!("DNA_EXTRACT_B_ERROR -- {}", e)))?;

        let comp = color_dna::compare_color_dna(&dna_a, &dna_b)
            .map_err(|e| ToolError::ExecutionError(format!("DNA_COMPARE_ERROR -- {}", e)))?;

        let display = comparison_to_json(&comp, &dna_a, &dna_b, wa, ha, wb, hb);
        let summary = format_comparison(&comp, &dna_a, &dna_b, wa, ha, wb, hb);
        Ok(ToolResult::ok(summary).with_display(display))
    }
}

// --- Serialization helpers ---

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn color_dna_to_json(dna: &ColorDna, w: usize, h: usize) -> Value {
    json!({
        "tool": "analyze_movie_color",
        "strip_dimensions": { "width": w, "height": h },
        "avg_l": round4(dna.avg_l),
        "avg_a": round4(dna.avg_a),
        "avg_b": round4(dna.avg_b),
        "contrast": round4(dna.contrast),
        "warmth": round4(dna.warmth),
        "saturation": round4(dna.saturation),
        "hue_distribution": dna.hue_distribution.iter().map(|&v| round4(v)).collect::<Vec<_>>(),
    })
}

fn format_color_dna(dna: &ColorDna, w: usize, h: usize) -> String {
    format!(
        "Color DNA ({}x{} strip):\n  Average: L={:.3}, a={:.3}, b={:.3}\n  Contrast={:.3}, Warmth={:.3}, Saturation={:.3}\n  Hue distribution: [{}]",
        w, h,
        dna.avg_l, dna.avg_a, dna.avg_b,
        dna.contrast, dna.warmth, dna.saturation,
        dna.hue_distribution.iter().map(|v| format!("{:.3}", v)).collect::<Vec<_>>().join(", ")
    )
}

fn boundaries_to_json(boundaries: &[SceneBoundary], w: usize, h: usize) -> Value {
    json!({
        "tool": "detect_movie_scenes",
        "strip_dimensions": { "width": w, "height": h },
        "boundary_count": boundaries.len(),
        "boundaries": boundaries.iter().map(|b| json!({
            "frame_idx": b.frame_idx,
            "confidence": round4(b.confidence),
            "prev_avg": { "L": round4(b.prev_l), "a": round4(b.prev_a), "b": round4(b.prev_b) },
            "next_avg": { "L": round4(b.next_l), "a": round4(b.next_a), "b": round4(b.next_b) },
        })).collect::<Vec<_>>(),
    })
}

fn format_boundaries(boundaries: &[SceneBoundary], w: usize, h: usize) -> String {
    let mut out = format!("Scene boundaries ({} detected, strip {}x{}):\n", boundaries.len(), w, h);
    for (i, b) in boundaries.iter().enumerate() {
        out.push_str(&format!(
            "  {}: frame {} (confidence {:.4})\n",
            i + 1, b.frame_idx, b.confidence
        ));
    }
    out
}

fn mood_segments_to_json(segments: &[MoodSegment], w: usize, h: usize) -> Value {
    json!({
        "tool": "get_movie_mood_timeline",
        "strip_dimensions": { "width": w, "height": h },
        "segment_count": segments.len(),
        "segments": segments.iter().map(|s| json!({
            "start_frame": s.start_frame,
            "end_frame": s.end_frame,
            "mood": match s.mood {
                MoodCategory::Dark => "Dark",
                MoodCategory::Vivid => "Vivid",
                MoodCategory::Warm => "Warm",
                MoodCategory::Cool => "Cool",
                MoodCategory::Neutral => "Neutral",
            },
            "mood_zh": s.mood.description_zh(),
            "dominant_color": {
                "L": round4(s.dominant_l),
                "a": round4(s.dominant_a),
                "b": round4(s.dominant_b),
            },
        })).collect::<Vec<_>>(),
    })
}

fn format_mood_segments(segments: &[MoodSegment], w: usize, h: usize) -> String {
    let mut out = format!("Color mood timeline ({} segments, strip {}x{}):\n", segments.len(), w, h);
    for s in segments {
        out.push_str(&format!(
            "  Frames {}-{}: {} ({}) — L={:.2}, a={:.2}, b={:.2}\n",
            s.start_frame, s.end_frame,
            s.mood.description_zh(),
            match s.mood {
                MoodCategory::Dark => "Dark",
                MoodCategory::Vivid => "Vivid",
                MoodCategory::Warm => "Warm",
                MoodCategory::Cool => "Cool",
                MoodCategory::Neutral => "Neutral",
            },
            s.dominant_l, s.dominant_a, s.dominant_b,
        ));
    }
    out
}

fn comparison_to_json(
    comp: &ColorDnaComparison,
    dna_a: &ColorDna,
    dna_b: &ColorDna,
    wa: usize, ha: usize,
    wb: usize, hb: usize,
) -> Value {
    json!({
        "tool": "compare_movies",
        "strip_a": { "width": wa, "height": ha, "avg_l": round4(dna_a.avg_l), "saturation": round4(dna_a.saturation) },
        "strip_b": { "width": wb, "height": hb, "avg_l": round4(dna_b.avg_l), "saturation": round4(dna_b.saturation) },
        "comparison": {
            "overall_similarity": round4(comp.overall_similarity),
            "hue_similarity": round4(comp.hue_similarity),
            "contrast_diff": round4(comp.contrast_diff),
            "warmth_diff": round4(comp.warmth_diff),
        },
    })
}

fn format_comparison(
    comp: &ColorDnaComparison,
    dna_a: &ColorDna,
    dna_b: &ColorDna,
    wa: usize, ha: usize,
    wb: usize, hb: usize,
) -> String {
    format!(
        "Movie comparison:\n  Overall similarity: {:.4}\n  Hue similarity: {:.4}\n  Contrast diff: {:.4}\n  Warmth diff: {:.4}\n  Strip A ({}x{}): L={:.3}, sat={:.3}\n  Strip B ({}x{}): L={:.3}, sat={:.3}",
        comp.overall_similarity, comp.hue_similarity, comp.contrast_diff, comp.warmth_diff,
        wa, ha, dna_a.avg_l, dna_a.saturation,
        wb, hb, dna_b.avg_l, dna_b.saturation,
    )
}
