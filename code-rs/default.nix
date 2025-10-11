{ pkgs, monorep-deps ? [ ], ... }:
let
  env = {
    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig:$PKG_CONFIG_PATH";
  };
  codeRsSrc = pkgs.lib.cleanSource ./.;
  codexSrc = pkgs.lib.cleanSource ../codex-rs;
  workspaceSrc = pkgs.runCommand "code-rs-src" { } ''
    mkdir $out
    cp -r ${codeRsSrc} $out/code-rs
    cp -r ${codexSrc} $out/codex-rs
    chmod -R u+w $out
  '';
in
rec {
  package = pkgs.rustPlatform.buildRustPackage {
    inherit env;
    pname = "code-rs";
    version = "0.1.0";
    src = workspaceSrc;
    cargoRoot = "code-rs";
    buildAndTestSubdir = "code-rs";
    cargoLock = {
      lockFile = ./Cargo.lock;
      outputHashes = {
        "ratatui-0.29.0" = "sha256-HBvT5c8GsiCxMffNjJGLmHnvG77A6cqEL+1ARurBXho=";
      };
    };
    doCheck = false;
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
      alias code="cd ${package.src}/code-rs/tui; cargo run; cd -"
      ${pkgs.rustPlatform.cargoSetupHook}
    '';
  };
  app = {
    type = "app";
    program = "${package}/bin/code";
  };
}
