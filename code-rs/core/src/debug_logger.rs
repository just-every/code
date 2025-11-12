use chrono::Local;
use code_otel::otel_event_manager::TurnLatencyPayload;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug)]
struct StreamInfo {
    response_file: PathBuf,
    events: Vec<Value>,
}

#[derive(Debug)]
pub struct DebugLogger {
    enabled: bool,
    log_dir: PathBuf,
    // Maps request_id to stream info for collecting events
    active_streams: Mutex<HashMap<String, StreamInfo>>,
    usage_dir: PathBuf,
    session_usage_file: Mutex<PathBuf>,
    turn_latency_dir: PathBuf,
    turn_latency_file: Mutex<Option<PathBuf>>,
}

impl DebugLogger {
    pub fn new(enabled: bool) -> Result<Self, std::io::Error> {
        if !enabled {
            return Ok(Self {
                enabled: false,
                log_dir: PathBuf::new(),
                active_streams: Mutex::new(HashMap::new()),
                usage_dir: PathBuf::new(),
                session_usage_file: Mutex::new(PathBuf::new()),
                turn_latency_dir: PathBuf::new(),
                turn_latency_file: Mutex::new(None),
            });
        }

        let mut log_dir = crate::config::find_code_home()?;
        log_dir.push("debug_logs");

        fs::create_dir_all(&log_dir)?;

        let mut usage_dir = log_dir.clone();
        usage_dir.push("usage");
        fs::create_dir_all(&usage_dir)?;

        let turn_latency_dir = log_dir.join("turn_latency");
        fs::create_dir_all(&turn_latency_dir)?;

        Ok(Self {
            enabled,
            log_dir,
            active_streams: Mutex::new(HashMap::new()),
            usage_dir,
            session_usage_file: Mutex::new(PathBuf::new()),
            turn_latency_dir,
            turn_latency_file: Mutex::new(None),
        })
    }

