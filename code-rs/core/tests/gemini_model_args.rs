use code_core::agent_defaults::agent_model_spec;

#[test]
fn gemini_specs_use_long_model_flag() {
    let pro = agent_model_spec("gemini-3.1-pro").expect("spec present");
    assert_eq!(pro.family, "antigravity");
    assert_eq!(pro.cli, "agy");
    assert_eq!(pro.model_args, ["--model", "Gemini 3.1 Pro (High)"]);

    // The shorthand `gemini` follows Google's Antigravity CLI migration path.
    let primary = agent_model_spec("gemini").expect("alias present");
    // 2026-10-10 — gemini-3.5-flash was deprecated by Google; the alias now
    // resolves to gemini-3.6-flash (the new default).
    assert_eq!(primary.slug, "gemini-3.6-flash");
    assert_eq!(primary.cli, "agy");
    assert!(primary.model_args.is_empty());

    // Legacy shorthand and older slugs should resolve to the newest Gemini presets.
    let legacy_pro = agent_model_spec("gemini-2.5-pro").expect("spec present via alias");
    assert_eq!(legacy_pro.slug, "gemini-3.1-pro");
    assert_eq!(legacy_pro.cli, "agy");

    // 2026-10-10 — `gemini-2.5-flash` and the legacy `gemini-3.5-flash` both
    // resolve to the new `gemini-3.6-flash` default.
    let legacy_flash = agent_model_spec("gemini-2.5-flash").expect("spec present via alias");
    assert_eq!(legacy_flash.slug, "gemini-3.6-flash");
    assert!(legacy_flash.model_args.is_empty());

    // Direct `gemini-3.5-flash` slug also resolves via the deprecation alias.
    let deprecated_flash = agent_model_spec("gemini-3.5-flash").expect("deprecated alias still resolves");
    assert_eq!(deprecated_flash.slug, "gemini-3.6-flash");
}
