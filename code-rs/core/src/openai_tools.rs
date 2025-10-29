use serde::Deserialize;
use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::agent_tool::create_agent_tool;
use crate::model_family::ModelFamily;
use crate::plan_tool::PLAN_TOOL;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use crate::tool_apply_patch::ApplyPatchToolType;
// apply_patch tools are not currently surfaced; keep imports out to avoid warnings.

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResponsesApiTool {
    pub(crate) name: String,
    pub(crate) description: String,
    /// TODO: Validation. When strict is set to true, the JSON schema,
    /// `required` and `additional_properties` must be present. All fields in
    /// `properties` must be present in `required`.
    pub(crate) strict: bool,
    pub(crate) parameters: JsonSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreeformTool {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) format: FreeformToolFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreeformToolFormat {
    pub(crate) r#type: String,
    pub(crate) syntax: String,
    pub(crate) definition: String,
}

/// When serialized as JSON, this produces a valid "Tool" in the OpenAI
/// Responses API.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum OpenAiTool {
    #[serde(rename = "function")]
    Function(ResponsesApiTool),
    #[serde(rename = "local_shell")]
    LocalShell {},
    /// Native Responses API web search tool. Optional fields like `filters`
    /// are serialized alongside the type discriminator.
    #[serde(rename = "web_search")]
    WebSearch(WebSearchTool),
    #[serde(rename = "custom")]
    Freeform(FreeformTool),
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct WebSearchTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<WebSearchFilters>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct WebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum ConfigShellToolType {
    DefaultShell,
    ShellWithRequest { sandbox_policy: SandboxPolicy },
    LocalShell,
    StreamableShell,
}

#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub shell_type: ConfigShellToolType,
    pub plan_tool: bool,
    #[allow(dead_code)]
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
    pub web_search_request: bool,
    #[allow(dead_code)]
    pub include_view_image_tool: bool,
    pub web_search_allowed_domains: Option<Vec<String>>,
    pub agent_model_allowed_values: Vec<String>,
    pub skill_tool: bool,
}

#[allow(dead_code)]
pub(crate) struct ToolsConfigParams<'a> {
    pub(crate) model_family: &'a ModelFamily,
    pub(crate) approval_policy: AskForApproval,
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) include_plan_tool: bool,
    pub(crate) include_apply_patch_tool: bool,
    pub(crate) include_web_search_request: bool,
    pub(crate) use_streamable_shell_tool: bool,
    pub(crate) include_view_image_tool: bool,
}

impl ToolsConfig {
    pub fn new(
        model_family: &ModelFamily,
        approval_policy: AskForApproval,
        sandbox_policy: SandboxPolicy,
        include_plan_tool: bool,
        include_apply_patch_tool: bool,
        include_web_search_request: bool,
        _use_streamable_shell_tool: bool,
        include_view_image_tool: bool,
    ) -> Self {
        // Our fork does not yet enable the experimental streamable shell tool
        // in the tool selection phase. Default to the existing behaviors.
        let use_streamable_shell_tool = false;
        let mut shell_type = if use_streamable_shell_tool {
            ConfigShellToolType::StreamableShell
        } else if model_family.uses_local_shell_tool {
            ConfigShellToolType::LocalShell
        } else {
            ConfigShellToolType::DefaultShell
        };
        if matches!(approval_policy, AskForApproval::OnRequest) && !use_streamable_shell_tool {
            shell_type = ConfigShellToolType::ShellWithRequest {
                sandbox_policy: sandbox_policy.clone(),
            }
        }

        let apply_patch_tool_type = if include_apply_patch_tool {
            model_family.apply_patch_tool_type.clone()
        } else {
            None
        };

        Self {
            shell_type,
            plan_tool: include_plan_tool,
            apply_patch_tool_type,
            web_search_request: include_web_search_request,
            include_view_image_tool,
            web_search_allowed_domains: None,
            agent_model_allowed_values: Vec::new(),
            skill_tool: false,
        }
    }

    // Compatibility constructor used by some tests/upstream calls.
    #[allow(dead_code)]
    pub(crate) fn new_from_params(p: &ToolsConfigParams) -> Self {
        Self::new(
            p.model_family,
            p.approval_policy,
            p.sandbox_policy.clone(),
            p.include_plan_tool,
            p.include_apply_patch_tool,
            p.include_web_search_request,
            p.use_streamable_shell_tool,
            p.include_view_image_tool,
        )
    }
}

impl ToolsConfig {
    pub fn set_agent_models(&mut self, models: Vec<String>) {
        self.agent_model_allowed_values = models;
    }

