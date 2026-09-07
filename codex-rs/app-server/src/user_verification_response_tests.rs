use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn accepted_device_proof_uses_existing_content_and_discards_metadata() {
    assert_eq!(
        from_client_result(Ok(Ok(json!({
            "action": "accept", "content": {"credentialId": "AQID", "signature": "BAUG"},
            "_meta": {"ignored": "untrusted"},
        })))),
        McpServerElicitationRequestResponse {
            action: McpServerElicitationAction::Accept,
            content: Some(json!({"credentialId": "AQID", "signature": "BAUG"})),
            meta: None,
        }
    );
}

#[test]
fn invalid_response_or_accept_without_proof_cancels() {
    for response in [
        json!({"invalid": "response"}),
        json!({"action": "accept", "content": null, "_meta": null}),
        json!({"action": "accept", "content": {}, "_meta": null}),
    ] {
        assert_eq!(
            from_client_result(Ok(Ok(response))),
            McpServerElicitationRequestResponse {
                action: McpServerElicitationAction::Cancel,
                content: None,
                meta: None,
            }
        );
    }
}
