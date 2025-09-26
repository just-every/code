use crossterm::tty::IsTty;

/// Emit an OSC 9 notification when supported by the terminal.
pub(crate) fn emit(title: &str, body: Option<&str>) {
    let mut stdout = std::io::stdout();
    if !stdout.is_tty() {
        return;
    }

    let payload = title.trim();
    let mut owned_payload;
    if payload.is_empty() {
        owned_payload = "Code".to_string();
    } else {
        owned_payload = payload.to_string();
    }

    if let Some(body) = body {
        let trimmed = body.trim();
        if !trimmed.is_empty() {
            owned_payload.push(':');
            owned_payload.push(' ');
            owned_payload.push_str(trimmed);
        }
    }

    let sequence = format!("\x1b]9;{}\x1b\\", owned_payload);
    let _ = crossterm::execute!(stdout, crossterm::style::Print(sequence));
}
