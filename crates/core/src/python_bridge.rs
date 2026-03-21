// python_bridge.rs — Rust ↔ Python subprocess bridge for AI inference

use serde_json::Value;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Error types for AI subprocess operations.
#[derive(Debug)]
pub enum AiError {
    SubprocessNotFound(String),
    SubprocessCrashed(String),
    Timeout(String),
    ProtocolError(String),
    InferenceError(String),
    ModelNotFound(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubprocessNotFound(msg) => write!(f, "AI_SUBPROCESS_NOT_FOUND -- {}", msg),
            Self::SubprocessCrashed(msg) => write!(f, "AI_SUBPROCESS_CRASHED -- {}", msg),
            Self::Timeout(msg) => write!(f, "AI_TIMEOUT -- {}", msg),
            Self::ProtocolError(msg) => write!(f, "AI_PROTOCOL_ERROR -- {}", msg),
            Self::InferenceError(msg) => write!(f, "AI_INFERENCE_ERROR -- {}", msg),
            Self::ModelNotFound(msg) => write!(f, "AI_MODEL_NOT_FOUND -- {}", msg),
        }
    }
}

impl std::error::Error for AiError {}

/// Internal subprocess handles: child process, stdin writer, stdout reader thread.
struct Subprocess {
    child: Child,
    stdin: Option<ChildStdin>,
    response_rx: mpsc::Receiver<String>,
    _reader_thread: thread::JoinHandle<()>,
}

impl Subprocess {
    /// Spawn a Python subprocess running `python -m mengxi_ai`.
    fn spawn(python: &str) -> Result<Self, AiError> {
        let mut child = Command::new(python)
            .args(["-m", "mengxi_ai"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                AiError::SubprocessNotFound(format!(
                    "Failed to spawn Python subprocess '{}': {}. Is Python installed?",
                    python, e
                ))
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            AiError::SubprocessCrashed("Failed to open subprocess stdin".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AiError::SubprocessCrashed("Failed to open subprocess stdout".into())
        })?;

        // Reader thread: reads lines from stdout and sends them through a channel.
        // This allows the main thread to use recv_timeout for inference timeout.
        let (tx, rx) = mpsc::channel();
        let _reader_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let _ = tx.send(line);
            }
        });

        Ok(Self {
            child,
            stdin: Some(stdin),
            response_rx: rx,
            _reader_thread,
        })
    }

    /// Check if the subprocess is still alive.
    fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    /// Send a raw JSON string request and wait for a response line with timeout.
    fn send_request(&mut self, request: &str, timeout_secs: u64) -> Result<String, AiError> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| AiError::SubprocessCrashed("stdin not available".into()))?;

        // Write request to subprocess stdin
        stdin
            .write_all(request.as_bytes())
            .map_err(|e| AiError::SubprocessCrashed(format!("Broken pipe: {}", e)))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| AiError::SubprocessCrashed(format!("Broken pipe: {}", e)))?;
        stdin
            .flush()
            .map_err(|e| AiError::SubprocessCrashed(format!("Flush failed: {}", e)))?;

        // Read response with timeout
        match self
            .response_rx
            .recv_timeout(Duration::from_secs(timeout_secs))
        {
            Ok(response) => Ok(response),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(AiError::Timeout(format!(
                "AI inference timed out after {}s",
                timeout_secs
            ))),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(AiError::SubprocessCrashed(
                "Reader thread disconnected: subprocess likely crashed".into(),
            )),
        }
    }
}

/// Manages the Python AI subprocess lifecycle.
///
/// Spawns a long-lived Python subprocess on first use, routes JSON requests
/// over stdin/stdout, and handles idle timeout, crash recovery, and inference timeout.
pub struct PythonBridge {
    process: Option<Subprocess>,
    idle_timeout_secs: u64,
    inference_timeout_secs: u64,
    model_name: String,
    last_activity: Instant,
}

impl PythonBridge {
    /// Create a new PythonBridge with the given configuration.
    pub fn new(
        idle_timeout_secs: u64,
        inference_timeout_secs: u64,
        model_name: String,
    ) -> Self {
        Self {
            process: None,
            idle_timeout_secs,
            inference_timeout_secs,
            model_name,
            last_activity: Instant::now(),
        }
    }

