use code_arg0::arg0_dispatch_or_else;
use code_common::CliConfigOverrides;
use code_mcp_server::run_main;

fn main() -> anyhow::Result<()> {
    code_utils_rustls_provider::ensure_rustls_crypto_provider();

    arg0_dispatch_or_else(|code_linux_sandbox_exe| async move {
        run_main(code_linux_sandbox_exe, CliConfigOverrides::default()).await?;
        Ok(())
    })
}
