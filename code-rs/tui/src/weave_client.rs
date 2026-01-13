#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeaveSession {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WeaveMessageKind {
    User,
    Reply,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeaveAgent {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeaveIncomingMessage {
    pub session_id: String,
    pub message_id: String,
    pub src: String,
    pub src_name: Option<String>,
    pub text: String,
    pub kind: WeaveMessageKind,
    pub conversation_id: String,
    pub conversation_owner: String,
    pub parent_message_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeaveMessageMetadata {
    pub conversation_id: String,
    pub conversation_owner: String,
    pub parent_message_id: Option<String>,
}

impl WeaveSession {
    pub(crate) fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| self.id.clone())
    }
}

impl WeaveAgent {
    pub(crate) fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| self.id.clone())
    }

    pub(crate) fn mention_text(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .filter(|name| !name.chars().any(char::is_whitespace))
            .map(ToString::to_string)
            .unwrap_or_else(|| self.id.clone())
    }
}

#[cfg(unix)]
mod platform {
    use super::WeaveAgent;
    use super::WeaveIncomingMessage;
    use super::WeaveSession;
    use chrono::SecondsFormat;
    use serde::Deserialize;
    use serde::Serialize;
    use serde_json::Value;
    use serde_json::json;
    use std::collections::HashMap;
    use std::env;
    use std::path::Path;
    use std::path::PathBuf;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;
    use tokio::io::ReadHalf;
    use tokio::io::WriteHalf;
    use tokio::net::UnixStream;
    use tokio::sync::mpsc;
    use tokio::sync::oneshot;
    use uuid::Uuid;

    const WEAVE_VERSION: u8 = 0;
    const COORD_SOCKET: &str = "coord.sock";
    const SESSIONS_DIR: &str = "sessions";
    const REQUEST_SRC: &str = "code-cli";

