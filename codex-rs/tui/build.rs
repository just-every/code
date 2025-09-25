fn main() {
    if let Ok(profile) = std::env::var("PROFILE") {
        println!("cargo:rustc-env=CODEX_PROFILE={profile}");
    }
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    #[derive(serde::Deserialize, serde::Serialize, Clone)]
    struct Src { interval: u64, frames: Vec<String> }
    #[derive(serde::Deserialize, serde::Serialize, Clone)]
    struct Dest { interval: u64, frames: Vec<String>, label: String, group: String }

    fn humanize(name: &str) -> String {
        let mut out = String::new();
        let mut prev_is_lower = false;
        let mut prev_is_alpha = false;
        for ch in name.chars() {
            if ch == '-' || ch == '_' { out.push(' '); prev_is_lower=false; prev_is_alpha=false; continue; }
            if ch.is_ascii_uppercase() && prev_is_lower { out.push(' '); }
            else if ch.is_ascii_digit() && prev_is_alpha { out.push(' '); }
            out.push(ch);
            prev_is_lower = ch.is_ascii_lowercase();
            prev_is_alpha = ch.is_ascii_alphabetic();
        }
        out.split_whitespace().map(|w| {
            let mut cs = w.chars();
            match cs.next() { Some(f) => format!("{}{}", f.to_uppercase(), cs.as_str().to_lowercase()), None => String::new() }
        }).collect::<Vec<_>>().join(" ")
    }
    fn group_for(name: &str) -> String {
        let n = name.to_ascii_lowercase();
        let key = n.as_str();
        let g = if key.contains("dot") { "Dots" }
            else if key.contains("circle") || key.contains("round") || key.contains("arc") { "Circles" }
            else if key.contains("line") || key.contains("pipe") || key.contains("bar") || key.contains("pulse") { "Lines" }
            else if key.contains("bounce") || key.contains("ball") || key.contains("pong") { "Bouncing" }
            else if key.contains("star") || key.contains("asterisk") { "Stars" }
            else if key.contains("arrow") || key.contains("triangle") { "Arrows" }
            else if key.contains("box") || key.contains("square") { "Boxes" }
            else if key.contains("toggle") { "Toggles" }
            else if key.contains("monkey") || key.contains("earth") || key.contains("moon") || key.contains("weather") || key.contains("smiley") || key.contains("emoji") { "Emoji" }
            else { "Other" };
        g.to_string()
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = PathBuf::from(manifest_dir).join("src").join("assets").join("spinners.json");
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(src) = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&text) {
            let mut changed = false;
            let mut out: BTreeMap<String, Dest> = BTreeMap::new();
            for (name, v) in src.into_iter() {
                if let Ok(d) = serde_json::from_value::<Dest>(v.clone()) {
                    out.insert(name, d); continue;
                }
                if let Ok(s) = serde_json::from_value::<Src>(v.clone()) {
                    out.insert(name.clone(), Dest { interval: s.interval, frames: s.frames, label: humanize(&name), group: group_for(&name) });
                    changed = true;
                }
            }
            if changed {
                let pretty = serde_json::to_string_pretty(&out).unwrap();
                let _ = fs::write(&path, pretty);
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
}
