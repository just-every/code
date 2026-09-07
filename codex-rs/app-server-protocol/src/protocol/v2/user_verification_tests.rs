use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn local_readiness_retains_credential_during_biometric_unavailability() {
    let status = UserVerificationStatusResponse {
        credential_id: Some("credential".into()),
        unavailable_reason: Some(UserVerificationUnavailableReason::BiometricsUnavailable),
        unavailable_message: Some("Unlock biometrics to continue.".into()),
    };
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        json!({
            "credentialId": "credential",
            "unavailableReason": "biometricsUnavailable",
            "unavailableMessage": "Unlock biometrics to continue."
        })
    );
    assert_eq!(
        serde_json::from_value::<UserVerificationStatusResponse>(
            serde_json::to_value(&status).unwrap()
        )
        .unwrap(),
        status
    );
}

#[test]
fn error_details_reject_cross_category_reasons_and_native_payloads() {
    let details = UserVerificationErrorDetails::Cancelled {
        reason: UserVerificationCancellationReason::UserCancelled,
    };
    assert_eq!(
        serde_json::to_value(details).unwrap(),
        json!({"type": "cancelled", "reason": "userCancelled"})
    );
    for invalid in [
        json!({"type": "failed", "reason": "userCancelled"}),
        json!({"type": "unavailable", "reason": "providerUnavailable", "nativeError": {"code": 1}}),
    ] {
        assert!(serde_json::from_value::<UserVerificationErrorDetails>(invalid).is_err());
    }
}
