use crate::config_types::EnvironmentVariablePattern;
use crate::config_types::ShellEnvironmentPolicy;
use crate::config_types::ShellEnvironmentPolicyInherit;
use std::collections::HashMap;
use std::collections::HashSet;

/// Construct an environment map based on the rules in the specified policy. The
/// resulting map can be passed directly to `Command::envs()` after calling
/// `env_clear()` to ensure no unintended variables are leaked to the spawned
/// process.
///
/// The derivation follows the algorithm documented in the struct-level comment
/// for [`ShellEnvironmentPolicy`].
pub fn create_env(policy: &ShellEnvironmentPolicy) -> HashMap<String, String> {
    populate_env(std::env::vars(), policy)
}

fn populate_env<I>(vars: I, policy: &ShellEnvironmentPolicy) -> HashMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    // Step 1 – determine the starting set of variables based on the
    // `inherit` strategy.
    let mut env_map: HashMap<String, String> = match policy.inherit {
        ShellEnvironmentPolicyInherit::All => vars.into_iter().collect(),
        ShellEnvironmentPolicyInherit::None => HashMap::new(),
        ShellEnvironmentPolicyInherit::Core => {
            const CORE_VARS: &[&str] = &[
                "HOME", "LOGNAME", "PATH", "PATHEXT", "SHELL", "USER", "USERNAME", "TMPDIR", "TEMP", "TMP",
            ];
            let allow: HashSet<&str> = CORE_VARS.iter().copied().collect();
            vars.into_iter()
                .filter(|(k, _)| allow.contains(k.as_str()))
                .collect()
        }
    };

    // Internal helper – does `name` match **any** pattern in `patterns`?
    let matches_any = |name: &str, patterns: &[EnvironmentVariablePattern]| -> bool {
        patterns.iter().any(|pattern| pattern.matches(name))
    };

    // Step 2 – Apply the default exclude if not disabled.
    if !policy.ignore_default_excludes {
        let default_excludes = vec![
            EnvironmentVariablePattern::new_case_insensitive("*KEY*"),
            EnvironmentVariablePattern::new_case_insensitive("*SECRET*"),
            EnvironmentVariablePattern::new_case_insensitive("*TOKEN*"),
        ];
        env_map.retain(|k, _| !matches_any(k, &default_excludes));
    }

    // Step 3 – Apply custom excludes.
    if !policy.exclude.is_empty() {
        env_map.retain(|k, _| !matches_any(k, &policy.exclude));
    }

    // Step 4 – Apply user-provided overrides.
    for (key, val) in &policy.r#set {
        env_map.insert(key.clone(), val.clone());
    }

    // Step 4.5 – Disable interactive pagers by default so shell calls do not
    // block on `less` (common for `gh`/`git`). Respect explicit overrides.
    for (key, val) in [
        ("PAGER", "cat"),
        ("GIT_PAGER", "cat"),
        ("GH_PAGER", "cat"),
    ] {
        env_map.entry(key.to_string()).or_insert_with(|| val.to_string());
    }

    // Step 4.6 – Non-interactive defaults to reduce hangs and environment
    // variance in headless runs. Respect explicit overrides.
    for (key, val) in [
        ("GIT_TERMINAL_PROMPT", "0"),
        ("CI", "1"),
        ("DEBIAN_FRONTEND", "noninteractive"),
        ("LANG", "C.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
    ] {
        env_map.entry(key.to_string()).or_insert_with(|| val.to_string());
    }

    #[cfg(unix)]
    {
        env_map
            .entry("GIT_ASKPASS".to_string())
            .or_insert_with(|| "true".to_string());
    }

    // Step 5 – If include_only is non-empty, keep *only* the matching vars.
    if !policy.include_only.is_empty() {
        env_map.retain(|k, _| matches_any(k, &policy.include_only));
    }

    env_map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_types::ShellEnvironmentPolicyInherit;
    use maplit::hashmap;

    fn make_vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_core_inherit_default_policy_skips_filters() {
        let vars = make_vars(&[
            ("PATH", "/usr/bin"),
            ("PATHEXT", ".EXE;.CMD"),
            ("HOME", "/home/user"),
            ("API_KEY", "secret"),
            ("SECRET_TOKEN", "t"),
        ]);

        let policy = ShellEnvironmentPolicy::default(); // inherit Core, default excludes disabled
        let result = populate_env(vars, &policy);

        let mut expected: HashMap<String, String> = hashmap! {
            "PATH".to_string() => "/usr/bin".to_string(),
            "PATHEXT".to_string() => ".EXE;.CMD".to_string(),
            "HOME".to_string() => "/home/user".to_string(),
            "PAGER".to_string() => "cat".to_string(),
            "GIT_PAGER".to_string() => "cat".to_string(),
            "GH_PAGER".to_string() => "cat".to_string(),
            "GIT_TERMINAL_PROMPT".to_string() => "0".to_string(),
            "CI".to_string() => "1".to_string(),
            "DEBIAN_FRONTEND".to_string() => "noninteractive".to_string(),
            "LANG".to_string() => "C.UTF-8".to_string(),
            "LC_ALL".to_string() => "C.UTF-8".to_string(),
        };

        expected.insert("API_KEY".to_string(), "secret".to_string());
        expected.insert("SECRET_TOKEN".to_string(), "t".to_string());

        #[cfg(unix)]
        expected.insert("GIT_ASKPASS".to_string(), "true".to_string());

        assert_eq!(result, expected);
    }

    #[test]
    fn test_include_only() {
        let vars = make_vars(&[("PATH", "/usr/bin"), ("FOO", "bar")]);

        let policy = ShellEnvironmentPolicy {
            // skip default excludes so nothing is removed prematurely
            ignore_default_excludes: true,
            include_only: vec![EnvironmentVariablePattern::new_case_insensitive("*PATH")],
            ..Default::default()
        };

        let result = populate_env(vars, &policy);

        let expected: HashMap<String, String> = hashmap! {
            "PATH".to_string() => "/usr/bin".to_string(),
        };

        assert_eq!(result, expected);
    }

    #[test]
    fn test_set_overrides() {
        let vars = make_vars(&[("PATH", "/usr/bin")]);

        let mut policy = ShellEnvironmentPolicy {
            ignore_default_excludes: true,
            ..Default::default()
        };
        policy.r#set.insert("NEW_VAR".to_string(), "42".to_string());

        let result = populate_env(vars, &policy);

        let mut expected: HashMap<String, String> = hashmap! {
            "PATH".to_string() => "/usr/bin".to_string(),
            "PAGER".to_string() => "cat".to_string(),
            "GIT_PAGER".to_string() => "cat".to_string(),
            "GH_PAGER".to_string() => "cat".to_string(),
            "NEW_VAR".to_string() => "42".to_string(),
            "GIT_TERMINAL_PROMPT".to_string() => "0".to_string(),
            "CI".to_string() => "1".to_string(),
            "DEBIAN_FRONTEND".to_string() => "noninteractive".to_string(),
            "LANG".to_string() => "C.UTF-8".to_string(),
            "LC_ALL".to_string() => "C.UTF-8".to_string(),
        };

        #[cfg(unix)]
        expected.insert("GIT_ASKPASS".to_string(), "true".to_string());

        assert_eq!(result, expected);
    }

    #[test]
    fn test_inherit_all() {
        let vars = make_vars(&[("PATH", "/usr/bin"), ("FOO", "bar")]);

        let policy = ShellEnvironmentPolicy {
            inherit: ShellEnvironmentPolicyInherit::All,
            ignore_default_excludes: true, // keep everything
            ..Default::default()
        };

        let result = populate_env(vars.clone(), &policy);
        let mut expected: HashMap<String, String> = vars.into_iter().collect();
        expected.insert("PAGER".to_string(), "cat".to_string());
        expected.insert("GIT_PAGER".to_string(), "cat".to_string());
        expected.insert("GH_PAGER".to_string(), "cat".to_string());
        expected.insert("GIT_TERMINAL_PROMPT".to_string(), "0".to_string());
        expected.insert("CI".to_string(), "1".to_string());
        expected.insert("DEBIAN_FRONTEND".to_string(), "noninteractive".to_string());
        expected.insert("LANG".to_string(), "C.UTF-8".to_string());
        expected.insert("LC_ALL".to_string(), "C.UTF-8".to_string());
        #[cfg(unix)]
        expected.insert("GIT_ASKPASS".to_string(), "true".to_string());
        assert_eq!(result, expected);
    }

    #[test]
    fn test_inherit_all_with_default_excludes() {
        let vars = make_vars(&[("PATH", "/usr/bin"), ("API_KEY", "secret")]);

        let policy = ShellEnvironmentPolicy {
            inherit: ShellEnvironmentPolicyInherit::All,
            ignore_default_excludes: false,
            ..Default::default()
        };

        let result = populate_env(vars, &policy);
        let mut expected: HashMap<String, String> = hashmap! {
            "PATH".to_string() => "/usr/bin".to_string(),
            "PAGER".to_string() => "cat".to_string(),
            "GIT_PAGER".to_string() => "cat".to_string(),
            "GH_PAGER".to_string() => "cat".to_string(),
            "GIT_TERMINAL_PROMPT".to_string() => "0".to_string(),
            "CI".to_string() => "1".to_string(),
            "DEBIAN_FRONTEND".to_string() => "noninteractive".to_string(),
            "LANG".to_string() => "C.UTF-8".to_string(),
            "LC_ALL".to_string() => "C.UTF-8".to_string(),
        };
        #[cfg(unix)]
        expected.insert("GIT_ASKPASS".to_string(), "true".to_string());
        assert_eq!(result, expected);
    }

    #[test]
    fn test_inherit_none() {
        let vars = make_vars(&[("PATH", "/usr/bin"), ("HOME", "/home")]);

        let mut policy = ShellEnvironmentPolicy {
            inherit: ShellEnvironmentPolicyInherit::None,
            ignore_default_excludes: true,
            ..Default::default()
        };
        policy
            .r#set
            .insert("ONLY_VAR".to_string(), "yes".to_string());

        let result = populate_env(vars, &policy);
        let mut expected: HashMap<String, String> = hashmap! {
            "ONLY_VAR".to_string() => "yes".to_string(),
            "PAGER".to_string() => "cat".to_string(),
            "GIT_PAGER".to_string() => "cat".to_string(),
            "GH_PAGER".to_string() => "cat".to_string(),
            "GIT_TERMINAL_PROMPT".to_string() => "0".to_string(),
            "CI".to_string() => "1".to_string(),
            "DEBIAN_FRONTEND".to_string() => "noninteractive".to_string(),
            "LANG".to_string() => "C.UTF-8".to_string(),
            "LC_ALL".to_string() => "C.UTF-8".to_string(),
        };

        #[cfg(unix)]
        expected.insert("GIT_ASKPASS".to_string(), "true".to_string());
        assert_eq!(result, expected);
    }
}
