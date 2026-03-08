use code_core::agent_defaults::agent_model_spec;

#[test]
fn gemini_specs_use_long_model_flag() {
    let pro = agent_model_spec("gemini-3.1-pro-preview").expect("spec present");
    assert_eq!(pro.model_args, ["--model", "gemini-3.1-pro-preview"]);

    // The shorthand `gemini` is treated as the primary Gemini default.
    let primary = agent_model_spec("gemini").expect("alias present");
    assert_eq!(primary.slug, "gemini-3-flash-preview");
    assert_eq!(primary.model_args, ["--model", "gemini-3-flash-preview"]);

    // Legacy shorthand and older slugs should resolve to the newest Gemini Pro preview.
    let legacy_pro = agent_model_spec("gemini-2.5-pro").expect("spec present via alias");
    assert_eq!(legacy_pro.slug, "gemini-3.1-pro-preview");

    let prior_preview = agent_model_spec("gemini-3-pro").expect("prior preview alias present");
    assert_eq!(prior_preview.slug, "gemini-3.1-pro-preview");

    let legacy_flash = agent_model_spec("gemini-2.5-flash").expect("spec present via alias");
    assert_eq!(legacy_flash.slug, "gemini-3-flash-preview");
    assert_eq!(legacy_flash.model_args, ["--model", "gemini-3-flash-preview"]);
}
