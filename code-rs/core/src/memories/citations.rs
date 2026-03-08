use uuid::Uuid;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct MemoryCitationInfo {
    pub(crate) rollout_ids: Vec<Uuid>,
}

const OPEN_TAG: &str = "<oai-mem-citation>";
const CLOSE_TAG: &str = "</oai-mem-citation>";
const ROLLOUT_IDS_OPEN: &str = "<rollout_ids>";
const ROLLOUT_IDS_CLOSE: &str = "</rollout_ids>";

pub(crate) fn strip_memory_citations(text: &str) -> (String, MemoryCitationInfo) {
    let mut cleaned = String::with_capacity(text.len());
    let mut citations = MemoryCitationInfo::default();
    let mut seen_rollout_ids = std::collections::HashSet::new();
    let mut cursor = 0usize;

    while let Some(open_rel) = text[cursor..].find(OPEN_TAG) {
        let open_idx = cursor + open_rel;
        cleaned.push_str(&text[cursor..open_idx]);
        let inner_start = open_idx + OPEN_TAG.len();
        let Some(close_rel) = text[inner_start..].find(CLOSE_TAG) else {
            return (cleaned, citations);
        };
        let inner_end = inner_start + close_rel;
        for rollout_id in parse_rollout_ids(&text[inner_start..inner_end]) {
            if seen_rollout_ids.insert(rollout_id) {
                citations.rollout_ids.push(rollout_id);
            }
        }
        cursor = inner_end + CLOSE_TAG.len();
    }

    cleaned.push_str(&text[cursor..]);
    (cleaned, citations)
}

fn parse_rollout_ids(inner: &str) -> Vec<Uuid> {
    let Some(open_rel) = inner.find(ROLLOUT_IDS_OPEN) else {
        return Vec::new();
    };
    let ids_start = open_rel + ROLLOUT_IDS_OPEN.len();
    let Some(close_rel) = inner[ids_start..].find(ROLLOUT_IDS_CLOSE) else {
        return Vec::new();
    };
    inner[ids_start..ids_start + close_rel]
        .lines()
        .filter_map(|line| Uuid::parse_str(line.trim()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_memory_citation_and_extracts_rollout_ids() {
        let one = Uuid::new_v4();
        let two = Uuid::new_v4();
        let text = format!(
            "hello\n<oai-mem-citation><rollout_ids>\n{one}\n{two}\n</rollout_ids></oai-mem-citation>"
        );
        let (cleaned, citation) = strip_memory_citations(&text);
        assert_eq!(cleaned, "hello\n");
        assert_eq!(citation.rollout_ids, vec![one, two]);
    }
}