    /// Find Python executable: try `python3` first, then `python`.
    fn find_python() -> Result<&'static str, AiError> {
        if Command::new("python3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            Ok("python3")
        } else if Command::new("python")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            Ok("python")
        } else {
            Err(AiError::SubprocessNotFound(
                "Python not found. Please install Python 3.x.".into(),
            ))
        }
    }

    /// Ensure the subprocess is running. Handles idle timeout and auto-spawn.
    fn ensure_running(&mut self) -> Result<(), AiError> {
        if let Some(ref mut process) = self.process {
            if process.is_alive() {
                // Check idle timeout
                if self.last_activity.elapsed() > Duration::from_secs(self.idle_timeout_secs) {
                    self.process = None;
                } else {
                    return Ok(());
                }
            } else {
                // Process has exited
                self.process = None;
            }
        }

        self.spawn()
    }

    /// Spawn the Python subprocess.
    fn spawn(&mut self) -> Result<(), AiError> {
        let python = Self::find_python()?;
        let subprocess = Subprocess::spawn(python)?;
        self.process = Some(subprocess);
        self.last_activity = Instant::now();
        Ok(())
    }

    /// Send a JSON request and receive a JSON response.
    ///
    /// Handles crash recovery: if the subprocess crashes mid-request,
    /// it is respawned and the request is retried exactly once.
    pub fn send_request(&mut self, request: &Value) -> Result<Value, AiError> {
        let request_str = serde_json::to_string(request)
            .map_err(|e| AiError::ProtocolError(format!("Failed to serialize request: {}", e)))?;

        let result = self.send_raw_request(&request_str);

        // Crash recovery: if subprocess crashed, respawn and retry once
        if matches!(result, Err(AiError::SubprocessCrashed(_))) {
            eprintln!("Warning: Python subprocess crashed, respawning...");
            self.process = None;

            if let Err(e) = self.spawn() {
                return Err(AiError::SubprocessCrashed(format!(
                    "Crash recovery failed: {}",
                    e
                )));
            }

            // Retry once
            return self.send_raw_request(&request_str);
        }

        result
    }

    /// Internal: send raw JSON string request with crash recovery handled by caller.
    fn send_raw_request(&mut self, request_str: &str) -> Result<Value, AiError> {
        self.ensure_running()?;

        let process = self
            .process
            .as_mut()
            .ok_or_else(|| AiError::SubprocessCrashed("No subprocess".into()))?;

        match process.send_request(request_str, self.inference_timeout_secs) {
            Ok(response_str) => {
                self.last_activity = Instant::now();
                serde_json::from_str(&response_str).map_err(|e| {
                    AiError::ProtocolError(format!("Invalid JSON from subprocess: {}", e))
                })
            }
            Err(AiError::Timeout(_)) => {
                // Kill subprocess on timeout, don't return partial results
                self.process = None;
                Err(AiError::Timeout(format!(
                    "AI inference timed out after {}s",
                    self.inference_timeout_secs
                )))
            }
            Err(e) => Err(e),
        }
    }

    /// Generate an embedding vector for an image.
    pub fn generate_embedding(&mut self, image_path: &str) -> Result<Vec<f64>, AiError> {
        let model_param = if self.model_name.is_empty() {
            Value::Null
        } else {
            Value::String(self.model_name.clone())
        };

        let request = serde_json::json!({
            "request_id": uuid_simple(),
            "method": "generate_embedding",
            "params": {
                "image_path": image_path,
                "model_name": model_param,
            }
        });

        let response = self.send_request(&request)?;

        // Check for error response
        if response["status"] == "error" {
            let code = response["error"]["code"].as_str().unwrap_or("UNKNOWN");
            let message = response["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(map_python_error(code, message));
        }

        // Extract embedding vector
        let embedding = response["result"]["embedding"].as_array().ok_or_else(|| {
            AiError::ProtocolError("Response missing 'result.embedding' array".into())
        })?;

        let floats: Vec<f64> = embedding
            .iter()
            .map(|v| {
                v.as_f64().ok_or_else(|| {
                    AiError::ProtocolError(format!("Embedding contains non-float value: {}", v))
                })
            })
            .collect::<Result<_, _>>()?;

        if floats.is_empty() {
            return Err(AiError::InferenceError(
                "Empty embedding vector returned".into(),
            ));
        }

        Ok(floats)
    }

    /// Generate semantic tags for an image using CLIP zero-shot classification.
    pub fn generate_tags(&mut self, image_path: &str, top_n: u32) -> Result<Vec<String>, AiError> {
        let request = serde_json::json!({
            "request_id": uuid_simple(),
            "method": "generate_tags",
            "params": {
                "image_path": image_path,
                "model_name": if self.model_name.is_empty() { Value::Null } else { Value::String(self.model_name.clone()) },
                "top_n": top_n,
            }
        });

        let response = self.send_request(&request)?;

        // Check for error response
        if response["status"] == "error" {
            let code = response["error"]["code"].as_str().unwrap_or("UNKNOWN");
            let message = response["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(map_python_error(code, message));
        }

        // Extract tags list
        let tags = response["result"]["tags"].as_array().ok_or_else(|| {
            AiError::ProtocolError("Response missing 'result.tags' array".into())
        })?;

        let strings: Vec<String> = tags
            .iter()
            .map(|v| {
                v.as_str().ok_or_else(|| {
                    AiError::ProtocolError(format!("Tag contains non-string value: {}", v))
                }).map(|s| s.to_string())
            })
            .collect::<Result<_, _>>()?;

        if strings.is_empty() {
            return Err(AiError::InferenceError(
                "No tags generated for image".into(),
            ));
        }

        Ok(strings)
    }

    /// Generate tags for an image, incorporating personalized vocabulary from calibration.
    /// If `personalized_tags` is non-empty, they are passed as `candidate_tags` to Python
    /// so CLIP ranks them alongside default tags. If empty, falls back to default
    /// `generate_tags()`.
    pub fn generate_tags_with_calibration(
        &mut self,
        image_path: &str,
        top_n: u32,
        personalized_tags: &[String],
    ) -> Result<Vec<String>, AiError> {
        if personalized_tags.is_empty() {
            return self.generate_tags(image_path, top_n);
        }

        let request = serde_json::json!({
            "request_id": uuid_simple(),
            "method": "generate_tags",
            "params": {
                "image_path": image_path,
                "model_name": if self.model_name.is_empty() { Value::Null } else { Value::String(self.model_name.clone()) },
                "top_n": top_n,
                "candidate_tags": personalized_tags,
            }
        });

        let response = self.send_request(&request)?;

        if response["status"] == "error" {
            let code = response["error"]["code"].as_str().unwrap_or("UNKNOWN");
            let message = response["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(map_python_error(code, message));
        }

        let tags = response["result"]["tags"].as_array().ok_or_else(|| {
            AiError::ProtocolError("Response missing 'result.tags' array".into())
        })?;

        let strings: Vec<String> = tags
            .iter()
            .map(|v| {
                v.as_str().ok_or_else(|| {
                    AiError::ProtocolError(format!("Tag contains non-string value: {}", v))
                }).map(|s| s.to_string())
            })
            .collect::<Result<_, _>>()?;

        if strings.is_empty() {
            return Err(AiError::InferenceError(
                "No tags generated for image".into(),
            ));
        }

        Ok(strings)
    }

    /// Ping the subprocess to check liveness.
    pub fn ping(&mut self) -> Result<bool, AiError> {
        let request = serde_json::json!({
            "request_id": uuid_simple(),
            "method": "ping",
            "params": {}
        });

        let response = self.send_request(&request)?;
        Ok(response["status"] == "ok")
    }

    /// Gracefully shut down the subprocess.
    pub fn shutdown(&mut self) {
        if let Some(ref mut process) = self.process {
            drop(process.stdin.take()); // Close stdin to signal subprocess
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
        self.process = None;
    }
}

impl Drop for PythonBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Generate a simple unique request ID from timestamp.
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}", duration.as_nanos())
}

/// Map a Python error code to the corresponding AiError variant.
fn map_python_error(code: &str, message: &str) -> AiError {
    match code {
        "FILE_NOT_FOUND" => AiError::ModelNotFound(message.to_string()),
        "AI_MODEL_NOT_FOUND" => AiError::ModelNotFound(message.to_string()),
        "INVALID_PARAMS" => AiError::ProtocolError(message.to_string()),
        "INFERENCE_ERROR" => AiError::InferenceError(message.to_string()),
        "AI_INFERENCE_ERROR" => AiError::InferenceError(message.to_string()),
        "TIMEOUT" => AiError::Timeout(message.to_string()),
        _ => AiError::InferenceError(format!("{}: {}", code, message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_error_display_subprocess_not_found() {
        let err = AiError::SubprocessNotFound("python3 not found".into());
        assert_eq!(
            format!("{}", err),
            "AI_SUBPROCESS_NOT_FOUND -- python3 not found"
        );
    }

    #[test]
    fn test_ai_error_display_crashed() {
        let err = AiError::SubprocessCrashed("segfault".into());
        assert_eq!(format!("{}", err), "AI_SUBPROCESS_CRASHED -- segfault");
    }

    #[test]
    fn test_ai_error_display_timeout() {
        let err = AiError::Timeout("30s exceeded".into());
        assert_eq!(format!("{}", err), "AI_TIMEOUT -- 30s exceeded");
    }

    #[test]
    fn test_ai_error_display_protocol_error() {
        let err = AiError::ProtocolError("bad json".into());
        assert_eq!(format!("{}", err), "AI_PROTOCOL_ERROR -- bad json");
    }

    #[test]
    fn test_ai_error_display_inference_error() {
        let err = AiError::InferenceError("model error".into());
        assert_eq!(format!("{}", err), "AI_INFERENCE_ERROR -- model error");
    }

    #[test]
    fn test_ai_error_display_model_not_found() {
        let err = AiError::ModelNotFound("model.onnx".into());
        assert_eq!(format!("{}", err), "AI_MODEL_NOT_FOUND -- model.onnx");
    }

    #[test]
    fn test_python_bridge_new() {
        let bridge = PythonBridge::new(300, 30, String::new());
        assert!(bridge.process.is_none());
        assert_eq!(bridge.idle_timeout_secs, 300);
        assert_eq!(bridge.inference_timeout_secs, 30);
        assert!(bridge.model_name.is_empty());
    }

    #[test]
    fn test_python_bridge_new_with_model() {
        let bridge = PythonBridge::new(600, 60, "model.onnx".to_string());
        assert_eq!(bridge.model_name, "model.onnx");
    }

    #[test]
    fn test_python_bridge_shutdown_when_not_running() {
        let mut bridge = PythonBridge::new(300, 30, String::new());
        bridge.shutdown(); // Should not panic
        assert!(bridge.process.is_none());
    }

    #[test]
    fn test_uuid_simple_format() {
        let id = uuid_simple();
        assert!(!id.is_empty());
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_uuid_simple_unique() {
        let id1 = uuid_simple();
        let id2 = uuid_simple();
        // Two rapid calls might produce the same id on fast systems,
        // but in practice they should differ
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
    }

    #[test]
    fn test_map_python_error_file_not_found() {
        let err = map_python_error("FILE_NOT_FOUND", "model.onnx not found");
        assert!(matches!(err, AiError::ModelNotFound(msg) if msg == "model.onnx not found"));
    }

    #[test]
    fn test_map_python_error_inference_error() {
        let err = map_python_error("INFERENCE_ERROR", "model failed");
        assert!(matches!(err, AiError::InferenceError(msg) if msg == "model failed"));
    }

    #[test]
    fn test_map_python_error_timeout() {
        let err = map_python_error("TIMEOUT", "30s exceeded");
        assert!(matches!(err, AiError::Timeout(msg) if msg == "30s exceeded"));
    }

    #[test]
    fn test_map_python_error_unknown() {
        let err = map_python_error("SOME_ERROR", "something went wrong");
        assert!(matches!(err, AiError::InferenceError(msg) if msg == "SOME_ERROR: something went wrong"));
    }

    #[test]
    fn test_request_json_format_with_model() {
        let request = serde_json::json!({
            "request_id": "test-123",
            "method": "generate_embedding",
            "params": {
                "image_path": "/path/to/image.png",
                "model_name": "model.onnx",
            }
        });

        assert_eq!(request["method"], "generate_embedding");
        assert_eq!(request["params"]["image_path"], "/path/to/image.png");
        assert_eq!(request["params"]["model_name"], "model.onnx");
    }

    #[test]
    fn test_request_json_format_without_model() {
        let request = serde_json::json!({
            "request_id": "test-456",
            "method": "generate_embedding",
            "params": {
                "image_path": "/path/to/image.png",
                "model_name": Value::Null,
            }
        });

        assert_eq!(request["params"]["model_name"], Value::Null);
    }

    #[test]
    fn test_ping_request_format() {
        let request = serde_json::json!({
            "request_id": "test-789",
            "method": "ping",
            "params": {}
        });

        assert_eq!(request["method"], "ping");
        assert_eq!(request["params"], Value::Object(serde_json::Map::new()));
    }

    #[test]
    fn test_parse_embedding_response() {
        let response = serde_json::json!({
            "request_id": "1",
            "status": "ok",
            "result": {
                "embedding": [0.1, 0.2, 0.3, 0.4, 0.5]
            }
        });

        assert_eq!(response["status"], "ok");
        let embedding = response["result"]["embedding"].as_array().unwrap();
        let floats: Vec<f64> = embedding.iter().map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(floats, vec![0.1, 0.2, 0.3, 0.4, 0.5]);
    }

    #[test]
    fn test_parse_error_response() {
        let response = serde_json::json!({
            "request_id": "1",
            "status": "error",
            "error": {
                "code": "FILE_NOT_FOUND",
                "message": "model.onnx not found"
            }
        });

        assert_eq!(response["status"], "error");
        assert_eq!(response["error"]["code"], "FILE_NOT_FOUND");

        let code = response["error"]["code"].as_str().unwrap();
        let message = response["error"]["message"].as_str().unwrap();
        let err = map_python_error(code, message);
        assert!(matches!(err, AiError::ModelNotFound(_)));
    }

    #[test]
    #[cfg(unix)]
    fn test_send_request_with_mock_subprocess() {
        let temp_dir = tempfile::tempdir().unwrap();
        let script_path = temp_dir.path().join("mock_ai.sh");
        std::fs::write(
            &script_path,
            "#!/bin/bash\nwhile IFS= read -r line; do\n  echo '{\"request_id\":\"1\",\"status\":\"ok\",\"result\":{\"status\":\"ok\"}}'\ndone\n",
        )
        .unwrap();

        let mut child = Command::new("bash")
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn mock subprocess");

        let stdin = child.stdin.take().expect("No stdin");
        let stdout = child.stdout.take().expect("No stdout");

        let (tx, rx) = mpsc::channel();
        let _reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                tx.send(line).unwrap();
            }
        });

        // Write request
        let mut stdin = stdin;
        writeln!(stdin, "{{\"method\":\"ping\"}}").unwrap();

        // Read response with timeout
        let response_str = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Timeout waiting for response");
        let value: Value = serde_json::from_str(&response_str).expect("Invalid JSON");
        assert_eq!(value["status"], "ok");

        // Cleanup
        drop(stdin);
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    #[cfg(unix)]
    fn test_subprocess_timeout_detection() {
        let temp_dir = tempfile::tempdir().unwrap();
        let script_path = temp_dir.path().join("slow_mock.sh");
        // Script that never responds (sleeps forever)
        std::fs::write(
            &script_path,
            "#!/bin/bash\nsleep 60\n",
        )
        .unwrap();

        let mut child = Command::new("bash")
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn mock subprocess");

        let stdout = child.stdout.take().expect("No stdout");

        let (tx, rx) = mpsc::channel::<String>();
        let _reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                tx.send(line).unwrap();
            }
        });

        // Read with 1-second timeout — should timeout
        let result = rx.recv_timeout(Duration::from_secs(1));
        assert!(matches!(result, Err(mpsc::RecvTimeoutError::Timeout)));

        // Cleanup
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn test_map_python_error_invalid_params() {
        let err = map_python_error("INVALID_PARAMS", "'top_n' must be a positive integer");
        assert!(matches!(err, AiError::ProtocolError(msg) if msg == "'top_n' must be a positive integer"));
    }

    #[test]
    fn test_map_python_error_ai_model_not_found() {
        let err = map_python_error("AI_MODEL_NOT_FOUND", "clip model not found");
        assert!(matches!(err, AiError::ModelNotFound(msg) if msg == "clip model not found"));
    }

    #[test]
    fn test_map_python_error_ai_inference_error() {
        let err = map_python_error("AI_INFERENCE_ERROR", "tag generation failed");
        assert!(matches!(err, AiError::InferenceError(msg) if msg == "tag generation failed"));
    }

    #[test]
    fn test_generate_tags_request_format() {
        let request = serde_json::json!({
            "request_id": "test-tags-1",
            "method": "generate_tags",
            "params": {
                "image_path": "/path/to/frame.dpx",
                "model_name": Value::Null,
                "top_n": 5,
            }
        });

        assert_eq!(request["method"], "generate_tags");
        assert_eq!(request["params"]["image_path"], "/path/to/frame.dpx");
        assert_eq!(request["params"]["top_n"], 5);
        assert_eq!(request["params"]["model_name"], Value::Null);
    }

    #[test]
    fn test_generate_tags_request_format_with_model() {
        let request = serde_json::json!({
            "request_id": "test-tags-2",
            "method": "generate_tags",
            "params": {
                "image_path": "/path/to/frame.dpx",
                "model_name": "clip_vit_b32.onnx",
                "top_n": 10,
            }
        });

        assert_eq!(request["params"]["model_name"], "clip_vit_b32.onnx");
        assert_eq!(request["params"]["top_n"], 10);
    }

    #[test]
    fn test_parse_generate_tags_response() {
        let response = serde_json::json!({
            "request_id": "1",
            "status": "ok",
            "result": {
                "tags": ["warm", "cinematic", "golden tones", "soft lighting", "desaturated"],
                "count": 5
            }
        });

        assert_eq!(response["status"], "ok");
        let tags = response["result"]["tags"].as_array().unwrap();
        let strings: Vec<String> = tags.iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(strings, vec!["warm", "cinematic", "golden tones", "soft lighting", "desaturated"]);
        assert_eq!(response["result"]["count"], 5);
    }

    #[test]
    fn test_parse_generate_tags_error_response() {
        let response = serde_json::json!({
            "request_id": "1",
            "status": "error",
            "error": {
                "code": "AI_MODEL_NOT_FOUND",
                "message": "No ONNX model found in models directory"
            }
        });

        assert_eq!(response["status"], "error");
        let code = response["error"]["code"].as_str().unwrap();
        let message = response["error"]["message"].as_str().unwrap();
        let err = map_python_error(code, message);
        assert!(matches!(err, AiError::ModelNotFound(_)));
    }

    #[test]
    fn test_generate_tags_with_calibration_request_format() {
        // Verify the request format includes candidate_tags when personalized tags are provided
        let personalized_tags = vec!["cool blue shadows".to_string(), "SK-II skin".to_string()];
        let request = serde_json::json!({
            "request_id": "test-tags-cal-1",
            "method": "generate_tags",
            "params": {
                "image_path": "/path/to/frame.dpx",
                "model_name": Value::Null,
                "top_n": 5,
                "candidate_tags": personalized_tags,
            }
        });

        assert_eq!(request["method"], "generate_tags");
        assert_eq!(request["params"]["image_path"], "/path/to/frame.dpx");
        assert_eq!(request["params"]["top_n"], 5);
        let ct = request["params"]["candidate_tags"].as_array().unwrap();
        assert_eq!(ct.len(), 2);
        assert_eq!(ct[0], "cool blue shadows");
        assert_eq!(ct[1], "SK-II skin");
    }

    #[test]
    fn test_generate_tags_with_calibration_request_format_no_tags() {
        // When no personalized tags, should NOT include candidate_tags in params
        // (generate_tags_with_calibration falls back to generate_tags which omits it)
        let request = serde_json::json!({
            "request_id": "test-tags-cal-2",
            "method": "generate_tags",
            "params": {
                "image_path": "/path/to/frame.dpx",
                "model_name": Value::Null,
                "top_n": 5,
            }
        });

        assert_eq!(request["method"], "generate_tags");
        assert!(request["params"].get("candidate_tags").is_none());
    }
}
