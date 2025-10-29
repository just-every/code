
import pathlib

try:
    import tomllib
except ImportError:
    import tomli as tomllib

def get_rust_toolchain():
    toolchain_path = pathlib.Path('code-rs/rust-toolchain.toml')
    if not toolchain_path.exists():
        toolchain_path = pathlib.Path('../code-rs/rust-toolchain.toml')
    return tomllib.loads(toolchain_path.read_text())['toolchain']['channel']

if __name__ == '__main__':
    print(get_rust_toolchain())
