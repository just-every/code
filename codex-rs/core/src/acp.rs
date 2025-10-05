#![allow(dead_code)]

use agent_client_protocol as acp;
use anyhow::Context as _;
use anyhow::Result;
use codex_apply_patch::FileSystem;
use codex_apply_patch::StdFileSystem;
use mcp_types::CallToolResult;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;

use crate::config_types::{ClientTools, McpToolId};
use crate::mcp_connection_manager::McpConnectionManager;
use crate::protocol::FileChange;
use crate::protocol::ReviewDecision;
use crate::util::strip_bash_lc_and_escape;

pub(crate) struct AcpFileSystem<'a> {
    session_id: Uuid,
    mcp_connection_manager: &'a McpConnectionManager,
    tools: &'a ClientTools,
}

impl<'a> AcpFileSystem<'a> {
    pub fn new(
        session_id: Uuid,
        tools: &'a ClientTools,
        mcp_connection_manager: &'a McpConnectionManager,
    ) -> Self {
        Self {
            session_id,
            mcp_connection_manager,
            tools,
        }
    }

    async fn read_text_file_impl(
        &self,
        tool: &McpToolId,
        path: &Path,
    ) -> Result<String> {
        let arguments = acp::ReadTextFileRequest {
            session_id: acp::SessionId(self.session_id.to_string().into()),
            path: path.to_path_buf(),
            line: None,
            limit: None,
            meta: None,
        };

        let CallToolResult {
            structured_content,
            is_error,
            ..
        } = self
            .mcp_connection_manager
            .call_tool(
                &tool.mcp_server,
                &tool.tool_name,
                Some(serde_json::to_value(arguments).unwrap_or_default()),

            )
            .await?;

        if is_error.unwrap_or_default() {
            anyhow::bail!("Error reading text file: {:?}", structured_content);
        }

        let output = serde_json::from_value::<acp::ReadTextFileResponse>(
            structured_content.context("No output from read_text_file tool")?,
        )?;

        Ok(output.content)
    }

    async fn write_text_file_impl(
        &self,
        tool: &McpToolId,
        path: &Path,
        content: String,
    ) -> Result<()> {
        let arguments = acp::WriteTextFileRequest {
            session_id: acp::SessionId(self.session_id.to_string().into()),
            path: path.to_path_buf(),
            content,
            meta: None,
        };

        let CallToolResult {
            structured_content,
            is_error,
            ..
        } = self
            .mcp_connection_manager
            .call_tool(
                &tool.mcp_server,
                &tool.tool_name,
                Some(serde_json::to_value(arguments).unwrap_or_default()),

            )
            .await?;

        if is_error.unwrap_or_default() {
            anyhow::bail!("Error writing text file: {:?}", structured_content);
        }

        Ok(())
    }
}

impl<'a> FileSystem for AcpFileSystem<'a> {
    async fn read_text_file(&self, path: &Path) -> std::io::Result<String> {
        if let Some(tool) = self.tools.read_text_file.as_ref() {
            self.read_text_file_impl(tool, path)
                .await
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
        } else {
            StdFileSystem.read_text_file(path).await
        }
    }

    async fn write_text_file(&self, path: &Path, contents: String) -> std::io::Result<()> {
        if let Some(tool) = self.tools.write_text_file.as_ref() {
            self.write_text_file_impl(tool, path, contents)
                .await
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
        } else {
            StdFileSystem.write_text_file(path, contents).await
        }
    }
}

pub(crate) async fn request_permission(
    permission_tool: &McpToolId,
    tool_call: acp::ToolCallUpdate,
    session_id: Uuid,
    mcp_connection_manager: &McpConnectionManager,
) -> Result<ReviewDecision> {
    let approve_for_session_id = acp::PermissionOptionId("approve_for_session".into());
    let approve_id = acp::PermissionOptionId("approve".into());
    let deny_id = acp::PermissionOptionId("deny".into());

    let arguments = acp::RequestPermissionRequest {
        session_id: acp::SessionId(session_id.to_string().into()),
        tool_call,
        options: vec![
            acp::PermissionOption {
                id: approve_for_session_id.clone(),
                name: "Approve for Session".into(),
                kind: acp::PermissionOptionKind::AllowAlways,
                meta: None,
            },
            acp::PermissionOption {
                id: approve_id.clone(),
                name: "Approve".into(),
                kind: acp::PermissionOptionKind::AllowOnce,
                meta: None,
            },
            acp::PermissionOption {
                id: deny_id.clone(),
                name: "Deny".into(),
                kind: acp::PermissionOptionKind::RejectOnce,
                meta: None,
            },
        ],
        meta: None,
    };

    let CallToolResult {
        structured_content, ..
    } = mcp_connection_manager
        .call_tool(
            &permission_tool.mcp_server,
            &permission_tool.tool_name,
            Some(serde_json::to_value(arguments).unwrap_or_default()),
        )
        .await?;

    let result = structured_content.context("No output from permission tool")?;
    let result = serde_json::from_value::<acp::RequestPermissionResponse>(result)?;

    use acp::RequestPermissionOutcome::*;
    let decision = match result.outcome {
        Selected { option_id } => {
            if option_id == approve_id {
                ReviewDecision::Approved
            } else if option_id == approve_for_session_id {
                ReviewDecision::ApprovedForSession
            } else if option_id == deny_id {
                ReviewDecision::Denied
            } else {
                anyhow::bail!("Unexpected permission option: {}", option_id);
            }
        }
        Cancelled => ReviewDecision::Abort,
    };

    Ok(decision)
}

