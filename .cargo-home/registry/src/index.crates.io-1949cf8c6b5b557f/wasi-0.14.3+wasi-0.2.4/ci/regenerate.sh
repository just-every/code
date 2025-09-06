#!/bin/sh

set -ex

generate() {
  file="$1"
  shift
  wit-bindgen rust wit --out-dir src --std-feature "$@" --format

  sed -z -i 's/#\[unsafe(\n    link_section = "\(.*\)"\n)\]/\
#[cfg_attr(feature = "rustc-dep-of-std", unsafe(link_section = "\1-in-libstd"))]\
#[cfg_attr(not(feature = "rustc-dep-of-std"), unsafe(link_section = "\1"))]\
/' $file
}

# Generate the main body of the bindings which includes all imports from the two
# worlds below.
generate src/bindings.rs --type-section-suffix rust-wasi-from-crates-io \
  --generate-all

# Generate bindings for the `wasi:cli/command` world specifically, namely the
# macro `export_command`.
#
# Note that `--with` is used to point at the previously generated bindings.
with="wasi:cli/environment@0.2.4=crate::cli::environment"
with="$with,wasi:cli/exit@0.2.4=crate::cli::exit"
with="$with,wasi:cli/stdin@0.2.4=crate::cli::stdin"
with="$with,wasi:cli/stdout@0.2.4=crate::cli::stdout"
with="$with,wasi:cli/stderr@0.2.4=crate::cli::stderr"
with="$with,wasi:cli/terminal-input@0.2.4=crate::cli::terminal_input"
with="$with,wasi:cli/terminal-output@0.2.4=crate::cli::terminal_output"
with="$with,wasi:cli/terminal-stdin@0.2.4=crate::cli::terminal_stdin"
with="$with,wasi:cli/terminal-stdout@0.2.4=crate::cli::terminal_stdout"
with="$with,wasi:cli/terminal-stderr@0.2.4=crate::cli::terminal_stderr"
with="$with,wasi:clocks/monotonic-clock@0.2.4=crate::clocks::monotonic_clock"
with="$with,wasi:clocks/wall-clock@0.2.4=crate::clocks::wall_clock"
with="$with,wasi:filesystem/types@0.2.4=crate::filesystem::types"
with="$with,wasi:filesystem/preopens@0.2.4=crate::filesystem::preopens"
with="$with,wasi:io/error@0.2.4=crate::io::error"
with="$with,wasi:io/poll@0.2.4=crate::io::poll"
with="$with,wasi:io/streams@0.2.4=crate::io::streams"
with="$with,wasi:random/random@0.2.4=crate::random::random"
with="$with,wasi:random/insecure@0.2.4=crate::random::insecure"
with="$with,wasi:random/insecure-seed@0.2.4=crate::random::insecure_seed"
with="$with,wasi:sockets/network@0.2.4=crate::sockets::network"
with="$with,wasi:sockets/instance-network@0.2.4=crate::sockets::instance_network"
with="$with,wasi:sockets/tcp@0.2.4=crate::sockets::tcp"
with="$with,wasi:sockets/tcp-create-socket@0.2.4=crate::sockets::tcp_create_socket"
with="$with,wasi:sockets/udp@0.2.4=crate::sockets::udp"
with="$with,wasi:sockets/udp-create-socket@0.2.4=crate::sockets::udp_create_socket"
with="$with,wasi:sockets/ip-name-lookup@0.2.4=crate::sockets::ip_name_lookup"
generate src/command.rs \
  --world wasi:cli/command \
  --with "$with" \
  --type-section-suffix rust-wasi-from-crates-io-command-world \
  --default-bindings-module wasi \
  --pub-export-macro \
  --export-macro-name _export_command

# Same as the `command` world, but for the proxy world.
with="wasi:cli/stdin@0.2.4=crate::cli::stdin"
with="$with,wasi:cli/stdout@0.2.4=crate::cli::stdout"
with="$with,wasi:cli/stderr@0.2.4=crate::cli::stderr"
with="$with,wasi:clocks/monotonic-clock@0.2.4=crate::clocks::monotonic_clock"
with="$with,wasi:clocks/wall-clock@0.2.4=crate::clocks::wall_clock"
with="$with,wasi:io/error@0.2.4=crate::io::error"
with="$with,wasi:io/poll@0.2.4=crate::io::poll"
with="$with,wasi:io/streams@0.2.4=crate::io::streams"
with="$with,wasi:random/random@0.2.4=crate::random::random"
with="$with,wasi:http/types@0.2.4=crate::http::types"
with="$with,wasi:http/outgoing-handler@0.2.4=crate::http::outgoing_handler"
generate src/proxy.rs \
  --world wasi:http/proxy \
  --with "$with" \
  --type-section-suffix rust-wasi-from-crates-io-proxy-world \
  --default-bindings-module wasi \
  --pub-export-macro \
  --export-macro-name _export_proxy
