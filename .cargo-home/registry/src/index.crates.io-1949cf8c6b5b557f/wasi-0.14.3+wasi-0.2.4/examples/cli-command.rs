use std::io::Write as _;

wasi::cli::command::export!(Example);

struct Example;

impl wasi::exports::cli::run::Guest for Example {
    fn run() -> Result<(), ()> {
        let mut stdout = wasi::cli::stdout::get_stdout();
        stdout.write_all(b"Hello, WASI!").unwrap();
        stdout.flush().unwrap();
        Ok(())
    }
}
