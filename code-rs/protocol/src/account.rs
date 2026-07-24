use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, JsonSchema, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(rename_all = "lowercase")]
pub enum PlanType {
    #[default]
    Free,
    Go,
    Plus,
    Pro,
    Team,
    Business,
    Ent26,
    Enterprise,
    Edu,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::PlanType;

    #[test]
    fn ent26_uses_expected_wire_name() {
        assert_eq!(
            serde_json::to_string(&PlanType::Ent26).expect("ent26 should serialize"),
            "\"ent26\""
        );
        assert_eq!(
            serde_json::from_str::<PlanType>("\"ent26\"").expect("ent26 should deserialize"),
            PlanType::Ent26
        );
    }
}
