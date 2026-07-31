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
    ProLite,
    Team,
    #[serde(rename = "self_serve_business_prolite")]
    #[ts(rename = "self_serve_business_prolite")]
    SelfServeBusinessProLite,
    #[serde(rename = "self_serve_business_usage_based")]
    #[ts(rename = "self_serve_business_usage_based")]
    SelfServeBusinessUsageBased,
    Business,
    Ent26,
    #[serde(rename = "enterprise_cbp_automation")]
    #[ts(rename = "enterprise_cbp_automation")]
    EnterpriseCbpAutomation,
    #[serde(rename = "enterprise_cbp_usage_based")]
    #[ts(rename = "enterprise_cbp_usage_based")]
    EnterpriseCbpUsageBased,
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

    #[test]
    fn business_plan_types_use_expected_wire_names() {
        for (plan_type, wire_name) in [
            (PlanType::ProLite, "prolite"),
            (
                PlanType::SelfServeBusinessProLite,
                "self_serve_business_prolite",
            ),
            (
                PlanType::SelfServeBusinessUsageBased,
                "self_serve_business_usage_based",
            ),
            (
                PlanType::EnterpriseCbpAutomation,
                "enterprise_cbp_automation",
            ),
            (
                PlanType::EnterpriseCbpUsageBased,
                "enterprise_cbp_usage_based",
            ),
        ] {
            assert_eq!(
                serde_json::to_string(&plan_type).expect("plan should serialize"),
                format!("\"{wire_name}\"")
            );
            assert_eq!(
                serde_json::from_str::<PlanType>(&format!("\"{wire_name}\""))
                    .expect("plan should deserialize"),
                plan_type
            );
        }
    }
}
