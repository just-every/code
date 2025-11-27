# npm releases

Use the helpers in this directory to generate npm tarballs for a release. For example,
invoke `build_npm_package.py` after `codex-cli/scripts/install_native_deps.py` has
hydrated `vendor/` for the desired packages; point `--vendor-src` at the populated
`vendor/` tree.
