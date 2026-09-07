//! Checks that filtering the new mode preserves the stable elicitation contracts.

use super::*;
use crate::McpServerElicitationRequestParams;
use crate::TS;
use anyhow::Result;
use pretty_assertions::assert_eq;

#[test]
fn verification_mode_is_removed_from_flattened_typescript_only_for_stable_exports() -> Result<()> {
    let generated = McpServerElicitationRequestParams::export_to_string()?;
    assert!(generated.contains(MODE));
    let filtered = filter_ts(&generated);
    assert!(!filtered.contains(MODE));
    assert_eq!(filter_ts(&filtered), filtered);
    for mode in ["form", "openai/form", "openaiForm", "url"] {
        assert!(filtered.contains(&format!("\"mode\": \"{mode}\"")));
    }
    assert!(filtered.contains("threadId: string"));
    assert!(filtered.contains("requestedSchema: McpElicitationSchema"));
    Ok(())
}

#[test]
fn verification_mode_is_removed_from_inline_json_without_changing_other_modes() -> Result<()> {
    let generated = serde_json::to_value(schemars::schema_for!(McpServerElicitationRequestParams))?;
    assert!(generated.to_string().contains(MODE));
    let mut expected = generated.clone();
    let modes = expected["oneOf"].as_array_mut().expect("flattened modes");
    let verification = modes.remove(0);
    assert_eq!(
        verification["properties"]["mode"]["enum"],
        serde_json::json!([MODE])
    );
    let mut filtered = generated;
    filter_json(&mut filtered);
    assert_eq!(filtered, expected);
    Ok(())
}
