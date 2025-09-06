use chromiumoxide_cdp::cdp::browser_protocol::target::{SessionId, TargetId};

/// Represents a Session within the cpd.
#[derive(Debug, Clone)]
pub struct Session {
    /// Identifier for this session.
    id: SessionId,
    /// The identifier of the target this session is attached to.
    target_id: TargetId,
}
impl Session {
    pub fn new(id: SessionId, target_id: TargetId) -> Self {
        Self { id, target_id }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.id
    }

    pub fn target_id(&self) -> &TargetId {
        &self.target_id
    }
}
