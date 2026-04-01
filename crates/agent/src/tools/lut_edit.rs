// tools/lut_edit.rs — LUT editing tools with hash verification and undo
//
// Provides LoadLutTool, EditLutTool, SaveLutTool, and UndoLutEditTool.
// Edit sessions are stored in a shared LutEditStore.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mengxi_format::lut::{self, LutData};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::tool::{Tool, ToolError, ToolResult};

use super::hash_anchor::{self, AnchoredLut, LuminanceRegion};

// ---------------------------------------------------------------------------
// Edit operations
// ---------------------------------------------------------------------------

/// Parametric LUT edit operations.
enum EditOp {
    /// Lift (shadow offset): out = in + lift * (1 - in)
    Lift { r: f64, g: f64, b: f64 },
    /// Gain (multiplicative): out = in * gain
    Gain { r: f64, g: f64, b: f64 },
    /// Gamma (power curve): out = in^(1/gamma)
    Gamma { r: f64, g: f64, b: f64 },
    /// Offset (additive): out = in + offset
    Offset { r: f64, g: f64, b: f64 },
    /// Saturation adjustment via HSL
    Saturation { amount: f64 },
    /// Hue rotation via HSL
    HueShift { degrees: f64 },
}

impl EditOp {
    fn label(&self) -> &str {
        match self {
            Self::Lift { .. } => "lift",
            Self::Gain { .. } => "gain",
            Self::Gamma { .. } => "gamma",
            Self::Offset { .. } => "offset",
            Self::Saturation { .. } => "saturation",
            Self::HueShift { .. } => "hue_shift",
        }
    }

    /// Apply this operation to a single RGB triplet, optionally only for entries
    /// in the specified luminance region.
    fn apply(&self, r: f64, g: f64, b: f64, region: Option<LuminanceRegion>) -> (f64, f64, f64) {
        if let Some(reg) = region {
            let entry_region = hash_anchor::classify_entry(r, g, b);
            if entry_region != reg {
                return (r, g, b); // skip entries outside the target region
            }
        }

        match self {
            Self::Lift { r: lr, g: lg, b: lb } => {
                let or = r + lr * (1.0 - r);
                let og = g + lg * (1.0 - g);
                let ob = b + lb * (1.0 - b);
                (or.clamp(0.0, 1.0), og.clamp(0.0, 1.0), ob.clamp(0.0, 1.0))
            }
            Self::Gain { r: gr, g: gg, b: gb } => {
                let or = r * gr;
                let og = g * gg;
                let ob = b * gb;
                (or.clamp(0.0, 1.0), og.clamp(0.0, 1.0), ob.clamp(0.0, 1.0))
            }
            Self::Gamma { r: gr, g: gg, b: gb } => {
                let or = if r > 0.0 { r.powf(1.0 / gr) } else { 0.0 };
                let og = if g > 0.0 { g.powf(1.0 / gg) } else { 0.0 };
                let ob = if b > 0.0 { b.powf(1.0 / gb) } else { 0.0 };
                (or.clamp(0.0, 1.0), og.clamp(0.0, 1.0), ob.clamp(0.0, 1.0))
            }
            Self::Offset { r: or, g: og, b: ob } => {
                ((r + or).clamp(0.0, 1.0), (g + og).clamp(0.0, 1.0), (b + ob).clamp(0.0, 1.0))
            }
            Self::Saturation { amount } => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                let new_s = (s * amount).clamp(0.0, 1.0);
                hsl_to_rgb(h, new_s, l)
            }
            Self::HueShift { degrees } => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                let new_h = (h + degrees / 360.0) % 1.0;
                hsl_to_rgb(if new_h < 0.0 { new_h + 1.0 } else { new_h }, s, l)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HSL helpers
// ---------------------------------------------------------------------------

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < 1e-10 {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };

    let h = if (max - r).abs() < 1e-10 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-10 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s.abs() < 1e-10 {
        return (l, l, l);
    }

    fn hue_to_rgb(p: f64, q: f64, t: f64) -> f64 {
        let t = if t < 0.0 { t + 1.0 } else if t > 1.0 { t - 1.0 } else { t };
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    }

    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0).clamp(0.0, 1.0),
        hue_to_rgb(p, q, h).clamp(0.0, 1.0),
        hue_to_rgb(p, q, h - 1.0 / 3.0).clamp(0.0, 1.0),
    )
}

