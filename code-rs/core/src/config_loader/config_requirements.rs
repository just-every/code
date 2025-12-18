use crate::config::Constrained;
use crate::config::ConstraintError;
use crate::protocol::AskForApproval;
use serde::Deserialize;

/// Normalized version of [`ConfigRequirementsToml`] after deserialization and normalization.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConfigRequirements {
    pub(crate) approval_policy: Constrained<AskForApproval>,
}

impl Default for ConfigRequirements {
    fn default() -> Self {
        Self {
            approval_policy: Constrained::allow_any_from_default(),
        }
    }
}

/// Base config deserialized from /etc/code/requirements.toml or MDM.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
pub(crate) struct ConfigRequirementsToml {
    pub allowed_approval_policies: Option<Vec<AskForApproval>>,
}

impl TryFrom<ConfigRequirementsToml> for ConfigRequirements {
    type Error = ConstraintError;

    fn try_from(toml: ConfigRequirementsToml) -> Result<Self, Self::Error> {
        let approval_policy: Constrained<AskForApproval> = match toml.allowed_approval_policies {
            Some(policies) => {
                let default_value = AskForApproval::default();
                if policies.contains(&default_value) {
                    Constrained::allow_values(default_value, policies)?
                } else if let Some(first) = policies.first() {
                    Constrained::allow_values(*first, policies)?
                } else {
                    return Err(ConstraintError::empty_field("allowed_approval_policies"));
                }
            }
            None => Constrained::allow_any_from_default(),
        };
        Ok(ConfigRequirements { approval_policy })
    }
}

/// The legacy mechanism for specifying admin-enforced configuration is to
/// provide a file like `/etc/code/managed_config.toml` that has the same
/// structure as `config.toml`, where fields like `approval_policy` specify a
/// single value rather than a set of allowed values.
///
/// When present, this can be treated as a `requirements.toml` where each field
/// is constrained to its specified value.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
pub(crate) struct LegacyManagedConfigToml {
    pub(crate) approval_policy: Option<AskForApproval>,
}

