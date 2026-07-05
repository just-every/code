use clap::Parser;
use code_responses_api_proxy::Args as ResponsesApiProxyArgs;

#[ctor::ctor]
fn pre_main() {
    code_process_hardening::pre_main_hardening();
}

pub fn main() -> anyhow::Result<()> {
    code_utils_rustls_provider::ensure_rustls_crypto_provider();

    let args = ResponsesApiProxyArgs::parse();
    code_responses_api_proxy::run_main(args)
}
