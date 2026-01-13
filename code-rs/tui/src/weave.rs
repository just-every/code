use crate::weave_client::WeaveAgent;
use std::collections::HashMap;
use std::collections::HashSet;

pub(crate) fn find_weave_mentions(
    text: &str,
    agents: &[WeaveAgent],
    self_agent_id: Option<&str>,
) -> Vec<WeaveAgent> {
    let mut mention_lookup: HashMap<String, WeaveAgent> = HashMap::new();
    for agent in agents {
        mention_lookup
            .entry(agent.mention_text().to_ascii_lowercase())
            .or_insert_with(|| agent.clone());
    }

    let mut recipients = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut ctx = WeaveTokenContext {
        text,
        mention_lookup: &mention_lookup,
        recipients: &mut recipients,
        seen_ids: &mut seen_ids,
        self_agent_id,
    };

    let mut token_start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = token_start.take() {
                process_weave_token(&mut ctx, start, idx);
            }
            continue;
        }
        if token_start.is_none() {
            token_start = Some(idx);
        }
    }
    if let Some(start) = token_start {
        process_weave_token(&mut ctx, start, text.len());
    }

    recipients
}

struct WeaveTokenContext<'a> {
    text: &'a str,
    mention_lookup: &'a HashMap<String, WeaveAgent>,
    recipients: &'a mut Vec<WeaveAgent>,
    seen_ids: &'a mut HashSet<String>,
    self_agent_id: Option<&'a str>,
}

fn process_weave_token(ctx: &mut WeaveTokenContext<'_>, start: usize, end: usize) {
    let token = &ctx.text[start..end];
    let Some(mention) = parse_weave_mention_label(token) else {
        return;
    };
    let Some(agent) = ctx.mention_lookup.get(&mention.to_ascii_lowercase()) else {
        return;
    };
    if ctx.self_agent_id == Some(agent.id.as_str()) {
        return;
    }
    if ctx.seen_ids.insert(agent.id.clone()) {
        ctx.recipients.push(agent.clone());
    }
}

pub(crate) fn parse_weave_mention_label(token: &str) -> Option<&str> {
    let Some(rest) = token.strip_prefix('#') else {
        return None;
    };
    let mut end = 0;
    for (idx, ch) in rest.char_indices() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_weave_mentions_finds_named_agents() {
        let agents = vec![
            WeaveAgent {
                id: "alice-id".to_string(),
                name: Some("alice".to_string()),
            },
            WeaveAgent {
                id: "bob-id".to_string(),
                name: Some("bob".to_string()),
            },
        ];

        let recipients = find_weave_mentions("hi #alice and #bob", &agents, None);

        assert_eq!(recipients.len(), 2);
        assert_eq!(recipients[0].id, "alice-id");
        assert_eq!(recipients[1].id, "bob-id");
    }

    #[test]
    fn find_weave_mentions_dedupes_and_skips_self() {
        let agents = vec![
            WeaveAgent {
                id: "me".to_string(),
                name: Some("me".to_string()),
            },
            WeaveAgent {
                id: "other".to_string(),
                name: Some("other".to_string()),
            },
        ];

        let recipients = find_weave_mentions("#me #other #other", &agents, Some("me"));

        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].id, "other");
    }

    #[test]
    fn find_weave_mentions_matches_id_when_name_is_not_mentionable() {
        let agents = vec![WeaveAgent {
            id: "agent-123".to_string(),
            name: Some("alice smith".to_string()),
        }];

        let recipients = find_weave_mentions("#agent-123", &agents, None);

        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].id, "agent-123");
    }

    #[test]
    fn find_weave_mentions_is_case_insensitive_and_strips_trailing_punctuation() {
        let agents = vec![WeaveAgent {
            id: "bob-id".to_string(),
            name: Some("bob".to_string()),
        }];

        let recipients = find_weave_mentions("hi #BoB, how are you?", &agents, None);

        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].id, "bob-id");
    }
}
