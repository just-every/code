use super::*;
use base64::Engine;
use pretty_assertions::assert_eq;
use serde_json::json;

fn chatgpt_auth(user: &str, workspace: &str, token: &str) -> CodexAuth {
    let claims = json!({
        "jti": token,
        "https://api.openai.com/auth": {"chatgpt_user_id": user},
    });
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
    CodexAuth::from_external_chatgpt_tokens(
        &format!("header.{payload}.signature"),
        workspace,
        /*chatgpt_plan_type*/ None,
    )
    .unwrap()
}

#[test]
fn auth_owner_generation_distinguishes_refreshes_from_identity_changes() {
    let manager = AuthManager::from_optional_auth_for_testing(/*auth*/ None);
    let mut changes = manager.auth_change_state_receiver();
    let mut legacy_changes = manager.auth_change_receiver();
    let initial = chatgpt_auth("user-a", "workspace-a", "token-1");
    for (auth, generation, owner_generation) in [
        (Some(initial.clone()), 1, 1),
        (Some(initial), 1, 1),
        (Some(chatgpt_auth("user-a", "workspace-a", "token-2")), 2, 1),
        (Some(chatgpt_auth("user-b", "workspace-a", "token-3")), 3, 2),
        (Some(chatgpt_auth("user-b", "workspace-b", "token-4")), 4, 3),
        (Some(chatgpt_auth("", "workspace-b", "token-5")), 5, 4),
        (Some(chatgpt_auth("", "workspace-b", "token-6")), 6, 5),
        (Some(CodexAuth::from_api_key("key-1")), 7, 6),
        (Some(CodexAuth::from_api_key("key-2")), 8, 7),
    ] {
        manager.set_cached_auth(auth);
        assert_eq!(
            *changes.borrow_and_update(),
            AuthChangeState {
                generation,
                owner_generation,
            },
        );
        assert_eq!(*legacy_changes.borrow_and_update(), generation);
    }
}

#[tokio::test]
async fn auth_owner_generation_preserves_logout_and_switches_when_coalesced() {
    let home = tempfile::tempdir().unwrap();
    let initial = chatgpt_auth("user-a", "workspace-a", "token-1");
    let manager =
        AuthManager::from_auth_for_testing_with_home(initial.clone(), home.path().to_path_buf());
    let mut changes = manager.auth_change_state_receiver();

    manager.logout().await.unwrap();
    manager.set_cached_auth(Some(initial.clone()));
    assert_eq!(
        *changes.borrow_and_update(),
        AuthChangeState {
            generation: 2,
            owner_generation: 2,
        },
    );

    manager.set_cached_auth(Some(chatgpt_auth("user-b", "workspace-a", "token-2")));
    manager.set_cached_auth(Some(initial));
    assert_eq!(
        *changes.borrow_and_update(),
        AuthChangeState {
            generation: 4,
            owner_generation: 4,
        },
    );
}
