//! Covers the saved Daybreak preference independently of transcript history.

use codex_protocol::ThreadId;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use uuid::Uuid;

use super::LocalThreadStore;
use super::test_support::test_config;
use super::test_support::write_session_file;
use crate::ReadThreadParams;
use crate::ThreadMetadataPatch;
use crate::ThreadStore;
use crate::ThreadStoreError;
use crate::UpdateThreadMetadataParams;

#[tokio::test]
async fn daybreak_preference_survives_reconciliation_and_cold_reads()
-> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    let config = test_config(home.path());
    let uuid = Uuid::new_v4();
    let thread_id = ThreadId::from_string(&uuid.to_string())?;
    let rollout_path = write_session_file(home.path(), "2025-01-03T14-20-00", uuid)?;
    let original_rollout = std::fs::read(&rollout_path)?;
    let runtime = codex_state::StateRuntime::init(
        config.sqlite.clone(),
        config.default_model_provider_id.clone(),
    )
    .await?;
    let store = LocalThreadStore::new(config.clone(), Some(runtime.clone()));
    for daybreak_enabled in [true, false] {
        let updated = store
            .update_thread_metadata(UpdateThreadMetadataParams {
                thread_id,
                patch: ThreadMetadataPatch {
                    daybreak_enabled: Some(daybreak_enabled),
                    ..Default::default()
                },
                include_archived: false,
            })
            .await?
            .expect("updated thread");
        assert_eq!(updated.daybreak_enabled, Some(daybreak_enabled));
    }
    let mut stale_metadata = runtime
        .get_thread(thread_id)
        .await?
        .expect("saved metadata");
    stale_metadata.daybreak_enabled = Some(true);
    runtime.upsert_thread(&stale_metadata).await?;
    codex_rollout::state_db::reconcile_rollout(
        Some(runtime.as_ref()),
        &rollout_path,
        &config.default_model_provider_id,
        /*builder*/ None,
        &[],
        /*archived_only*/ None,
        /*new_thread_memory_mode*/ None,
    )
    .await;
    drop(store);
    drop(runtime);

    let runtime = codex_state::StateRuntime::init(
        config.sqlite.clone(),
        config.default_model_provider_id.clone(),
    )
    .await?;
    let store = LocalThreadStore::new(config, Some(runtime));
    for include_history in [false, true] {
        let thread = store
            .read_thread(ReadThreadParams {
                thread_id,
                include_archived: false,
                include_history,
            })
            .await?;
        assert_eq!(thread.daybreak_enabled, Some(false));
    }
    assert_eq!(std::fs::read(rollout_path)?, original_rollout);
    Ok(())
}

#[tokio::test]
async fn daybreak_preference_requires_sqlite() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    let uuid = Uuid::new_v4();
    let thread_id = ThreadId::from_string(&uuid.to_string())?;
    write_session_file(home.path(), "2025-01-03T14-20-00", uuid)?;
    let store = LocalThreadStore::new(test_config(home.path()), /*state_db*/ None);

    let result = store
        .update_thread_metadata(UpdateThreadMetadataParams {
            thread_id,
            patch: ThreadMetadataPatch {
                daybreak_enabled: Some(false),
                ..Default::default()
            },
            include_archived: false,
        })
        .await;
    assert!(matches!(result, Err(ThreadStoreError::Internal { .. })));
    Ok(())
}