    pub fn agent_models(&self) -> &[String] {
        &self.agent_model_allowed_values
    }

    pub fn enable_skill_tool(&mut self, enabled: bool) {
        self.skill_tool = enabled;
    }
}

/// Whether additional properties are allowed, and if so, any required schema
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<JsonSchema>),
}

impl From<bool> for AdditionalProperties {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

impl From<JsonSchema> for AdditionalProperties {
    fn from(s: JsonSchema) -> Self {
        Self::Schema(Box::new(s))
    }
}

/// Generic JSON‑Schema subset needed for our tool definitions
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum JsonSchema {
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "enum")]
        allowed_values: Option<Vec<String>>,
    },
    /// MCP schema allows "number" | "integer" for Number
    #[serde(alias = "integer")]
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Array {
        items: Box<JsonSchema>,

        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Object {
        properties: BTreeMap<String, JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<Vec<String>>,
        #[serde(
            rename = "additionalProperties",
            skip_serializing_if = "Option::is_none"
        )]
        additional_properties: Option<AdditionalProperties>,
    },
}

impl Serialize for JsonSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            JsonSchema::Boolean { description } => {
                let mut state = serializer.serialize_struct("JsonSchema", if description.is_some() { 2 } else { 1 })?;
                state.serialize_field("type", "boolean")?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                state.end()
            }
            JsonSchema::String {
                description,
                allowed_values,
            } => {
                let mut fields = 1;
                if description.is_some() {
                    fields += 1;
                }
                if allowed_values.is_some() {
                    fields += 1;
                }
                let mut state = serializer.serialize_struct("JsonSchema", fields)?;
                state.serialize_field("type", "string")?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                if let Some(values) = allowed_values {
                    state.serialize_field("enum", values)?;
                }
                state.end()
            }
            JsonSchema::Number { description } => {
                let mut state = serializer.serialize_struct("JsonSchema", if description.is_some() { 2 } else { 1 })?;
                state.serialize_field("type", "number")?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                state.end()
            }
            JsonSchema::Array { items, description } => {
                let mut fields = 2; // type + items
                if description.is_some() {
                    fields += 1;
                }
                let mut state = serializer.serialize_struct("JsonSchema", fields)?;
                state.serialize_field("type", "array")?;
                state.serialize_field("items", items)?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                state.end()
            }
            JsonSchema::Object {
                properties,
                required,
                additional_properties,
            } => {
                let req: Vec<String> = match required {
                    Some(explicit) => explicit.clone(),
                    None => properties.keys().cloned().collect(),
                };
                let mut fields = 3; // type, properties, required
                if additional_properties.is_some() {
                    fields += 1;
                }
                let mut state = serializer.serialize_struct("JsonSchema", fields)?;
                state.serialize_field("type", "object")?;
                state.serialize_field("properties", properties)?;
                state.serialize_field("required", &req)?;
                if let Some(additional) = additional_properties {
                    state.serialize_field("additionalProperties", additional)?;
                }
                state.end()
            }
        }
    }
}

