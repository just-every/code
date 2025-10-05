// Returns true if the provided source appears to be inside an unclosed
// fenced code block using triple backticks. The check is tolerant of
// leading whitespace before fences and optional language identifiers
// on the opening fence line.
pub fn is_inside_unclosed_fence(source: &str) -> bool {
    let mut open = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            // Toggle fence state on each fence line.
            open = !open;
        }
    }
    open
}

// Remove empty fenced code blocks (```lang ... ``` with only whitespace or
// blank lines inside). Preserves a single blank line in place so that
// subsequent headings or content start on a new line, matching the
// expectations of the streaming renderer.
pub fn strip_empty_fenced_code_blocks(source: &str) -> String {
    // Fast path: if there's no fence, return as-is.
    if !source.contains("```") {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    let mut lines = source.lines();
    let mut pending_blank = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            // Capture possible language; then scan forward until closing fence.
            let mut inner = Vec::new();
            let mut closed = false;
            while let Some(l) = lines.next() {
                if l.trim_start().starts_with("```") {
                    closed = true;
                    break;
                }
                inner.push(l);
            }

            if closed {
                // Determine if inner content is effectively empty (only whitespace).
                let empty = inner.iter().all(|l| l.trim().is_empty());
                if empty {
                    // Replace the entire empty block with a single blank separator.
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    // Ensure exactly one blank line separation.
                    if !out.ends_with("\n\n") {
                        out.push('\n');
                    }
                    // Do not emit the original fences.
                    continue;
                } else {
                    // Not empty: re-emit the original block faithfully.
                    out.push_str(line);
                    out.push('\n');
                    for l in inner {
                        out.push_str(l);
                        out.push('\n');
                    }
                    out.push_str("```");
                    out.push('\n');
                    continue;
                }
            } else {
                // Unclosed fence: re-emit what we consumed (opening line only)
                // and let the caller's is_inside_unclosed_fence handle holdback.
                out.push_str(line);
                out.push('\n');
                continue;
            }
        }

        // Normal line passthrough.
        if pending_blank {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            pending_blank = false;
        }
        out.push_str(line);
        out.push('\n');
    }

    // Preserve exact trailing newline behavior of input.
    match source.chars().last() {
        Some('\n') => {}
        _ => {
            // We added '\n' after each line; trim one if input didn't end with newline.
            if out.ends_with('\n') {
                out.pop();
            }
        }
    }
    out
}