    fn ensure_log_dir(&self, tag: Option<&str>) -> Result<PathBuf, std::io::Error> {
        let Some(tag) = tag else {
            return Ok(self.log_dir.clone());
        };

        let mut dir = self.log_dir.clone();
        let mut applied = false;
        for segment in tag.split('/') {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut cleaned = String::with_capacity(trimmed.len());
            for ch in trimmed.chars() {
                let valid = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.');
                cleaned.push(if valid { ch } else { '_' });
            }
            if cleaned.is_empty() {
                continue;
            }
            dir.push(cleaned);
            applied = true;
        }

        if !applied {
            return Ok(self.log_dir.clone());
        }

        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Start a new request/response log file and return the request ID
    pub fn start_request_log(
        &self,
        endpoint: &str,
        payload: &Value,
        headers: Option<&Value>,
        tag: Option<&str>,
    ) -> Result<String, std::io::Error> {
        if !self.enabled {
            return Ok(String::new());
        }

        let log_dir = self.ensure_log_dir(tag)?;
        let timestamp = Local::now();
        let request_id = Uuid::new_v4().to_string();
        let request_id_short = &request_id[..8]; // Use first 8 chars of UUID for brevity

        // Create request file with pretty-printed JSON
        let request_filename = format!(
            "{}_{}_request.json",
            timestamp.format("%Y%m%d_%H%M%S%.3f"),
            request_id_short
        );
        let request_file_path = log_dir.join(request_filename);

        // Create request object with metadata
        let mut request_entry = serde_json::json!({
            "timestamp": timestamp.to_rfc3339(),
            "request_id": request_id,
            "endpoint": endpoint,
            "payload": payload
        });

        if let (Some(headers), Some(obj)) = (headers, request_entry.as_object_mut()) {
            obj.insert("headers".to_string(), headers.clone());
        }

        // Write pretty-printed JSON to request file
        let formatted_request = serde_json::to_string_pretty(&request_entry)?;
        fs::write(&request_file_path, formatted_request)?;

        // Prepare response file path
        let response_filename = format!(
            "{}_{}_response.json",
            timestamp.format("%Y%m%d_%H%M%S%.3f"),
            request_id_short
        );
        let response_file_path = log_dir.join(&response_filename);

        if let Ok(mut streams) = self.active_streams.lock() {
            streams.insert(
                request_id.clone(),
                StreamInfo {
                    response_file: response_file_path,
                    events: Vec::new(),
                },
            );
        }

        Ok(request_id)
    }

    /// Append a response event to the in-memory event list
    pub fn append_response_event(
        &self,
        request_id: &str,
        event_type: &str,
        data: &Value,
    ) -> Result<(), std::io::Error> {
        if !self.enabled || request_id.is_empty() {
            return Ok(());
        }

        if let Ok(mut streams) = self.active_streams.lock() {
            if let Some(stream_info) = streams.get_mut(request_id) {
                let timestamp = Local::now();
                let event_entry = serde_json::json!({
                    "timestamp": timestamp.to_rfc3339(),
                    "type": event_type,
                    "data": data
                });
                stream_info.events.push(event_entry);
            }
        }

        if let Some(response) = data.get("response") {
            if let Some(usage) = response.get("usage") {
                self.append_usage_entry(usage.clone())?;
            }
        } else if let Some(usage) = data.get("usage") {
            self.append_usage_entry(usage.clone())?;
        }

        Ok(())
    }

    /// Mark a stream as completed and write all collected events to response file
    pub fn end_request_log(&self, request_id: &str) -> Result<(), std::io::Error> {
        if !self.enabled || request_id.is_empty() {
            return Ok(());
        }

        if let Ok(mut streams) = self.active_streams.lock() {
            if let Some(stream_info) = streams.remove(request_id) {
                // Create the response object with all events as an array
                let response_data = serde_json::json!({
                    "request_id": request_id,
                    "completed_at": Local::now().to_rfc3339(),
                    "events": stream_info.events
                });

                // Write pretty-printed JSON to response file
                let formatted_response = serde_json::to_string_pretty(&response_data)?;
                fs::write(&stream_info.response_file, formatted_response)?;
            }
        }

        Ok(())
    }

    fn append_usage_entry(&self, usage: Value) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let path = {
            let guard = self
                .session_usage_file
                .lock()
                .expect("usage lock poisoned");
            if guard.as_os_str().is_empty() {
                return Ok(());
            }
            guard.clone()
        };

        let mut entries: Vec<Value> = if path.exists() {
            let contents = fs::read_to_string(&path)?;
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            Vec::new()
        };

        entries.push(usage);

        let formatted = serde_json::to_string_pretty(&entries)?;
        fs::write(path, formatted)?;
        Ok(())
    }

    pub fn set_session_usage_file(&self, session_id: &Uuid) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let session_id_str = session_id.to_string();
        let path = if let Some(existing) = self.find_existing_usage_log(&session_id_str) {
            existing
        } else {
            let timestamp = Local::now().format("%Y%m%d_%H%M%S%.3f");
            self
                .usage_dir
                .join(format!("{}_{}_usage.json", timestamp, session_id_str))
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !path.exists() {
            fs::write(&path, "[]")?;
        }

        let mut guard = self
            .session_usage_file
            .lock()
            .expect("usage lock poisoned");
        *guard = path;

        self.set_turn_latency_file(session_id)
    }

    fn set_turn_latency_file(&self, session_id: &Uuid) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let path = self
            .turn_latency_dir
            .join(format!("{}_turn_latency.jsonl", session_id));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Ensure file exists so tail -f works immediately.
        OpenOptions::new().create(true).append(true).open(&path)?;

        let mut guard = self
            .turn_latency_file
            .lock()
            .expect("turn latency lock poisoned");
        *guard = Some(path);

