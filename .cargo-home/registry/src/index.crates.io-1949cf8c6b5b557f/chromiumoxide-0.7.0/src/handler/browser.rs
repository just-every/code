use chromiumoxide_cdp::cdp::browser_protocol::browser::BrowserContextId;

/// BrowserContexts provide a way to operate multiple independent browser
/// sessions. When a browser is launched, it has a single BrowserContext used by
/// default.
///
/// If a page opens another page, e.g. with a `window.open` call, the popup will
/// belong to the parent page's browser context.
#[derive(Debug, Clone, Default, Hash, Eq, PartialEq)]
pub struct BrowserContext {
    id: Option<BrowserContextId>,
}

impl BrowserContext {
    /// Whether the BrowserContext is incognito.
    pub fn is_incognito(&self) -> bool {
        self.id.is_some()
    }

    /// The identifier of this context
    pub fn id(&self) -> Option<&BrowserContextId> {
        self.id.as_ref()
    }

    pub(crate) fn take(&mut self) -> Option<BrowserContextId> {
        self.id.take()
    }
}

impl From<BrowserContextId> for BrowserContext {
    fn from(id: BrowserContextId) -> Self {
        Self { id: Some(id) }
    }
}