    #[derive(Debug, Serialize, Deserialize)]
    struct WeaveErrorDetail {
        code: String,
        message: String,
        #[serde(default)]
        detail: Option<Value>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct WeaveEnvelope {
        v: u8,
        #[serde(rename = "type")]
        r#type: String,
        id: String,
        ts: String,
        src: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        dst: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        topic: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        corr: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ack: Option<WeaveAck>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<WeaveErrorDetail>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct WeaveAck {
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(rename = "timeout_ms", skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<i32>,
    }

    #[derive(Debug, Deserialize)]
    struct SessionListPayload {
        sessions: Vec<SessionListEntry>,
    }

    #[derive(Debug, Deserialize)]
    struct SessionListEntry {
        id: String,
        name: String,
    }

    #[derive(Debug, Deserialize)]
    struct AgentListPayload {
        agents: Vec<AgentListEntry>,
    }

    #[derive(Debug, Deserialize)]
    struct AgentListEntry {
        id: String,
        #[serde(default)]
        name: Option<String>,
    }

    pub(crate) struct WeaveAgentConnection {
        session_id: String,
        agent_id: String,
        agent_name: String,
        outgoing_tx: mpsc::UnboundedSender<WeaveOutgoingRequest>,
        incoming_rx: Option<mpsc::UnboundedReceiver<WeaveIncomingMessage>>,
        shutdown_tx: Option<oneshot::Sender<()>>,
    }

    impl std::fmt::Debug for WeaveAgentConnection {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WeaveAgentConnection")
                .field("session_id", &self.session_id)
                .field("agent_id", &self.agent_id)
                .finish()
        }
    }

    impl WeaveAgentConnection {
        pub(crate) fn sender(&self) -> WeaveAgentSender {
            WeaveAgentSender {
                session_id: self.session_id.clone(),
                agent_id: self.agent_id.clone(),
                agent_name: self.agent_name.clone(),
                outgoing_tx: self.outgoing_tx.clone(),
            }
        }

        pub(crate) fn set_agent_name(&mut self, name: String) {
            self.agent_name = name;
        }

        pub(crate) fn take_incoming_rx(
            &mut self,
        ) -> Option<mpsc::UnboundedReceiver<WeaveIncomingMessage>> {
            self.incoming_rx.take()
        }

        pub(crate) fn shutdown(&mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }

    impl Drop for WeaveAgentConnection {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }

    pub(crate) async fn list_sessions() -> Result<Vec<WeaveSession>, String> {
        let socket_path = coord_socket_path(&resolve_weave_home()?);
        let request = new_envelope("session.list", None, None);
        let response = send_request(&socket_path, &request).await?;
        if let Some(message) = response_error(&response) {
            return Err(message);
        }
        let payload = response
            .payload
            .ok_or_else(|| "Weave session list response missing payload".to_string())?;
        let list: SessionListPayload = serde_json::from_value(payload)
            .map_err(|err| format!("Failed to parse Weave session list: {err}"))?;
        let sessions = list
            .sessions
            .into_iter()
            .map(|entry| {
                let trimmed = entry.name.trim();
                WeaveSession {
                    id: entry.id,
                    name: (!trimmed.is_empty()).then_some(trimmed.to_string()),
                }
            })
            .collect();
        Ok(sessions)
    }

    pub(crate) async fn create_session(name: Option<String>) -> Result<WeaveSession, String> {
        let socket_path = coord_socket_path(&resolve_weave_home()?);
        let payload = name.as_ref().map(|name| json!({ "name": name }));
        let request = new_envelope("session.create", None, payload);
        let response = send_request(&socket_path, &request).await?;
        if let Some(message) = response_error(&response) {
            return Err(message);
        }
        let session_id = response
            .session
            .ok_or_else(|| "Weave session.create response missing session id".to_string())?;
        Ok(WeaveSession {
            id: session_id,
            name,
        })
    }

    pub(crate) async fn close_session(session_id: &str) -> Result<(), String> {
        let weave_home = resolve_weave_home()?;
        let session_socket = session_socket_path(&weave_home, session_id);
        let socket_path = if session_socket.exists() {
            session_socket
        } else {
            coord_socket_path(&weave_home)
        };
        let request = new_envelope("session.close", Some(session_id.to_string()), None);
        let response = send_request(&socket_path, &request).await?;
        if let Some(message) = response_error(&response) {
            return Err(message);
        }
        Ok(())
    }

    pub(crate) async fn list_agents(session_id: &str, src: &str) -> Result<Vec<WeaveAgent>, String> {
        let weave_home = resolve_weave_home()?;
        let session_socket = session_socket_path(&weave_home, session_id);
        let socket_path = if session_socket.exists() {
            session_socket
        } else {
            coord_socket_path(&weave_home)
        };
        let request =
            new_envelope_with_src("agent.list", src.to_string(), Some(session_id.to_string()), None);
        let response = send_request(&socket_path, &request).await?;
        if let Some(message) = response_error(&response) {
            return Err(message);
        }
        let payload = response
            .payload
            .ok_or_else(|| "Weave agent list response missing payload".to_string())?;
        let list: AgentListPayload = serde_json::from_value(payload)
            .map_err(|err| format!("Failed to parse Weave agent list: {err}"))?;
        let agents = list
            .agents
            .into_iter()
            .map(|entry| {
                let name = entry
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(ToString::to_string);
                WeaveAgent { id: entry.id, name }
            })
            .collect();
        Ok(agents)
    }

    pub(crate) async fn connect_agent(
        session_id: String,
        agent_id: String,
        name: Option<String>,
    ) -> Result<WeaveAgentConnection, String> {
        let weave_home = resolve_weave_home()?;
        let session_socket = session_socket_path(&weave_home, &session_id);
        let socket_path = if session_socket.exists() {
            session_socket
        } else {
            coord_socket_path(&weave_home)
        };
        let stream = UnixStream::connect(&socket_path)
            .await
            .map_err(|err| format!("Failed to connect to Weave coordinator: {err}"))?;
        let (read_half, mut write_half) = tokio::io::split(stream);
        let mut reader = BufReader::new(read_half);
        let payload = agent_add_payload(&agent_id, name.as_deref());
        let request = new_envelope_with_src(
            "agent.add",
            agent_id.clone(),
            Some(session_id.clone()),
            Some(payload),
        );
        send_envelope(&mut write_half, &request).await?;
        let response = read_response(&mut reader, request.id.as_str()).await?;
        if let Some(message) = response_error(&response) {
            return Err(message);
        }
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let agent_name = name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| agent_id.clone());
        let session_id_for_task = session_id.clone();
        let agent_id_for_task = agent_id.clone();
        let agent_name_for_task = agent_name.clone();
        let state = AgentConnectionState {
            session_id: session_id_for_task,
            agent_id: agent_id_for_task,
            agent_name: agent_name_for_task,
            outgoing_rx,
            incoming_tx,
        };
        let _task = tokio::spawn(async move {
            hold_agent_connection(reader, write_half, shutdown_rx, state).await;
        });
        Ok(WeaveAgentConnection {
            session_id,
            agent_id,
            agent_name,
            outgoing_tx,
            incoming_rx: Some(incoming_rx),
            shutdown_tx: Some(shutdown_tx),
        })
    }

    fn resolve_weave_home() -> Result<PathBuf, String> {
        if let Ok(value) = env::var("WEAVE_HOME") {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err("WEAVE_HOME is set but empty".to_string());
            }
            return expand_home(trimmed);
        }
        let home = dirs::home_dir().ok_or_else(|| "Failed to resolve home directory".to_string())?;
        Ok(home.join(".weave"))
    }

