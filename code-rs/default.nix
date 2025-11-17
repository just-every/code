{ pkgs, monorep-deps ? [], ... }:
let
  env = {
    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig:$PKG_CONFIG_PATH";
  };
in
rec {
  package = pkgs.rustPlatform.buildRustPackage {
    inherit env;
    pname = "code-rs";
    version = "0.1.0";
    cargoLock = {
      lockFile = ./Cargo.lock;
      outputHashes = {
        "mcp-types-0.0.0" = "sha256-BGpEuNXky7neVIQQHqyqRjoa/wemfp6zj+usJlkRN+g=";
        "ratatui-0.29.0" = "sha256-HBvT5c8GsiCxMffNjJGLmHnvG77A6cqEL+1ARurBXho=";
      };
    };

    doCheck = false;
    # Cargo.toml depends on ../codex-rs, so point the builder at the
    # repository root and then cd back into this package's directory.
    src = ../.;
    sourceRoot = "${baseNameOf ../.}/${baseNameOf ./.}";
    nativeBuildInputs = with pkgs; [
      pkg-config
      openssl
    ];
    meta = with pkgs.lib; {
      description = "OpenAI Codex command‑line interface rust implementation";
      license = licenses.asl20;
      homepage = "https://github.com/openai/codex";
    };
  };
  devShell = pkgs.mkShell {
    inherit env;
    name = "code-rs-dev";
    packages = monorep-deps ++ [
      pkgs.cargo
      package
    ];
    shellHook = ''
      echo "Entering development shell for code-rs"
      alias codex="cd ${package.src}/tui; cargo run; cd -"
      ${pkgs.rustPlatform.cargoSetupHook}
    '';
  };
  app = {
    type = "app";
    program = "${package}/bin/codex";
  };
}