        Ok(())
    }

    pub fn log_turn_latency(&self, payload: &TurnLatencyPayload) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let path = {
            let guard = self
                .turn_latency_file
                .lock()
                .expect("turn latency lock poisoned");
            guard.clone()
        };

        let Some(path) = path else {
            return Ok(());
        };

        let payload_value = serde_json::to_value(payload).unwrap_or(Value::Null);
        let entry = match payload_value {
            Value::Object(mut map) => {
                map.insert(
                    "timestamp".to_string(),
                    Value::String(Local::now().to_rfc3339()),
                );
                Value::Object(map)
            }
            other => serde_json::json!({
                "timestamp": Local::now().to_rfc3339(),
                "payload": other,
            }),
        };

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{}", serde_json::to_string(&entry)?)?;
        Ok(())
    }

    fn find_existing_usage_log(&self, session_id: &str) -> Option<PathBuf> {
        if let Ok(entries) = fs::read_dir(&self.usage_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == format!("{}_usage.json", session_id)
                        || name.ends_with(&format!("_{}_usage.json", session_id))
                    {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    // Legacy methods for backward compatibility - they now create standalone files
    pub fn log_request(
        &self,
        endpoint: &str,
        payload: &Value,
        tag: Option<&str>,
    ) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let log_dir = self.ensure_log_dir(tag)?;
        let timestamp = Local::now();
        let filename = format!("{}_request.json", timestamp.format("%Y%m%d_%H%M%S%.3f"));

        let file_path = log_dir.join(filename);

        let log_entry = serde_json::json!({
            "timestamp": timestamp.to_rfc3339(),
            "type": "request",
            "endpoint": endpoint,
            "payload": payload
        });

        let formatted = serde_json::to_string_pretty(&log_entry)?;
        fs::write(file_path, formatted)?;

        Ok(())
    }

    pub fn log_response(
        &self,
        endpoint: &str,
        response: &Value,
        tag: Option<&str>,
    ) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let log_dir = self.ensure_log_dir(tag)?;
        let timestamp = Local::now();
        let filename = format!("{}_response.json", timestamp.format("%Y%m%d_%H%M%S%.3f"));

        let file_path = log_dir.join(filename);

        let log_entry = serde_json::json!({
            "timestamp": timestamp.to_rfc3339(),
            "type": "response",
            "endpoint": endpoint,
            "response": response
        });

        let formatted = serde_json::to_string_pretty(&log_entry)?;
        fs::write(file_path, formatted)?;

        Ok(())
    }

    pub fn log_stream_chunk(
        &self,
        endpoint: &str,
        chunk: &str,
        tag: Option<&str>,
    ) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let log_dir = self.ensure_log_dir(tag)?;
        let timestamp = Local::now();
        let filename = format!("{}_stream.txt", timestamp.format("%Y%m%d_%H%M%S%.3f"));

        let file_path = log_dir.join(filename);

        let log_entry = format!(
            "=== Stream Chunk at {} ===\nEndpoint: {}\n\n{}\n",
            timestamp.to_rfc3339(),
            endpoint,
            chunk
        );

        fs::write(file_path, log_entry)?;

        Ok(())
    }

    pub fn log_error(
        &self,
        endpoint: &str,
        error: &str,
        tag: Option<&str>,
    ) -> Result<(), std::io::Error> {
        if !self.enabled {
            return Ok(());
        }

        let log_dir = self.ensure_log_dir(tag)?;
        let timestamp = Local::now();
        let filename = format!("{}_error.txt", timestamp.format("%Y%m%d_%H%M%S%.3f"));

        let file_path = log_dir.join(filename);

        let log_entry = format!(
            "=== Error at {} ===\nEndpoint: {}\n\n{}\n",
            timestamp.to_rfc3339(),
            endpoint,
            error
        );

        fs::write(file_path, log_entry)?;

        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn log_sse_event(
        &self,
        endpoint: &str,
        event_data: &Value,
        tag: Option<&str>,
    ) -> Result<(), std::io::Error> {
        // Legacy method - now creates standalone files
        if !self.enabled {
            return Ok(());
        }

        let log_dir = self.ensure_log_dir(tag)?;
        let timestamp = Local::now();
        let filename = format!("{}_sse.json", timestamp.format("%Y%m%d_%H%M%S%.3f"));

        let file_path = log_dir.join(filename);

        let log_entry = serde_json::json!({
            "timestamp": timestamp.to_rfc3339(),
            "type": "sse_event",
            "endpoint": endpoint,
            "event": event_data
        });

        let formatted = serde_json::to_string_pretty(&log_entry)?;
        fs::write(file_path, formatted)?;

        Ok(())
    }
}