    fn expand_home(path: &str) -> Result<PathBuf, String> {
        if path == "~" || path.starts_with("~/") {
            let home =
                dirs::home_dir().ok_or_else(|| "Failed to resolve home directory".to_string())?;
            if path == "~" {
                return Ok(home);
            }
            return Ok(home.join(&path[2..]));
        }
        Ok(PathBuf::from(path))
    }

    fn coord_socket_path(weave_home: &Path) -> PathBuf {
        weave_home.join(COORD_SOCKET)
    }

    fn session_socket_path(weave_home: &Path, session_id: &str) -> PathBuf {
        weave_home.join(SESSIONS_DIR).join(session_id).join(COORD_SOCKET)
    }

    fn new_envelope(req_type: &str, session: Option<String>, payload: Option<Value>) -> WeaveEnvelope {
        new_envelope_with_src(req_type, REQUEST_SRC.to_string(), session, payload)
    }

    fn new_envelope_with_src(
        req_type: &str,
        src: String,
        session: Option<String>,
        payload: Option<Value>,
    ) -> WeaveEnvelope {
        WeaveEnvelope {
            v: WEAVE_VERSION,
            r#type: req_type.to_string(),
            id: Uuid::new_v4().to_string(),
            ts: now_timestamp(),
            src,
            dst: None,
            topic: None,
            session,
            corr: None,
            payload,
            ack: None,
            status: None,
            error: None,
        }
    }

    fn now_timestamp() -> String {
        chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
    }

    fn response_error(response: &WeaveEnvelope) -> Option<String> {
        if response.status.as_deref() != Some("error") {
            return None;
        }
        let fallback = "Weave request failed".to_string();
        let Some(detail) = response.error.as_ref() else {
            return Some(fallback);
        };
        let code = detail.code.trim();
        let message = detail.message.trim();
        match (code.is_empty(), message.is_empty()) {
            (false, false) => Some(format!("{code}: {message}")),
            (false, true) => Some(code.to_string()),
            (true, false) => Some(message.to_string()),
            (true, true) => Some(fallback),
        }
    }