pub fn new_execute_tool_call(
    call_id: &str,
    command: &[String],
    status: acp::ToolCallStatus,
) -> acp::ToolCall {
    acp::ToolCall {
        id: acp::ToolCallId(call_id.into()),
        title: format!("`{}`", strip_bash_lc_and_escape(command)),
        kind: acp::ToolKind::Execute,
        status,
        content: vec![],
        locations: vec![],
        raw_input: None,
        raw_output: None,
        meta: None,
    }
}

pub fn new_patch_tool_call(
    call_id: &str,
    changes: &HashMap<PathBuf, FileChange>,
    status: acp::ToolCallStatus,
) -> acp::ToolCall {
    let title = if changes.len() == 1
        && let Some((path, change)) = changes.iter().next()
    {
        let file_name = path.file_name().unwrap_or_default().display().to_string();

        match change {
            FileChange::Delete { content: _ } => {
                return acp::ToolCall {
                    id: acp::ToolCallId(call_id.into()),
                    title: format!("Delete “`{file_name}`”"),
                    kind: acp::ToolKind::Delete,
                    status,
                    content: vec![],
                    locations: vec![],
                    raw_input: None,
                    raw_output: None,
                    meta: None,
                };
            }
            FileChange::Update {
                move_path: Some(new_path),
                original_content,
                new_content,
                ..
            } if original_content == new_content => {
                return acp::ToolCall {
                    id: acp::ToolCallId(call_id.into()),
                    title: move_path_label(path, new_path),
                    kind: acp::ToolKind::Move,
                    status,
                    content: vec![],
                    locations: vec![],
                    raw_input: None,
                    raw_output: None,
                    meta: None,
                };
            }
            _ => {}
        }

        format!("Edit “`{file_name}`”")
    } else {
        format!("Edit {} files", changes.len())
    };

    let mut locations = Vec::with_capacity(changes.len());
    let mut content = Vec::with_capacity(changes.len());

    for (path, change) in changes.iter() {
        match change {
            FileChange::Add { content: new_content } => {
                content.push(acp::ToolCallContent::Diff {
                    diff: acp::Diff {
                        path: path.clone(),
                        old_text: None,
                        new_text: new_content.clone(),
                        meta: None,
                    },
                });

                locations.push(acp::ToolCallLocation {
                    path: path.clone(),
                    line: None,
                    meta: None,
                });
            }
            FileChange::Delete { content: _ } => {
                content.push(
                    format!(
                        "Delete “`{}`”\n\n",
                        path.file_name().unwrap_or(path.as_os_str()).display()
                    )
                    .into(),
                );
            }
            FileChange::Update {
                move_path,
                new_content,
                original_content,
                unified_diff: _,
            } => {
                if let Some(new_path) = move_path
                    && changes.len() > 1
                {
                    content.push(move_path_label(path, new_path).into());

                    if status == acp::ToolCallStatus::Completed {
                        locations.push(acp::ToolCallLocation {
                            path: new_path.clone(),
                            line: None,
                            meta: None,
                        });
                    } else {
                        locations.push(acp::ToolCallLocation {
                            path: path.clone(),
                            line: None,
                            meta: None,
                        });
                    }
                } else {
                    locations.push(acp::ToolCallLocation {
                        path: path.clone(),
                        line: None,
                        meta: None,
                    });
                }

                if original_content != new_content {
                    content.push(acp::ToolCallContent::Diff {
                        diff: acp::Diff {
                            path: path.clone(),
                            old_text: Some(original_content.clone()),
                            new_text: new_content.clone(),
                            meta: None,
                        },
                    });
                }
            }
        }
    }

    acp::ToolCall {
        id: acp::ToolCallId(call_id.into()),
        title,
        kind: acp::ToolKind::Edit,
        status,
        content,
        locations,
        raw_input: None,
        raw_output: None,
        meta: None,
    }
}

fn move_path_label(old: &Path, new: &Path) -> String {
    if old.parent() == new.parent() {
        let old_name = old.file_name().unwrap_or(old.as_os_str()).display();
        let new_name = new.file_name().unwrap_or(new.as_os_str()).display();

        format!("Rename “`{old_name}`” to “`{new_name}`”")
    } else {
        format!("Move “`{}`” to “`{}`”", old.display(), new.display())
    }
}
