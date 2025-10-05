use std::borrow::Cow;

#[cfg(target_os = "macos")]
pub(crate) fn macos_brew_formula_for_command(command: &str) -> Cow<'_, str> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Cow::Borrowed(trimmed);
    }
    if trimmed.contains('/') || trimmed.contains(char::is_whitespace) {
        return Cow::Borrowed(trimmed);
    }
    if trimmed.eq_ignore_ascii_case("claude") {
        return Cow::Borrowed("claude-code");
    }
    if trimmed.eq_ignore_ascii_case("gemini") {
        return Cow::Borrowed("gemini-cli");
    }
    if trimmed.eq_ignore_ascii_case("qwen") {
        return Cow::Borrowed("qwen-code");
    }
    Cow::Borrowed(trimmed)
}