    async fn send_request(socket_path: &Path, request: &WeaveEnvelope) -> Result<WeaveEnvelope, String> {
        if !socket_path.exists() {
            return Err(format!(
                "Weave coordinator socket not found at {}",
                socket_path.display()
            ));
        }
        let mut stream = UnixStream::connect(socket_path)
            .await
            .map_err(|err| format!("Failed to connect to Weave coordinator: {err}"))?;
        let payload = serde_json::to_vec(request)
            .map_err(|err| format!("Failed to serialize Weave request: {err}"))?;
        stream
            .write_all(&payload)
            .await
            .map_err(|err| format!("Failed to write Weave request: {err}"))?;
        stream
            .write_all(b"\n")
            .await
            .map_err(|err| format!("Failed to write Weave request: {err}"))?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader
                .read_line(&mut line)
                .await
                .map_err(|err| format!("Failed to read Weave response: {err}"))?;
            if bytes == 0 {
                return Err("Weave coordinator closed the connection".to_string());
            }
            let response: WeaveEnvelope = serde_json::from_str(line.trim_end())
                .map_err(|err| format!("Failed to parse Weave response: {err}"))?;
            if response.corr.as_deref() == Some(request.id.as_str()) {
                return Ok(response);
            }
        }
    }

    async fn send_envelope(writer: &mut WriteHalf<UnixStream>, request: &WeaveEnvelope) -> Result<(), String> {
        let payload = serde_json::to_vec(request)
            .map_err(|err| format!("Failed to serialize Weave request: {err}"))?;
        writer
            .write_all(&payload)
            .await
            .map_err(|err| format!("Failed to write Weave request: {err}"))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|err| format!("Failed to write Weave request: {err}"))?;
        Ok(())
    }

    async fn read_response(
        reader: &mut BufReader<ReadHalf<UnixStream>>,
        request_id: &str,
    ) -> Result<WeaveEnvelope, String> {
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader
                .read_line(&mut line)
                .await
                .map_err(|err| format!("Failed to read Weave response: {err}"))?;
            if bytes == 0 {
                return Err("Weave coordinator closed the connection".to_string());
            }
            let response: WeaveEnvelope = serde_json::from_str(line.trim_end())
                .map_err(|err| format!("Failed to parse Weave response: {err}"))?;
            if response.corr.as_deref() == Some(request_id) {
                return Ok(response);
            }
        }
    }

    fn agent_add_payload(agent_id: &str, name: Option<&str>) -> Value {
        let trimmed = name.map(str::trim).filter(|name| !name.is_empty());
        match trimmed {
            Some(name) => json!({ "id": agent_id, "name": name }),
            None => json!({ "id": agent_id }),
        }
    }

    fn agent_update_payload(agent_id: &str, name: &str) -> Value {
        json!({ "id": agent_id, "name": name })
    }

    fn message_payload(
        text: String,
        sender_name: &str,
        kind: Option<&str>,
        metadata: Option<&super::WeaveMessageMetadata>,
    ) -> Value {
        let mut codex = serde_json::Map::new();
        codex.insert("sender_name".to_string(), json!(sender_name));
        if let Some(kind) = kind {
            codex.insert("kind".to_string(), json!(kind));
        }
        if let Some(metadata) = metadata {
            codex.insert("conversation_id".to_string(), json!(metadata.conversation_id));
            codex.insert(
                "conversation_owner".to_string(),
                json!(metadata.conversation_owner),
            );
            if let Some(parent_message_id) = metadata
                .parent_message_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                codex.insert("parent_message_id".to_string(), json!(parent_message_id));
            }
        }
        json!({
            "text": text,
            "codex": Value::Object(codex),
        })
    }

    #[derive(Debug)]
    struct WeaveOutgoingRequest {
        envelope: WeaveEnvelope,
        response_tx: oneshot::Sender<Result<WeaveEnvelope, String>>,
    }

    struct AgentConnectionState {
        session_id: String,
        agent_id: String,
        agent_name: String,
        outgoing_rx: mpsc::UnboundedReceiver<WeaveOutgoingRequest>,
        incoming_tx: mpsc::UnboundedSender<WeaveIncomingMessage>,
    }

    #[derive(Clone, Debug)]
    pub(crate) struct WeaveAgentSender {
        session_id: String,
        agent_id: String,
        agent_name: String,
        outgoing_tx: mpsc::UnboundedSender<WeaveOutgoingRequest>,
    }

    impl WeaveAgentSender {
        pub(crate) async fn send_message_with_metadata(
            &self,
            dst: String,
            text: String,
            metadata: Option<&super::WeaveMessageMetadata>,
            message_id: Option<String>,
        ) -> Result<(), String> {
            let payload = message_payload(text, &self.agent_name, None, metadata);
            self.send_payload(dst, payload, message_id).await
        }

        pub(crate) async fn send_reply_with_metadata(
            &self,
            dst: String,
            text: String,
            metadata: Option<&super::WeaveMessageMetadata>,
            message_id: Option<String>,
        ) -> Result<(), String> {
            let payload = message_payload(text, &self.agent_name, Some("reply"), metadata);
            self.send_payload(dst, payload, message_id).await
        }

        pub(crate) async fn update_agent_name(&self, name: String) -> Result<(), String> {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err("Weave agent name is empty".to_string());
            }
            let payload = agent_update_payload(&self.agent_id, trimmed);
            let request = new_envelope_with_src(
                "agent.update",
                self.agent_id.clone(),
                Some(self.session_id.clone()),
                Some(payload),
            );
            let response = self.send_request(request).await?;
            if let Some(message) = response_error(&response) {
                return Err(message);
            }
            Ok(())
        }

        async fn send_payload(
            &self,
            dst: String,
            payload: Value,
            message_id: Option<String>,
        ) -> Result<(), String> {
            let mut request = new_envelope_with_src(
                "message.send",
                self.agent_id.clone(),
                Some(self.session_id.clone()),
                Some(payload),
            );
            if let Some(message_id) = message_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                request.id = message_id.to_string();
            }
            request.dst = Some(dst);
            let response = self.send_request(request).await?;
            if let Some(message) = response_error(&response) {
                return Err(message);
            }
            Ok(())
        }

        async fn send_request(&self, request: WeaveEnvelope) -> Result<WeaveEnvelope, String> {
            let (response_tx, response_rx) = oneshot::channel();
            self.outgoing_tx
                .send(WeaveOutgoingRequest {
                    envelope: request,
                    response_tx,
                })
                .map_err(|_| "Weave agent connection closed".to_string())?;
            response_rx
                .await
                .map_err(|_| "Weave agent connection closed".to_string())?
        }
    }

    async fn hold_agent_connection(
        mut reader: BufReader<ReadHalf<UnixStream>>,
        mut writer: WriteHalf<UnixStream>,
        mut shutdown_rx: oneshot::Receiver<()>,
        state: AgentConnectionState,
    ) {
        let AgentConnectionState {
            session_id,
            agent_id,
            agent_name,
            mut outgoing_rx,
            incoming_tx,
        } = state;
        let mut line = String::new();
        let mut pending: HashMap<String, oneshot::Sender<Result<WeaveEnvelope, String>>> =
            HashMap::new();
        loop {
            line.clear();
            tokio::select! {
                _ = &mut shutdown_rx => {
                    let request = new_envelope_with_src(
                        "agent.remove",
                        agent_id.clone(),
                        Some(session_id.clone()),
                        Some(json!({ "id": agent_id.clone() })),
                    );
                    let _ = send_envelope(&mut writer, &request).await;
                    let _ = read_response(&mut reader, request.id.as_str()).await;
                    break;
                }
                request = outgoing_rx.recv() => {
                    let Some(request) = request else {
                        break;
                    };
                    let request_id = request.envelope.id.clone();
                    pending.insert(request_id.clone(), request.response_tx);
                    if let Err(err) = send_envelope(&mut writer, &request.envelope).await
                        && let Some(response_tx) = pending.remove(&request_id) {
                            let _ = response_tx.send(Err(err));
                        }
                }
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => break,
                        Ok(_) => {
                            let response: WeaveEnvelope = match serde_json::from_str(line.trim_end()) {
                                Ok(response) => response,
                                Err(_) => continue,
                            };
                            if let Some(corr) = response.corr.as_deref()
                                && let Some(response_tx) = pending.remove(corr)
                            {
                                let _ = response_tx.send(Ok(response));
                                continue;
                            }
                            if response.r#type == "message.send" {
                                if let Some(message) =
                                    build_incoming_message(&response, &agent_id, &agent_name)
                                {
                                    let _ = incoming_tx.send(message);
                                }
                                if response.ack.as_ref().and_then(|ack| ack.mode.as_deref()) == Some("auto")
                                {
                                    let mut ack_request = new_envelope_with_src(
                                        "message.ack",
                                        agent_id.clone(),
                                        Some(session_id.clone()),
                                        Some(json!({ "acked": response.id.clone() })),
                                    );
                                    ack_request.corr = Some(response.id.clone());
                                    let _ = send_envelope(&mut writer, &ack_request).await;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        for (_, response_tx) in pending {
            let _ = response_tx.send(Err("Weave agent connection closed".to_string()));
        }
    }

    fn build_incoming_message(
        envelope: &WeaveEnvelope,
        agent_id: &str,
        agent_name: &str,
    ) -> Option<WeaveIncomingMessage> {
        if envelope.src == agent_id || envelope.src == agent_name {
            return None;
        }
        if !is_direct_message_for_agent(envelope, agent_id) {
            return None;
        }
        let text = payload_text(envelope.payload.as_ref())?;
        let kind = payload_kind(envelope.payload.as_ref());
        let src_name = payload_sender_name(envelope.payload.as_ref());
        let session_id = envelope.session.as_ref()?.clone();
        let src = envelope.src.clone();
        let message_id = envelope.id.clone();
        let conversation_id = payload_conversation_id(envelope.payload.as_ref())
            .unwrap_or_else(|| message_id.clone());
        let conversation_owner =
            payload_conversation_owner(envelope.payload.as_ref()).unwrap_or_else(|| src.clone());
        let parent_message_id = payload_parent_message_id(envelope.payload.as_ref());
        Some(WeaveIncomingMessage {
            session_id,
            message_id,
            src,
            src_name,
            text,
            kind,
            conversation_id,
            conversation_owner,
            parent_message_id,
        })
    }

    fn is_direct_message_for_agent(envelope: &WeaveEnvelope, agent_id: &str) -> bool {
        if envelope.dst.as_deref() == Some(agent_id) {
            return true;
        }
        let Some(topic) = envelope.topic.as_deref() else {
            return false;
        };
        topic == format!("agent.{agent_id}.inbox")
    }

    fn payload_text(payload: Option<&Value>) -> Option<String> {
        let payload = payload?;
        if let Some(text) = payload.as_str() {
            return Some(text.to_string());
        }
        if let Some(map) = payload.as_object() {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return Some(text.to_string());
            }
            if let Some(input) = map.get("input").and_then(Value::as_object)
                && let Some(text) = input.get("text").and_then(Value::as_str)
            {
                return Some(text.to_string());
            }
        }
        let rendered = payload.to_string();
        if rendered.is_empty() {
            None
        } else {
            Some(rendered)
        }
    }

    fn payload_kind(payload: Option<&Value>) -> super::WeaveMessageKind {
        let Some(payload) = payload else {
            return super::WeaveMessageKind::User;
        };
        let Some(map) = payload.as_object() else {
            return super::WeaveMessageKind::User;
        };
        let Some(codex) = map.get("codex").and_then(Value::as_object) else {
            return super::WeaveMessageKind::User;
        };
        let Some(kind) = codex.get("kind").and_then(Value::as_str) else {
            return super::WeaveMessageKind::User;
        };
        if kind == "reply" {
            super::WeaveMessageKind::Reply
        } else {
            super::WeaveMessageKind::User
        }
    }

    fn payload_sender_name(payload: Option<&Value>) -> Option<String> {
        let payload = payload?;
        let map = payload.as_object()?;
        let codex = map.get("codex")?.as_object()?;
        let name = codex.get("sender_name")?.as_str()?;
        let name = name.trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    }

    fn payload_conversation_id(payload: Option<&Value>) -> Option<String> {
        let payload = payload?;
        let map = payload.as_object()?;
        let codex = map.get("codex")?.as_object()?;
        let id = codex.get("conversation_id")?.as_str()?;
        let id = id.trim();
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }

    fn payload_conversation_owner(payload: Option<&Value>) -> Option<String> {
        let payload = payload?;
        let map = payload.as_object()?;
        let codex = map.get("codex")?.as_object()?;
        let owner = codex.get("conversation_owner")?.as_str()?;
        let owner = owner.trim();
        if owner.is_empty() {
            None
        } else {
            Some(owner.to_string())
        }
    }

    fn payload_parent_message_id(payload: Option<&Value>) -> Option<String> {
        let payload = payload?;
        let map = payload.as_object()?;
        let codex = map.get("codex")?.as_object()?;
        let parent = codex.get("parent_message_id")?.as_str()?;
        let parent = parent.trim();
        if parent.is_empty() {
            None
        } else {
            Some(parent.to_string())
        }
    }
}

#[cfg(not(unix))]
mod platform {
    use super::WeaveAgent;
    use super::WeaveIncomingMessage;
    use super::WeaveSession;
    use tokio::sync::mpsc;

    pub(crate) struct WeaveAgentConnection;

    #[derive(Clone, Debug)]
    pub(crate) struct WeaveAgentSender;

    impl std::fmt::Debug for WeaveAgentConnection {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WeaveAgentConnection").finish()
        }
    }

    impl WeaveAgentConnection {
        pub(crate) fn sender(&self) -> WeaveAgentSender {
            WeaveAgentSender
        }

        pub(crate) fn set_agent_name(&mut self, _name: String) {}

        pub(crate) fn shutdown(&mut self) {}

        pub(crate) fn take_incoming_rx(
            &mut self,
        ) -> Option<mpsc::UnboundedReceiver<WeaveIncomingMessage>> {
            None
        }
    }

    impl WeaveAgentSender {
        pub(crate) async fn send_message_with_metadata(
            &self,
            _dst: String,
            _text: String,
            _metadata: Option<&super::WeaveMessageMetadata>,
            _message_id: Option<String>,
        ) -> Result<(), String> {
            Err("Weave sessions are only supported on Unix platforms.".to_string())
        }

        pub(crate) async fn send_reply_with_metadata(
            &self,
            _dst: String,
            _text: String,
            _metadata: Option<&super::WeaveMessageMetadata>,
            _message_id: Option<String>,
        ) -> Result<(), String> {
            Err("Weave sessions are only supported on Unix platforms.".to_string())
        }

        pub(crate) async fn update_agent_name(&self, _name: String) -> Result<(), String> {
            Err("Weave sessions are only supported on Unix platforms.".to_string())
        }
    }

    pub(crate) async fn list_sessions() -> Result<Vec<WeaveSession>, String> {
        Err("Weave sessions are only supported on Unix platforms.".to_string())
    }

    pub(crate) async fn create_session(_name: Option<String>) -> Result<WeaveSession, String> {
        Err("Weave sessions are only supported on Unix platforms.".to_string())
    }

    pub(crate) async fn close_session(_session_id: &str) -> Result<(), String> {
        Err("Weave sessions are only supported on Unix platforms.".to_string())
    }

    pub(crate) async fn list_agents(_session_id: &str, _src: &str) -> Result<Vec<WeaveAgent>, String> {
        Err("Weave sessions are only supported on Unix platforms.".to_string())
    }

    pub(crate) async fn connect_agent(
        _session_id: String,
        _agent_id: String,
        _name: Option<String>,
    ) -> Result<WeaveAgentConnection, String> {
        Err("Weave sessions are only supported on Unix platforms.".to_string())
    }
}

pub(crate) use platform::WeaveAgentConnection;
pub(crate) use platform::close_session;
pub(crate) use platform::connect_agent;
pub(crate) use platform::create_session;
pub(crate) use platform::list_agents;
pub(crate) use platform::list_sessions;