fn create_shell_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("The command to execute".to_string()),
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout".to_string(),
        JsonSchema::Number {
            description: Some("Optional hard timeout in milliseconds (minimum 1,800,000 / 30 minutes). By default, commands have no hard timeout; long runs are streamed and may be backgrounded by the agent.".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_string(),
        description: "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks. Optional `timeout` can set a hard kill if needed.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

fn create_shell_tool_for_sandbox(sandbox_policy: &SandboxPolicy) -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "command".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some("The command to execute".to_string()),
        },
    );
    properties.insert(
        "workdir".to_string(),
        JsonSchema::String {
            description: Some("The working directory to execute the command in".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some("Optional hard timeout in milliseconds (minimum 1,800,000 / 30 minutes). By default, commands have no hard timeout; long runs are streamed and may be backgrounded by the agent.".to_string()),
        },
    );

    if matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }) {
        properties.insert(
            "with_escalated_permissions".to_string(),
            JsonSchema::Boolean {
                description: Some("Whether to request escalated permissions. Set to true if command needs to be run without sandbox restrictions".to_string()),
            },
        );
        properties.insert(
            "justification".to_string(),
            JsonSchema::String {
                description: Some("Only set if with_escalated_permissions is true. 1-sentence explanation of why we want to run this command.".to_string()),
                allowed_values: None,
            },
        );
    }

    let description = match sandbox_policy {
        SandboxPolicy::WorkspaceWrite {
            network_access,
            writable_roots,
            ..
        } => {
            let roots_str = if writable_roots.is_empty() {
                "    - (none)\n".to_string()
            } else {
                writable_roots
                    .iter()
                    .map(|p| format!("    - {}\n", p.display()))
                    .collect()
            };
            format!(
                r#"
The shell tool is used to execute shell commands.
- When invoking the shell tool, your call will be running in a sandbox, and some shell commands will require escalated privileges:
  - Types of actions that require escalated privileges:
    - Writing files other than those in the writable roots
      - writable roots:
{}{}
  - Examples of commands that require escalated privileges:
    - git commit
    - npm install or pnpm install
    - cargo build
    - cargo test
- When invoking a command that will require escalated privileges:
  - Provide the with_escalated_permissions parameter with the boolean value true
  - Include a short, 1 sentence explanation for why we need to run with_escalated_permissions in the justification parameter.

Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks. Optional `timeout` can set a hard kill if needed."#,
                roots_str,
                if !network_access {
                    "\n    - Commands that require network access\n"
                } else {
                    ""
                }
            )
        }
        SandboxPolicy::DangerFullAccess => {
            "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks.".to_string()
        }
        SandboxPolicy::ReadOnly => {
            "Runs a shell command and returns its output. Output streams live to the UI. Long-running commands may be backgrounded after an initial window. Use `wait` to await background tasks.".to_string()
        }
    };

    OpenAiTool::Function(ResponsesApiTool {
        name: "shell".to_string(),
        description,
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

/// Returns JSON values that are compatible with Function Calling in the
/// Responses API:
/// https://platform.openai.com/docs/guides/function-calling?api-mode=responses
pub fn create_tools_json_for_responses_api(
    tools: &[OpenAiTool],
) -> crate::error::Result<Vec<serde_json::Value>> {
    let mut tools_json = Vec::new();

    for tool in tools {
        let json = serde_json::to_value(tool)?;
        tools_json.push(json);
    }

    Ok(tools_json)
}
/// Returns JSON values that are compatible with Function Calling in the
/// Chat Completions API:
/// https://platform.openai.com/docs/guides/function-calling?api-mode=chat
pub(crate) fn create_tools_json_for_chat_completions_api(
    tools: &[OpenAiTool],
) -> crate::error::Result<Vec<serde_json::Value>> {
    // We start with the JSON for the Responses API and than rewrite it to match
    // the chat completions tool call format.
    let responses_api_tools_json = create_tools_json_for_responses_api(tools)?;
    let tools_json = responses_api_tools_json
        .into_iter()
        .filter_map(|mut tool| {
            if tool.get("type") != Some(&serde_json::Value::String("function".to_string())) {
                return None;
            }

            if let Some(map) = tool.as_object_mut() {
                // Remove "type" field as it is not needed in chat completions.
                map.remove("type");
                Some(json!({
                    "type": "function",
                    "function": map,
                }))
            } else {
                None
            }
        })
        .collect::<Vec<serde_json::Value>>();
    Ok(tools_json)
}

pub(crate) fn mcp_tool_to_openai_tool(
    fully_qualified_name: String,
    tool: mcp_types::Tool,
) -> Result<ResponsesApiTool, serde_json::Error> {
    let mcp_types::Tool {
        description,
        mut input_schema,
        ..
    } = tool;

    // OpenAI models mandate the "properties" field in the schema. The Agents
    // SDK fixed this by inserting an empty object for "properties" if it is not
    // already present https://github.com/openai/openai-agents-python/issues/449
    // so here we do the same.
    if input_schema.properties.is_none() {
        input_schema.properties = Some(serde_json::Value::Object(serde_json::Map::new()));
    }

    // Serialize to a raw JSON value so we can sanitize schemas coming from MCP
    // servers. Some servers omit the top-level or nested `type` in JSON
    // Schemas (e.g. using enum/anyOf), or use unsupported variants like
    // `integer`. Our internal JsonSchema is a small subset and requires
    // `type`, so we coerce/sanitize here for compatibility.
    let mut serialized_input_schema = serde_json::to_value(input_schema)?;
    sanitize_json_schema(&mut serialized_input_schema);
    let input_schema = serde_json::from_value::<JsonSchema>(serialized_input_schema)?;

    Ok(ResponsesApiTool {
        name: fully_qualified_name,
        description: description.unwrap_or_default(),
        strict: false,
        parameters: input_schema,
    })
}

/// Sanitize a JSON Schema (as serde_json::Value) so it can fit our limited
/// JsonSchema enum. This function:
/// - Ensures every schema object has a "type". If missing, infers it from
///   common keywords (properties => object, items => array, enum/const/format => string)
///   and otherwise defaults to "string".
/// - Fills required child fields (e.g. array items, object properties) with
///   permissive defaults when absent.
fn sanitize_json_schema(value: &mut JsonValue) {
    match value {
        JsonValue::Bool(_) => {
            // JSON Schema boolean form: true/false. Coerce to an accept-all string.
            *value = json!({ "type": "string" });
        }
        JsonValue::Array(arr) => {
            for v in arr.iter_mut() {
                sanitize_json_schema(v);
            }
        }
        JsonValue::Object(map) => {
            // First, recursively sanitize known nested schema holders
            if let Some(props) = map.get_mut("properties") {
                if let Some(props_map) = props.as_object_mut() {
                    for (_k, v) in props_map.iter_mut() {
                        sanitize_json_schema(v);
                    }
                }
            }
            if let Some(items) = map.get_mut("items") {
                sanitize_json_schema(items);
            }
            // Some schemas use oneOf/anyOf/allOf - sanitize their entries
            for combiner in ["oneOf", "anyOf", "allOf", "prefixItems"] {
                if let Some(v) = map.get_mut(combiner) {
                    sanitize_json_schema(v);
                }
            }

            // Normalize/ensure type
            let mut ty = map.get("type").and_then(|v| v.as_str()).map(str::to_string);

            // If type is an array (union), pick first supported; else leave to inference
            if ty.is_none() {
                if let Some(JsonValue::Array(types)) = map.get("type") {
                    for t in types {
                        if let Some(tt) = t.as_str() {
                            if matches!(
                                tt,
                                "object" | "array" | "string" | "number" | "integer" | "boolean"
                            ) {
                                ty = Some(tt.to_string());
                                break;
                            }
                        }
                    }
                }
            }

            // Infer type if still missing
            if ty.is_none() {
                if map.contains_key("properties")
                    || map.contains_key("required")
                    || map.contains_key("additionalProperties")
                {
                    ty = Some("object".to_string());
                } else if map.contains_key("items") || map.contains_key("prefixItems") {
                    ty = Some("array".to_string());
                } else if map.contains_key("enum")
                    || map.contains_key("const")
                    || map.contains_key("format")
                {
                    ty = Some("string".to_string());
                } else if map.contains_key("minimum")
                    || map.contains_key("maximum")
                    || map.contains_key("exclusiveMinimum")
                    || map.contains_key("exclusiveMaximum")
                    || map.contains_key("multipleOf")
                {
                    ty = Some("number".to_string());
                }
            }
            // If we still couldn't infer, default to string
            let ty = ty.unwrap_or_else(|| "string".to_string());
            map.insert("type".to_string(), JsonValue::String(ty.to_string()));

            // Ensure object schemas have properties map
            if ty == "object" {
                if !map.contains_key("properties") {
                    map.insert(
                        "properties".to_string(),
                        JsonValue::Object(serde_json::Map::new()),
                    );
                }
                // If additionalProperties is an object schema, sanitize it too.
                // Leave booleans as-is, since JSON Schema allows boolean here.
                if let Some(ap) = map.get_mut("additionalProperties") {
                    let is_bool = matches!(ap, JsonValue::Bool(_));
                    if !is_bool {
                        sanitize_json_schema(ap);
                    }
                }
            }

            // Ensure array schemas have items
            if ty == "array" && !map.contains_key("items") {
                map.insert("items".to_string(), json!({ "type": "string" }));
            }
        }
        _ => {}
    }
}

/// Returns a list of OpenAiTools based on the provided config and MCP tools.
/// Note that the keys of mcp_tools should be fully qualified names. See
/// [`McpConnectionManager`] for more details.
pub fn get_openai_tools(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, mcp_types::Tool>>,
    browser_enabled: bool,
    _agents_active: bool,
) -> Vec<OpenAiTool> {
    let mut tools: Vec<OpenAiTool> = Vec::new();

    match &config.shell_type {
        ConfigShellToolType::DefaultShell => {
            tools.push(create_shell_tool());
        }
        ConfigShellToolType::ShellWithRequest { sandbox_policy } => {
            tools.push(create_shell_tool_for_sandbox(sandbox_policy));
        }
        ConfigShellToolType::LocalShell => {
            tools.push(OpenAiTool::LocalShell {});
        }
        ConfigShellToolType::StreamableShell => {
            tools.push(OpenAiTool::Function(
                crate::exec_command::create_exec_command_tool_for_responses_api(),
            ));
            tools.push(OpenAiTool::Function(
                crate::exec_command::create_write_stdin_tool_for_responses_api(),
            ));
        }
    }

    if config.plan_tool {
        tools.push(PLAN_TOOL.clone());
    }

    if config.skill_tool {
        tools.push(create_skill_tool());
    }

    tools.push(create_browser_tool(browser_enabled));

    // Add agent management tool for launching and monitoring asynchronous agents
    tools.push(create_agent_tool(config.agent_models()));

    // Add general wait tool for background completions
    tools.push(create_wait_tool());
    tools.push(create_kill_tool());

    if config.web_search_request {
        let tool = match &config.web_search_allowed_domains {
            Some(domains) if !domains.is_empty() => OpenAiTool::WebSearch(WebSearchTool {
                filters: Some(WebSearchFilters {
                    allowed_domains: Some(domains.clone()),
                }),
            }),
            _ => OpenAiTool::WebSearch(WebSearchTool::default()),
        };
        tools.push(tool);
    }


    if let Some(mcp_tools) = mcp_tools {
        // Ensure deterministic ordering to maximize prompt cache hits.
        // HashMap iteration order is non-deterministic, so sort by fully-qualified tool name.
        let mut entries: Vec<(String, mcp_types::Tool)> = mcp_tools.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, tool) in entries.into_iter() {
            match mcp_tool_to_openai_tool(name.clone(), tool.clone()) {
                Ok(converted_tool) => tools.push(OpenAiTool::Function(converted_tool)),
                Err(e) => {
                    tracing::error!("Failed to convert {name:?} MCP tool to OpenAI tool: {e:?}");
                }
            }
        }
    }

    tools
}

// ——————————————————————————————————————————————————————————————
// Background waiting tool (for long-running shell calls)
// ——————————————————————————————————————————————————————————————

pub fn create_wait_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "call_id".to_string(),
        JsonSchema::String {
            description: Some("Background call_id to wait for.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some(
                "Maximum time in milliseconds to wait (default 600000 = 10 minutes, max 3600000 = 60 minutes)."
                    .to_string(),
            ),
        },
    );
    OpenAiTool::Function(ResponsesApiTool {
        name: "wait".to_string(),
        description: "Wait for the background command identified by call_id to finish (optionally bounded by timeout_ms).".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["call_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub fn create_kill_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "call_id".to_string(),
        JsonSchema::String {
            description: Some("Background call_id to terminate.".to_string()),
            allowed_values: None,
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "kill".to_string(),
        description: "Terminate a running background command by call_id.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["call_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}

pub fn create_skill_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "skill".to_string(),
        JsonSchema::String {
            description: Some("Identifier of the skill to invoke.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "action".to_string(),
        JsonSchema::String {
            description: Some("Skill action to run (e.g., browser, agents, bash).".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "arguments".to_string(),
        JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: Some(true.into()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "skill".to_string(),
        description: "Invoke a Claude Skill with a specific action. Execution is stubbed in this build.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["skill".to_string(), "action".to_string()]),
            additional_properties: Some(true.into()),
        },
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::model_family::find_family_for_model;
    use mcp_types::ToolInputSchema;
    use pretty_assertions::assert_eq;

    use super::*;

    use crate::agent_defaults::enabled_agent_model_specs;

    fn test_agent_models() -> Vec<String> {
        enabled_agent_model_specs()
            .into_iter()
            .map(|spec| spec.slug.to_string())
            .collect()
    }

    fn apply_default_agent_models(config: &mut ToolsConfig) {
        config.set_agent_models(test_agent_models());
    }

    fn assert_eq_tool_names(tools: &[OpenAiTool], expected_names: &[&str]) {
        let tool_names = tools
            .iter()
            .map(|tool| match tool {
                OpenAiTool::Function(ResponsesApiTool { name, .. }) => name,
                OpenAiTool::LocalShell {} => "local_shell",
                OpenAiTool::WebSearch(_) => "web_search",
                OpenAiTool::Freeform(FreeformTool { name, .. }) => name,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            tool_names.len(),
            expected_names.len(),
            "tool_name mismatch, {tool_names:?}, {expected_names:?}",
        );
        for (name, expected_name) in tool_names.iter().zip(expected_names.iter()) {
            assert_eq!(
                name, expected_name,
                "tool_name mismatch, {name:?}, {expected_name:?}"
            );
        }
    }

    #[test]
    fn test_get_openai_tools() {
        let model_family = find_family_for_model("codex-mini-latest")
            .expect("codex-mini-latest should be a valid model family");
        let mut config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            true,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(&config, Some(HashMap::new()), false, false);

        assert_eq_tool_names(
            &tools,
            &[
                "local_shell",
                "update_plan",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
            ],
        );
    }

    #[test]
    fn test_get_openai_tools_with_active_agents() {
        let model_family = find_family_for_model("codex-mini-latest")
            .expect("codex-mini-latest should be a valid model family");
        let mut config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            true,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(&config, Some(HashMap::new()), false, true);

        assert_eq_tool_names(
            &tools,
            &[
                "local_shell",
                "update_plan",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
            ],
        );
    }

    #[test]
    fn test_get_openai_tools_default_shell() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let mut config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            true,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(&config, Some(HashMap::new()), false, false);

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "update_plan",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
            ],
        );
    }

    #[test]
    fn test_get_openai_tools_mcp_tools() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let mut config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            false,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "test_server/do_something_cool".to_string(),
                mcp_types::Tool {
                    name: "do_something_cool".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "string_argument": {
                                "type": "string",
                            },
                            "number_argument": {
                                "type": "number",
                            },
                            "object_argument": {
                                "type": "object",
                                "properties": {
                                    "string_property": { "type": "string" },
                                    "number_property": { "type": "number" },
                                },
                                "required": [
                                    "string_property",
                                    "number_property",
                                ],
                                "additionalProperties": Some(false),
                            },
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Do something cool".to_string()),
                },
            )])),
            false,
            true,
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
                "test_server/do_something_cool",
            ],
        );

        assert_eq!(
            tools[6],
            OpenAiTool::Function(ResponsesApiTool {
                name: "test_server/do_something_cool".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([
                        (
                            "string_argument".to_string(),
                            JsonSchema::String { description: None, allowed_values: None }
                        ),
                        (
                            "number_argument".to_string(),
                            JsonSchema::Number { description: None }
                        ),
                        (
                            "object_argument".to_string(),
                            JsonSchema::Object {
                                properties: BTreeMap::from([
                                    (
                                        "string_property".to_string(),
                                        JsonSchema::String { description: None, allowed_values: None }
                                    ),
                                    (
                                        "number_property".to_string(),
                                        JsonSchema::Number { description: None }
                                    ),
                                ]),
                                required: Some(vec![
                                    "string_property".to_string(),
                                    "number_property".to_string(),
                                ]),
                                additional_properties: Some(false.into()),
                            },
                        ),
                    ]),
                    required: None,
                    additional_properties: None,
                },
                description: "Do something cool".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_get_openai_tools_mcp_tools_with_additional_properties_schema() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let mut config = ToolsConfig::new_from_params(&ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: true,
        });
        apply_default_agent_models(&mut config);
        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "test_server/do_something_cool".to_string(),
                mcp_types::Tool {
                    name: "do_something_cool".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "string_argument": {
                                "type": "string",
                            },
                            "number_argument": {
                                "type": "number",
                            },
                            "object_argument": {
                                "type": "object",
                                "properties": {
                                    "string_property": { "type": "string" },
                                    "number_property": { "type": "number" },
                                },
                                "required": [
                                    "string_property",
                                    "number_property",
                                ],
                                "additionalProperties": {
                                    "type": "object",
                                    "properties": {
                                        "addtl_prop": { "type": "string" },
                                    },
                                    "required": [
                                        "addtl_prop",
                                    ],
                                    "additionalProperties": false,
                                },
                            },
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Do something cool".to_string()),
                },
            )])),
            false,
            true,
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
                "test_server/do_something_cool",
            ],
        );

        assert_eq!(
            tools[6],
            OpenAiTool::Function(ResponsesApiTool {
                name: "test_server/do_something_cool".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([
                        (
                            "string_argument".to_string(),
                            JsonSchema::String { description: None, allowed_values: None }
                        ),
                        (
                            "number_argument".to_string(),
                            JsonSchema::Number { description: None }
                        ),
                        (
                            "object_argument".to_string(),
                            JsonSchema::Object {
                                properties: BTreeMap::from([
                                    (
                                        "string_property".to_string(),
                                        JsonSchema::String { description: None, allowed_values: None }
                                    ),
                                    (
                                        "number_property".to_string(),
                                        JsonSchema::Number { description: None }
                                    ),
                                ]),
                                required: Some(vec![
                                    "string_property".to_string(),
                                    "number_property".to_string(),
                                ]),
                                additional_properties: Some(
                                    JsonSchema::Object {
                                        properties: BTreeMap::from([(
                                            "addtl_prop".to_string(),
                                            JsonSchema::String { description: None, allowed_values: None }
                                        ),]),
                                        required: Some(vec!["addtl_prop".to_string(),]),
                                        additional_properties: Some(false.into()),
                                    }
                                    .into()
                                ),
                            },
                        ),
                    ]),
                    required: None,
                    additional_properties: None,
                },
                description: "Do something cool".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_get_openai_tools_mcp_tools_sorted_by_name() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let _config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            false,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
    }

    #[test]
    fn test_mcp_tool_property_missing_type_defaults_to_string() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let mut config = ToolsConfig::new_from_params(&ToolsConfigParams {
            model_family: &model_family,
            approval_policy: AskForApproval::Never,
            sandbox_policy: SandboxPolicy::ReadOnly,
            include_plan_tool: false,
            include_apply_patch_tool: false,
            include_web_search_request: true,
            use_streamable_shell_tool: false,
            include_view_image_tool: true,
        });
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/search".to_string(),
                mcp_types::Tool {
                    name: "search".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "query": {
                                "description": "search query"
                            }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Search docs".to_string()),
                },
            )])),
            false,
            true,
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
                "dash/search",
            ],
        );

        assert_eq!(
            tools[6],
            OpenAiTool::Function(ResponsesApiTool {
                name: "dash/search".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "query".to_string(),
                        JsonSchema::String {
                            description: Some("search query".to_string()),
                            allowed_values: None,
                        }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "Search docs".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_mcp_tool_integer_normalized_to_number() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let mut config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            false,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/paginate".to_string(),
                mcp_types::Tool {
                    name: "paginate".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "page": { "type": "integer" }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Pagination".to_string()),
                },
            )])),
            false,
            true,
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
                "dash/paginate",
            ],
        );
        let paginate_tool = tools
            .iter()
            .find(|tool| matches!(tool, OpenAiTool::Function(ResponsesApiTool { name, .. }) if name == "dash/paginate"))
            .expect("dash/paginate tool present");

        assert_eq!(
            paginate_tool,
            &OpenAiTool::Function(ResponsesApiTool {
                name: "dash/paginate".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "page".to_string(),
                        JsonSchema::Number { description: None }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "Pagination".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_mcp_tool_array_without_items_gets_default_string_items() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let mut config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            false,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/tags".to_string(),
                mcp_types::Tool {
                    name: "tags".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "tags": { "type": "array" }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("Tags".to_string()),
                },
            )])),
            false,
            true,
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
                "dash/tags",
            ],
        );
        assert_eq!(
            tools[6],
            OpenAiTool::Function(ResponsesApiTool {
                name: "dash/tags".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "tags".to_string(),
                        JsonSchema::Array {
                            items: Box::new(JsonSchema::String { description: None, allowed_values: None }),
                            description: None
                        }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "Tags".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_mcp_tool_anyof_defaults_to_string() {
        let model_family = find_family_for_model("o3").expect("o3 should be a valid model family");
        let mut config = ToolsConfig::new(
            &model_family,
            AskForApproval::Never,
            SandboxPolicy::ReadOnly,
            false,
            false,
            true,
            /*use_experimental_streamable_shell_tool*/ false,
            false,
        );
        apply_default_agent_models(&mut config);

        let tools = get_openai_tools(
            &config,
            Some(HashMap::from([(
                "dash/value".to_string(),
                mcp_types::Tool {
                    name: "value".to_string(),
                    input_schema: ToolInputSchema {
                        properties: Some(serde_json::json!({
                            "value": { "anyOf": [ { "type": "string" }, { "type": "number" } ] }
                        })),
                        required: None,
                        r#type: "object".to_string(),
                    },
                    output_schema: None,
                    title: None,
                    annotations: None,
                    description: Some("AnyOf Value".to_string()),
                },
            )])),
            false,
            true,
        );

        assert_eq_tool_names(
            &tools,
            &[
                "shell",
                "browser",
                "agent",
                "wait",
                "kill",
                "web_search",
                "dash/value",
            ],
        );
        assert_eq!(
            tools[6],
            OpenAiTool::Function(ResponsesApiTool {
                name: "dash/value".to_string(),
                parameters: JsonSchema::Object {
                    properties: BTreeMap::from([(
                        "value".to_string(),
                        JsonSchema::String { description: None, allowed_values: None }
                    )]),
                    required: None,
                    additional_properties: None,
                },
                description: "AnyOf Value".to_string(),
                strict: false,
            })
        );
    }

    #[test]
    fn test_shell_tool_for_sandbox_workspace_write() {
        let sandbox_policy = SandboxPolicy::WorkspaceWrite {
            writable_roots: vec!["workspace".into()],
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
            allow_git_writes: true,
        };
        let tool = super::create_shell_tool_for_sandbox(&sandbox_policy);
        let OpenAiTool::Function(ResponsesApiTool {
            description, name, ..
        }) = &tool
        else {
            panic!("expected function tool");
        };
        assert_eq!(name, "shell");
        assert!(
            description.contains("The shell tool is used to execute shell commands."),
            "description should explain shell usage"
        );
        assert!(
            description.contains("writable roots:"),
            "description should list writable roots"
        );
        assert!(
            description.contains("- workspace"),
            "description should mention workspace root"
        );
        assert!(
            description.contains("Commands that require network access"),
            "description should mention network access requirements"
        );
        assert!(
            description.contains("Long-running commands may be backgrounded"),
            "description should mention backgrounded commands"
        );
    }

    #[test]
    fn test_shell_tool_for_sandbox_readonly() {
        let tool = super::create_shell_tool_for_sandbox(&SandboxPolicy::ReadOnly);
        let OpenAiTool::Function(ResponsesApiTool {
            description, name, ..
        }) = &tool
        else {
            panic!("expected function tool");
        };
        assert_eq!(name, "shell");

        assert_eq!(name, "shell");
        assert!(description.starts_with("Runs a shell command and returns its output."));
        assert!(description.contains("Long-running commands may be backgrounded"));
    }

    #[test]
    fn test_shell_tool_for_sandbox_danger_full_access() {
        let tool = super::create_shell_tool_for_sandbox(&SandboxPolicy::DangerFullAccess);
        let OpenAiTool::Function(ResponsesApiTool {
            description, name, ..
        }) = &tool
        else {
            panic!("expected function tool");
        };
        assert_eq!(name, "shell");
        assert!(description.starts_with("Runs a shell command and returns its output."));
        assert!(description.contains("Long-running commands may be backgrounded"));
    }
}

fn create_browser_tool(browser_enabled: bool) -> OpenAiTool {
    let mut actions = vec!["open", "status", "fetch"];
    if browser_enabled {
        actions.extend([
            "close",
            "click",
            "move",
            "type",
            "key",
            "javascript",
            "scroll",
            "history",
            "inspect",
            "console",
            "cleanup",
            "cdp",
        ]);
    }

    let mut properties = BTreeMap::new();
    properties.insert(
        "action".to_string(),
        JsonSchema::String {
            description: Some(
                "Required: choose one of the supported browser actions (e.g., 'open', 'click', 'fetch')."
                    .to_string(),
            ),
            allowed_values: Some(actions.iter().map(|value| value.to_string()).collect()),
        },
    );

    properties.insert(
        "url".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=open or fetch: URL to navigate to or retrieve (e.g., https://example.com)."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "type".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=click: optional mouse event type ('click', 'mousedown', 'mouseup')."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "x".to_string(),
        JsonSchema::Number {
            description: Some(
                "For actions=click/move/inspect: absolute X coordinate; use with 'y'."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "y".to_string(),
        JsonSchema::Number {
            description: Some(
                "For actions=click/move/inspect: absolute Y coordinate; use with 'x'."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "dx".to_string(),
        JsonSchema::Number {
            description: Some(
                "For action=move/scroll: relative X delta in CSS pixels (use with 'dy')."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "dy".to_string(),
        JsonSchema::Number {
            description: Some(
                "For action=move/scroll: relative Y delta in CSS pixels (use with 'dx')."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "text".to_string(),
        JsonSchema::String {
            description: Some("For action=type: text to send to the focused element.".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "key".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=key: key to press (e.g., Enter, Tab, Escape).".to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "code".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=javascript: JavaScript source to execute in the browser context.".to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "direction".to_string(),
        JsonSchema::String {
            description: Some("For action=history: history direction ('back' or 'forward').".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "id".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=inspect: optional element id (without '#') to inspect.".to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "lines".to_string(),
        JsonSchema::Number {
            description: Some(
                "For action=console: optional number of recent console lines to return.".to_string(),
            ),
        },
    );
    properties.insert(
        "method".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=cdp: Chrome DevTools Protocol method name (e.g., 'Page.navigate')."
                    .to_string(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "params".to_string(),
        JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: Some(true.into()),
        },
    );
    properties.insert(
        "target".to_string(),
        JsonSchema::String {
            description: Some("For action=cdp: target session ('page' default or 'browser').".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout_ms".to_string(),
        JsonSchema::Number {
            description: Some(
                "For action=fetch: optional timeout in milliseconds for the HTTP request.".to_string(),
            ),
        },
    );
    properties.insert(
        "mode".to_string(),
        JsonSchema::String {
            description: Some(
                "For action=fetch: optional fetch mode ('auto', 'browser', or 'http').".to_string(),
            ),
            allowed_values: None,
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "browser".to_string(),
        description: "Unified browser controller for navigation, interaction, console access, DevTools commands, and one-shot fetches. Choose an action and supply the matching fields.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["action".to_string()]),
            additional_properties: Some(false.into()),
        },
    })
}
