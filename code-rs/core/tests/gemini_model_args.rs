use code_core::agent_defaults::agent_model_spec;

#[test]
fn gemini_specs_use_long_model_flag() {
    let latest = agent_model_spec("gemini-3-pro").expect("spec present");
    assert_eq!(latest.model_args, ["--model", "gemini-3-pro"]);
    // Legacy shorthand and older slugs should resolve to the newest Gemini 3 Pro.
    let alias = agent_model_spec("gemini").expect("alias present");
    assert_eq!(alias.slug, "gemini-3-pro");

    let legacy = agent_model_spec("gemini-2.5-pro").expect("spec present via alias");
    assert_eq!(legacy.slug, "gemini-3-pro");

    let flash = agent_model_spec("gemini-2.5-flash").expect("spec present");
    assert_eq!(flash.model_args, ["--model", "gemini-2.5-flash"]);
}
