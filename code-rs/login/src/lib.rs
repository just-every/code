mod device_code_auth;
mod pkce;
mod server;

pub use device_code_auth::{run_device_code_login, DeviceCodeSession};
pub use server::LoginServer;
pub use server::ServerOptions;
pub use server::ShutdownHandle;
pub use server::run_login_server;

// Re-export commonly used auth types and helpers from codex-core for compatibility
pub use code_app_server_protocol::AuthMode;
pub use code_core::AuthManager;
pub use code_core::CodexAuth;
pub use code_core::auth::AuthDotJson;
pub use code_core::auth::CLIENT_ID;
pub use code_core::auth::CODEX_API_KEY_ENV_VAR;
pub use code_core::auth::OPENAI_API_KEY_ENV_VAR;
pub use code_core::auth::get_auth_file;
pub use code_core::auth::login_with_api_key;
pub use code_core::auth::logout;
pub use code_core::auth::try_read_auth_json;
pub use code_core::auth::write_auth_json;
pub use code_core::token_data::TokenData;
