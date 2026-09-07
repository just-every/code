//! Cache the running executor's build identity before accepting connections.

use std::sync::LazyLock;

use codex_build_info::BuildInfo;
use codex_build_info::build_id;

use crate::protocol::EnvironmentInfo;

pub(super) static PROVIDER_ID: LazyLock<Option<String>> = LazyLock::new(|| {
    let info = BuildInfo::get();
    info.target()
        .and_then(|target| build_id(info.build_commit(), target))
});

pub(super) fn local_environment_info() -> EnvironmentInfo {
    EnvironmentInfo {
        provider_id: PROVIDER_ID.clone(),
        ..super::release_version::local_environment_info()
    }
}
