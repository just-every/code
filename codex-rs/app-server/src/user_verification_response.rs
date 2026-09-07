//! Device proofs travel only in elicitation content, never in diagnostic metadata.

use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequestResponse;
use codex_app_server_protocol::UserVerificationProof;
use serde::Deserialize;
use tokio::sync::oneshot;

use crate::outgoing_message::ClientRequestResult;

pub(crate) fn from_client_result(
    result: Result<ClientRequestResult, oneshot::error::RecvError>,
) -> McpServerElicitationRequestResponse {
    let response = result.ok().and_then(Result::ok).and_then(|value| {
        serde_json::from_value::<McpServerElicitationRequestResponse>(value).ok()
    });
    if let Some(mut response) = response {
        response.meta = None;
        match response.action {
            McpServerElicitationAction::Accept => {
                if response
                    .content
                    .as_ref()
                    .is_some_and(|content| UserVerificationProof::deserialize(content).is_ok())
                {
                    // The MCP boundary validates the encoded proof and size limits.
                    return response;
                }
            }
            McpServerElicitationAction::Decline | McpServerElicitationAction::Cancel => {
                response.content = None;
                return response;
            }
        }
    }
    McpServerElicitationRequestResponse {
        action: McpServerElicitationAction::Cancel,
        content: None,
        meta: None,
    }
}

#[cfg(test)]
#[path = "user_verification_response_tests.rs"]
mod tests;
