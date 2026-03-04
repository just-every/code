use clap::Parser;
use code_app_server::AppServerTransport;
use code_app_server::run_main_with_transport;
use code_arg0::arg0_dispatch_or_else;
use code_common::CliConfigOverrides;

#[derive(Debug, Parser)]
struct AppServerArgs {
    /// Transport endpoint URL. Supported values: `stdio://` (default),
    /// `ws://IP:PORT`.
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = AppServerTransport::DEFAULT_LISTEN_URL
    )]
    listen: AppServerTransport,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|code_linux_sandbox_exe| async move {
        let args = AppServerArgs::parse();
        run_main_with_transport(code_linux_sandbox_exe, CliConfigOverrides::default(), args.listen)
            .await?;
        Ok(())
    })
}
