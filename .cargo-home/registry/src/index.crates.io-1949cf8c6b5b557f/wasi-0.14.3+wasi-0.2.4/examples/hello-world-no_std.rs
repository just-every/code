fn main() {
    let stdout = wasi::cli::stdout::get_stdout();
    stdout.blocking_write_and_flush(b"Hello, world!\n").unwrap();
}
