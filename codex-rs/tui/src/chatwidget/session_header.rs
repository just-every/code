#[derive(Default, Clone)]
pub(crate) struct SessionHeader {
    model: String,
    reasoning: Option<codex_core::config_types::ReasoningEffort>,
    manual_title: Option<String>,
    tags: Vec<String>,
}

impl SessionHeader {
    pub(crate) fn new(model: String) -> Self {
        Self {
            model,
            ..Self::default()
        }
    }

    pub(crate) fn set_model(&mut self, model: &str) {
        if self.model != model {
            self.model = model.to_string();
        }
    }

    pub(crate) fn set_reasoning(&mut self, reasoning: codex_core::config_types::ReasoningEffort) {
        if self.reasoning != Some(reasoning) {
            self.reasoning = Some(reasoning);
        }
    }

    pub(crate) fn set_manual_title(&mut self, title: Option<String>) {
        if self.manual_title != title {
            self.manual_title = title;
        }
    }

    pub(crate) fn manual_title(&self) -> Option<&str> {
        self.manual_title.as_deref()
    }

    pub(crate) fn model(&self) -> &str {
        &self.model
    }

    pub(crate) fn tags(&self) -> &[String] {
        &self.tags
    }

    pub(crate) fn set_custom_tags(&mut self, tags: Vec<String>) {
        self.tags = tags;
    }
}
