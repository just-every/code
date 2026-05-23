use schemars::JsonSchema;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde_json::Value as JsonValue;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub namespace: Option<String>,
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
    #[serde(default)]
    pub defer_loading: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DynamicToolSpecDe {
    namespace: Option<String>,
    name: String,
    description: String,
    input_schema: JsonValue,
    defer_loading: Option<bool>,
    expose_to_context: Option<bool>,
}

impl<'de> Deserialize<'de> for DynamicToolSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let DynamicToolSpecDe {
            namespace,
            name,
            description,
            input_schema,
            defer_loading,
            expose_to_context,
        } = DynamicToolSpecDe::deserialize(deserializer)?;

        Ok(Self {
            namespace,
            name,
            description,
            input_schema,
            defer_loading: defer_loading
                .unwrap_or_else(|| expose_to_context.map(|visible| !visible).unwrap_or(false)),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolCallRequest {
    pub call_id: String,
    pub turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub namespace: Option<String>,
    pub tool: String,
    pub arguments: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolResponse {
    pub content_items: Vec<DynamicToolCallOutputContentItem>,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type")]
pub enum DynamicToolCallOutputContentItem {
    #[serde(rename_all = "camelCase")]
    InputText { text: String },
    #[serde(rename_all = "camelCase")]
    InputImage { image_url: String },
}
