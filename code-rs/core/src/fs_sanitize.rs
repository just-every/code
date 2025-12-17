pub(crate) fn safe_path_component(value: &str, fallback: &str) -> String {
    if is_safe_single_component(value) && value.len() <= 64 {
        return value.to_string();
    }

    let mut slug = String::with_capacity(value.len().min(64));
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') {
            slug.push(ch);
        } else {
            slug.push('_');
        }
        if slug.len() >= 48 {
            break;
        }
    }

    while matches!(slug.chars().next(), Some('_' | '-' | '.')) {
        slug.remove(0);
    }
    while matches!(slug.chars().last(), Some('_' | '-' | '.')) {
        slug.pop();
    }

    if slug.is_empty() || slug == "." || slug == ".." {
        slug = fallback.to_string();
    }

    let hash = fnv1a_64(value.as_bytes());
    format!("{slug}-{hash:016x}")
}

fn is_safe_single_component(value: &str) -> bool {
    if value.is_empty() || value == "." || value == ".." {
        return false;
    }

    !value
        .chars()
        .any(|ch| matches!(ch, '/' | '\\' | '\0'))
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
