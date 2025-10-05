//! Compatibility shim for the legacy `codex_app_server_protocol` crate.
//! This crate re-exports the MCP protocol surface that downstream crates
//! still depend on while the fork finishes migrating to the new structure.

pub use codex_protocol::mcp_protocol::{
    self, APPLY_PATCH_APPROVAL_METHOD, AddConversationListenerParams,
    AddConversationSubscriptionResponse, ApplyPatchApprovalParams, ApplyPatchApprovalResponse,
    ArchiveConversationParams, ArchiveConversationResponse, AuthMode, CancelLoginChatGptParams,
    CancelLoginChatGptResponse, ClientInfo, ClientNotification, ClientRequest, ConversationId,
    EXEC_COMMAND_APPROVAL_METHOD, ExecCommandApprovalParams, ExecCommandApprovalResponse,
    GetAuthStatusParams, GetAuthStatusResponse, GetUserAgentResponse, GetUserSavedConfigResponse,
    GitDiffToRemoteParams, GitDiffToRemoteResponse, GitSha, InitializeParams, InputItem,
    InterruptConversationParams, InterruptConversationResponse, ListConversationsParams,
    ListConversationsResponse, LoginApiKeyParams, LoginChatGptCompleteNotification,
    LoginChatGptResponse, LogoutChatGptParams, LogoutChatGptResponse, NewConversationParams,
    NewConversationResponse, Profile, RemoveConversationListenerParams,
    RemoveConversationSubscriptionResponse, ResumeConversationParams, ResumeConversationResponse,
    SandboxSettings, SendUserMessageParams, SendUserMessageResponse, SendUserTurnParams,
    SendUserTurnResponse, ServerNotification, ServerRequest, SessionConfiguredNotification,
    SetDefaultModelParams, SetDefaultModelResponse, Tools, UserInfoResponse, UserSavedConfig,
};
pub use mcp_types::{
    JSONRPCError, JSONRPCErrorError, JSONRPCMessage, JSONRPCNotification, JSONRPCRequest,
    JSONRPCResponse, RequestId,
};