// ---------------------------------------------------------------------------
// Edit session
// ---------------------------------------------------------------------------

/// A loaded LUT with its editing state.
pub struct LutEditSession {
    pub id: String,
    pub source_path: String,
    pub original: LutData,
    pub current: LutData,
    pub current_anchors: AnchoredLut,
    pub undo_stack: Vec<LutData>,
    pub edit_count: usize,
}

/// Shared store for LUT edit sessions across tools.
pub type LutEditStore = Arc<Mutex<HashMap<String, LutEditSession>>>;

/// Create a new shared LUT edit store.
pub fn new_store() -> LutEditStore {
    Arc::new(Mutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// LoadLutTool
// ---------------------------------------------------------------------------

pub struct LoadLutTool {
    store: LutEditStore,
}

impl LoadLutTool {
    pub fn new(store: LutEditStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for LoadLutTool {
    fn name(&self) -> &str {
        "load_lut"
    }

    fn description(&self) -> &str {
        "Load a LUT file into an editing session. Returns session ID, LUT summary, \
         and hash anchors for each tonal region (shadows, midtones, highlights). \
         The session ID is used with edit_lut, save_lut, and undo_lut_edit tools."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the LUT file (.cube, .3dl, .look, .csp, .cdl)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path".into()))?;

        let lut = lut::parse_lut(Path::new(path))
            .map_err(|e| ToolError::ExecutionError(format!("LUT_PARSE_ERROR -- {}", e)))?;

        lut.validate()
            .map_err(|e| ToolError::ExecutionError(format!("LUT_VALIDATE_ERROR -- {}", e)))?;

        let source_path = path.to_string();
        let anchors = hash_anchor::compute_anchors(&lut);
        let session_id = Uuid::new_v4().to_string()[..8].to_string();

        let session = LutEditSession {
            id: session_id.clone(),
            source_path: source_path.clone(),
            original: lut.clone(),
            current: lut,
            current_anchors: anchors.clone(),
            undo_stack: Vec::new(),
            edit_count: 0,
        };

        let summary = format_session_summary(&session);

        self.store
            .lock()
            .map_err(|e| ToolError::ExecutionError(format!("LOCK_ERROR -- {}", e)))?
            .insert(session_id.clone(), session);

        Ok(ToolResult::ok(format!(
            "Loaded LUT session {}.\n{}\n\nAnchors:\n{}",
            session_id,
            summary,
            format_anchors(&anchors)
        )))
    }
}

// ---------------------------------------------------------------------------
// EditLutTool
// ---------------------------------------------------------------------------

pub struct EditLutTool {
    store: LutEditStore,
}

impl EditLutTool {
    pub fn new(store: LutEditStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for EditLutTool {
    fn name(&self) -> &str {
        "edit_lut"
    }

    fn description(&self) -> &str {
        "Apply an edit operation to a loaded LUT session. Operations: lift, gain, gamma, offset, saturation, hue_shift. \
         Optionally target a specific tonal region (shadows, midtones, highlights) and provide a hash anchor for verification."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "LUT editing session ID" },
                "operation": {
                    "type": "string",
                    "enum": ["lift", "gain", "gamma", "offset", "saturation", "hue_shift"],
                    "description": "Edit operation type"
                },
                "r": { "type": "number", "description": "Red channel value (default 1.0 for gain/gamma, 0.0 for lift/offset)" },
                "g": { "type": "number", "description": "Green channel value" },
                "b": { "type": "number", "description": "Blue channel value" },
                "amount": { "type": "number", "description": "Amount for saturation or hue_shift" },
                "region": {
                    "type": "string",
                    "enum": ["shadows", "midtones", "highlights"],
                    "description": "Target tonal region (default: all)"
                },
                "expected_hash": { "type": "integer", "description": "Expected region or full hash for verification" }
            },
            "required": ["session_id", "operation"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: session_id".into()))?;
        let op_name = params
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: operation".into()))?;

        let region = params
            .get("region")
            .and_then(|v| v.as_str())
            .and_then(|r| match r {
                "shadows" => Some(LuminanceRegion::Shadows),
                "midtones" => Some(LuminanceRegion::Midtones),
                "highlights" => Some(LuminanceRegion::Highlights),
                _ => None,
            });

        let expected_hash = params.get("expected_hash").and_then(|v| v.as_u64());

        let op = parse_edit_op(op_name, &params)?;

        let mut store = self.store
            .lock()
            .map_err(|e| ToolError::ExecutionError(format!("LOCK_ERROR -- {}", e)))?;

        let session = store
            .get_mut(session_id)
            .ok_or_else(|| ToolError::ExecutionError(format!("SESSION_NOT_FOUND -- {}", session_id)))?;

        // Verify hash anchor if provided
        if let Some(expected) = expected_hash {
            let actual = if let Some(reg) = region {
                session.current_anchors.region_anchors.iter()
                    .find(|a| a.region == reg)
                    .map(|a| a.hash)
                    .unwrap_or(session.current_anchors.full_hash)
            } else {
                session.current_anchors.full_hash
            };
            if actual != expected {
                return Ok(ToolResult::err(format!(
                    "HASH_MISMATCH -- expected {} but current {} hash is {}. \
                     The LUT may have been modified. Reload the session to get current anchors.",
                    expected, op_name, actual
                )));
            }
        }

        // Push current state to undo stack
        session.undo_stack.push(session.current.clone());

        // Apply the edit
        let old = session.current.clone();
        apply_edit(&mut session.current, &op, region);

        // Recompute anchors
        session.current_anchors = hash_anchor::compute_anchors(&session.current);
        session.edit_count += 1;

        // Compute diff for reporting
        let diff = match old.diff(&session.current) {
            Ok(d) => d,
            Err(_) => {
                // Grid size mismatch shouldn't happen, but handle gracefully
                return Ok(ToolResult::ok(format!(
                    "Applied {} (edit #{})",
                    op.label(), session.edit_count
                )));
            }
        };

        let region_label = region.map(|r| r.label()).unwrap_or("all");
        Ok(ToolResult::ok(format!(
            "Applied {} to {} (edit #{}).\n\nDiff:\n  R: mean={:.4}, max={:.4}, changed={}\n  G: mean={:.4}, max={:.4}, changed={}\n  B: mean={:.4}, max={:.4}, changed={}\n\nNew anchors:\n{}",
            op.label(),
            region_label,
            session.edit_count,
            diff.channels[0].mean_delta, diff.channels[0].max_delta, diff.channels[0].changed_count,
            diff.channels[1].mean_delta, diff.channels[1].max_delta, diff.channels[1].changed_count,
            diff.channels[2].mean_delta, diff.channels[2].max_delta, diff.channels[2].changed_count,
            format_anchors(&session.current_anchors),
        )))
    }
}

// ---------------------------------------------------------------------------
// SaveLutTool
// ---------------------------------------------------------------------------

pub struct SaveLutTool {
    store: LutEditStore,
}

impl SaveLutTool {
    pub fn new(store: LutEditStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for SaveLutTool {
    fn name(&self) -> &str {
        "save_lut"
    }

    fn description(&self) -> &str {
        "Save an edited LUT session to a file. Format is detected from the output file extension."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "LUT editing session ID" },
                "output_path": { "type": "string", "description": "Output file path" }
            },
            "required": ["session_id", "output_path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: session_id".into()))?;
        let output_path = params
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: output_path".into()))?;

        let store = self.store
            .lock()
            .map_err(|e| ToolError::ExecutionError(format!("LOCK_ERROR -- {}", e)))?;

        let session = store
            .get(session_id)
            .ok_or_else(|| ToolError::ExecutionError(format!("SESSION_NOT_FOUND -- {}", session_id)))?;

        lut::serialize_lut(&session.current, Path::new(output_path))
            .map_err(|e| ToolError::ExecutionError(format!("LUT_SAVE_ERROR -- {}", e)))?;

        let ext = Path::new(output_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        Ok(ToolResult::ok(format!(
            "Saved LUT to {} (format: {}, grid_size: {}, {} edits applied)",
            output_path, ext, session.current.grid_size, session.edit_count
        )))
    }
}

// ---------------------------------------------------------------------------
// UndoLutEditTool
// ---------------------------------------------------------------------------

pub struct UndoLutEditTool {
    store: LutEditStore,
}

impl UndoLutEditTool {
    pub fn new(store: LutEditStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for UndoLutEditTool {
    fn name(&self) -> &str {
        "undo_lut_edit"
    }

    fn description(&self) -> &str {
        "Undo the last edit operation on a LUT session, restoring the previous state."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "LUT editing session ID" }
            },
            "required": ["session_id"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: session_id".into()))?;

        let mut store = self.store
            .lock()
            .map_err(|e| ToolError::ExecutionError(format!("LOCK_ERROR -- {}", e)))?;

        let session = store
            .get_mut(session_id)
            .ok_or_else(|| ToolError::ExecutionError(format!("SESSION_NOT_FOUND -- {}", session_id)))?;

        match session.undo_stack.pop() {
            Some(previous) => {
                session.current = previous;
                session.current_anchors = hash_anchor::compute_anchors(&session.current);
                session.edit_count = session.edit_count.saturating_sub(1);
                Ok(ToolResult::ok(format!(
                    "Undo successful. Restored to edit #{}. Remaining undo depth: {}.\n\nAnchors:\n{}",
                    session.edit_count,
                    session.undo_stack.len(),
                    format_anchors(&session.current_anchors),
                )))
            }
            None => Ok(ToolResult::err("Nothing to undo — undo stack is empty.".to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_edit_op(name: &str, params: &Value) -> Result<EditOp, ToolError> {
    let r = params.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let g = params.get("g").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let b = params.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let amount = params.get("amount").and_then(|v| v.as_f64()).unwrap_or(1.0);

    match name {
        "lift" => Ok(EditOp::Lift { r, g, b }),
        "gain" => Ok(EditOp::Gain {
            r: if r == 0.0 { 1.0 } else { r },
            g: if g == 0.0 { 1.0 } else { g },
            b: if b == 0.0 { 1.0 } else { b },
        }),
        "gamma" => Ok(EditOp::Gamma {
            r: if r == 0.0 { 1.0 } else { r },
            g: if g == 0.0 { 1.0 } else { g },
            b: if b == 0.0 { 1.0 } else { b },
        }),
        "offset" => Ok(EditOp::Offset { r, g, b }),
        "saturation" => Ok(EditOp::Saturation { amount }),
        "hue_shift" => Ok(EditOp::HueShift { degrees: amount }),
        _ => Err(ToolError::InvalidParams(format!("Unknown operation: {}", name))),
    }
}

fn apply_edit(lut: &mut LutData, op: &EditOp, region: Option<LuminanceRegion>) {
    let total = lut.grid_size as usize * lut.grid_size as usize * lut.grid_size as usize;
    for i in 0..total {
        let idx = i * 3;
        let (r, g, b) = op.apply(lut.values[idx], lut.values[idx + 1], lut.values[idx + 2], region);
        lut.values[idx] = r;
        lut.values[idx + 1] = g;
        lut.values[idx + 2] = b;
    }
}

fn format_session_summary(session: &LutEditSession) -> String {
    format!(
        "Source: {}\nGrid size: {}\nDomain: [{:.2},{:.2},{:.2}] → [{:.2},{:.2},{:.2}]\nTotal entries: {}",
        session.source_path,
        session.current.grid_size,
        session.current.domain_min[0], session.current.domain_min[1], session.current.domain_min[2],
        session.current.domain_max[0], session.current.domain_max[1], session.current.domain_max[2],
        session.current.grid_size as usize * session.current.grid_size as usize * session.current.grid_size as usize,
    )
}

fn format_anchors(anchored: &AnchoredLut) -> String {
    let mut s = format!("  Full hash: {}\n", anchored.full_hash);
    for anchor in &anchored.region_anchors {
        s.push_str(&format!(
            "  {}: hash={}, entries={}\n",
            anchor.region.label(), anchor.hash, anchor.entry_count
        ));
    }
    s
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> LutEditStore {
        new_store()
    }

    fn make_session(store: &LutEditStore) -> String {
        let id = "test1234".to_string();
        let lut = LutData::identity(9);
        let anchors = hash_anchor::compute_anchors(&lut);
        let session = LutEditSession {
            id: id.clone(),
            source_path: "test.cube".into(),
            original: lut.clone(),
            current: lut,
            current_anchors: anchors,
            undo_stack: Vec::new(),
            edit_count: 0,
        };
        store.lock().unwrap().insert(id.clone(), session);
        id
    }

    #[test]
    fn test_edit_op_lift() {
        let op = EditOp::Lift { r: 0.1, g: 0.0, b: 0.0 };
        // Dark pixel: in=0.0 → out = 0.0 + 0.1*(1-0.0) = 0.1
        let (r, _, _) = op.apply(0.0, 0.0, 0.0, None);
        assert!((r - 0.1).abs() < 1e-6);
        // Bright pixel: in=1.0 → out = 1.0 + 0.1*(1-1.0) = 1.0
        let (r, _, _) = op.apply(1.0, 0.0, 0.0, None);
        assert!((r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_edit_op_gain() {
        let op = EditOp::Gain { r: 1.5, g: 1.0, b: 1.0 };
        let (r, _, _) = op.apply(0.5, 0.5, 0.5, None);
        assert!((r - 0.75).abs() < 1e-6);
    }

    #[test]
    fn test_edit_op_gamma() {
        let op = EditOp::Gamma { r: 2.0, g: 1.0, b: 1.0 };
        let (r, _, _) = op.apply(0.25, 0.25, 0.25, None);
        // out = 0.25^(1/2) = 0.5
        assert!((r - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_edit_op_region_filter() {
        let op = EditOp::Offset { r: 1.0, g: 1.0, b: 1.0 };
        // Shadow pixel should be affected
        let (r, _g, _b) = op.apply(0.1, 0.1, 0.1, Some(LuminanceRegion::Shadows));
        assert!(r > 0.1);
        // Highlight pixel should NOT be affected when targeting shadows
        let (r, _, _) = op.apply(0.9, 0.9, 0.9, Some(LuminanceRegion::Shadows));
        assert!((r - 0.9).abs() < 1e-6);
    }

    #[test]
    fn test_rgb_hsl_roundtrip() {
        let (r, g, b) = (0.8, 0.3, 0.5);
        let (h, s, l) = rgb_to_hsl(r, g, b);
        let (r2, g2, b2) = hsl_to_rgb(h, s, l);
        assert!((r - r2).abs() < 1e-6);
        assert!((g - g2).abs() < 1e-6);
        assert!((b - b2).abs() < 1e-6);
    }

    #[test]
    fn test_undo_restores_state() {
        let store = make_store();
        let id = make_session(&store);
        {
            let mut guard = store.lock().unwrap();
            let session = guard.get_mut(&id).unwrap();
            let original_hash = session.current_anchors.full_hash;
            session.undo_stack.push(session.current.clone());
            apply_edit(&mut session.current, &EditOp::Gain { r: 2.0, g: 2.0, b: 2.0 }, None);
            session.current_anchors = hash_anchor::compute_anchors(&session.current);
            assert_ne!(session.current_anchors.full_hash, original_hash);
        }
        // Undo
        {
            let mut guard = store.lock().unwrap();
            let session = guard.get_mut(&id).unwrap();
            let previous = session.undo_stack.pop().unwrap();
            session.current = previous;
            session.current_anchors = hash_anchor::compute_anchors(&session.current);
            let lut = &session.current;
            // Should be identity
            let identity = LutData::identity(9);
            let diff = lut.diff(&identity).unwrap();
            assert_eq!(diff.channels[0].mean_delta, 0.0);
        }
    }

    #[tokio::test]
    async fn test_load_lut_tool() {
        let store = make_store();
        let tool = LoadLutTool::new(store.clone());

        // Create a temp cube file
        let lut = LutData::identity(5);
        let dir = std::env::temp_dir().join("mengxi_test_lut_edit");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.cube");
        lut::serialize_lut(&lut, &path).unwrap();

        let result = tool.execute(json!({"path": path.to_str().unwrap()})).await;
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(!tool_result.is_error);
        assert!(tool_result.content.contains("Loaded LUT session"));

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_edit_lut_hash_mismatch() {
        let store = make_store();
        let id = make_session(&store);
        let tool = EditLutTool::new(store.clone());

        let result = tool.execute(json!({
            "session_id": id,
            "operation": "lift",
            "r": 0.1, "g": 0.1, "b": 0.1,
            "expected_hash": 99999
        })).await;

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_error);
        assert!(tool_result.content.contains("HASH_MISMATCH"));
    }
}
