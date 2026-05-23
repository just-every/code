const CITATION_OPEN: &str = "<oai-mem-citation>";
const CITATION_CLOSE: &str = "</oai-mem-citation>";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct MemoryCitationChunk {
    pub(crate) visible_text: String,
    pub(crate) citations: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryCitationParser {
    pending: String,
    active_citation: String,
    inside_citation: bool,
}

impl MemoryCitationParser {
    pub(crate) fn push_str(&mut self, chunk: &str) -> MemoryCitationChunk {
        let mut input = std::mem::take(&mut self.pending);
        input.push_str(chunk);

        let mut visible_text = String::new();
        let mut citations = Vec::new();
        let mut cursor = 0usize;

        loop {
            let rest = &input[cursor..];
            if rest.is_empty() {
                break;
            }

            if self.inside_citation {
                if let Some(close_idx) = rest.find(CITATION_CLOSE) {
                    self.active_citation.push_str(&rest[..close_idx]);
                    citations.push(std::mem::take(&mut self.active_citation));
                    self.inside_citation = false;
                    cursor += close_idx + CITATION_CLOSE.len();
                    continue;
                }

                let overlap = longest_suffix_prefix(rest, CITATION_CLOSE);
                let split_at = rest.len().saturating_sub(overlap);
                self.active_citation.push_str(&rest[..split_at]);
                self.pending.push_str(&rest[split_at..]);
                break;
            }

            if let Some(open_idx) = rest.find(CITATION_OPEN) {
                visible_text.push_str(&rest[..open_idx]);
                self.inside_citation = true;
                cursor += open_idx + CITATION_OPEN.len();
                continue;
            }

            let overlap = longest_suffix_prefix(rest, CITATION_OPEN);
            let split_at = rest.len().saturating_sub(overlap);
            visible_text.push_str(&rest[..split_at]);
            self.pending.push_str(&rest[split_at..]);
            break;
        }

        MemoryCitationChunk {
            visible_text,
            citations,
        }
    }

    pub(crate) fn finish(&mut self) -> MemoryCitationChunk {
        let mut out = self.push_str("");

        if self.inside_citation {
            self.active_citation.push_str(&self.pending);
            self.pending.clear();
            out.citations.push(std::mem::take(&mut self.active_citation));
            self.inside_citation = false;
        } else if !self.pending.is_empty() {
            out.visible_text.push_str(&self.pending);
            self.pending.clear();
        }

        out
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.active_citation.clear();
        self.inside_citation = false;
    }
}

pub(crate) fn strip_memory_citations(text: &str) -> MemoryCitationChunk {
    let mut parser = MemoryCitationParser::default();
    let mut out = parser.push_str(text);
    let tail = parser.finish();
    out.visible_text.push_str(&tail.visible_text);
    out.citations.extend(tail.citations);
    out
}

fn longest_suffix_prefix(input: &str, pattern: &str) -> usize {
    let max_overlap = input.len().min(pattern.len().saturating_sub(1));
    (1..=max_overlap)
        .rev()
        .find(|len| input.ends_with(&pattern[..*len]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::MemoryCitationParser;
    use super::strip_memory_citations;
    use pretty_assertions::assert_eq;

    #[test]
    fn strips_memory_citations_from_single_message() {
        let parsed = strip_memory_citations(
            "hello <oai-mem-citation>MEMORY.md:1-2|note=[x]</oai-mem-citation> world",
        );

        assert_eq!(parsed.visible_text, "hello  world");
        assert_eq!(
            parsed.citations,
            vec!["MEMORY.md:1-2|note=[x]".to_string()]
        );
    }

    #[test]
    fn strips_memory_citations_across_delta_boundaries() {
        let mut parser = MemoryCitationParser::default();

        let first = parser.push_str("hello <oai-mem-cit");
        let second = parser.push_str("ation>doc1</oai-mem-citation> world");
        let tail = parser.finish();

        assert_eq!(first.visible_text, "hello ");
        assert!(first.citations.is_empty());
        assert_eq!(second.visible_text, " world");
        assert_eq!(second.citations, vec!["doc1".to_string()]);
        assert_eq!(tail.visible_text, "");
        assert!(tail.citations.is_empty());
    }

    #[test]
    fn auto_closes_unterminated_memory_citation_at_finish() {
        let mut parser = MemoryCitationParser::default();

        let first = parser.push_str("x<oai-mem-citation>doc");
        let tail = parser.finish();

        assert_eq!(first.visible_text, "x");
        assert!(first.citations.is_empty());
        assert_eq!(tail.visible_text, "");
        assert_eq!(tail.citations, vec!["doc".to_string()]);
    }
}
