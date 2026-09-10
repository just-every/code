use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Weak;

use codex_model_provider_info::ModelProviderAwsAuthInfo;

use crate::amazon_bedrock::AwsAuthRecovery;
use crate::amazon_bedrock::AwsCredentialExport;

/// Provider-owned runtime state shared across independently configured sessions.
#[derive(Debug, Default)]
pub(crate) struct ModelProviderSharedState {
    aws_credential_exports: Mutex<Vec<(ModelProviderAwsAuthInfo, Weak<AwsCredentialExport>)>>,
    aws_auth_recoveries: Mutex<Vec<(ModelProviderAwsAuthInfo, Weak<AwsAuthRecovery>)>>,
}

pub(crate) fn process_shared_state() -> &'static ModelProviderSharedState {
    static STATE: OnceLock<ModelProviderSharedState> = OnceLock::new();
    STATE.get_or_init(ModelProviderSharedState::default)
}

impl ModelProviderSharedState {
    pub(crate) fn aws_credential_export(
        &self,
        aws: &ModelProviderAwsAuthInfo,
    ) -> Option<Arc<AwsCredentialExport>> {
        let config = aws.credential_export.as_ref()?;
        let mut exports = self
            .aws_credential_exports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        exports.retain(|(_, export)| export.strong_count() != 0);

        if let Some(export) = exports
            .iter()
            .find(|(cached_aws, _)| cached_aws == aws)
            .and_then(|(_, export)| export.upgrade())
        {
            return Some(export);
        }

        let export = Arc::new(AwsCredentialExport::new(config.clone()));
        exports.push((aws.clone(), Arc::downgrade(&export)));
        Some(export)
    }

    pub(crate) fn aws_auth_recovery(
        &self,
        aws: &ModelProviderAwsAuthInfo,
    ) -> Option<Arc<AwsAuthRecovery>> {
        let config = aws.auth_refresh.as_ref()?;
        let mut recoveries = self
            .aws_auth_recoveries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recoveries.retain(|(_, recovery)| recovery.strong_count() != 0);

        if let Some(recovery) = recoveries
            .iter()
            .find(|(cached_aws, _)| cached_aws == aws)
            .and_then(|(_, recovery)| recovery.upgrade())
        {
            return Some(recovery);
        }

        let recovery = Arc::new(AwsAuthRecovery::new(config.clone()));
        recoveries.push((aws.clone(), Arc::downgrade(&recovery)));
        Some(recovery)
    }
}
