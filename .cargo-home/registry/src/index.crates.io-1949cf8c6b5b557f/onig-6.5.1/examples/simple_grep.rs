use onig::*;
use std::collections::HashMap;
use std::env;
use std::io;
use std::io::prelude::*;

fn main() {
    let mut regexes = HashMap::new();
    for arg in env::args().skip(1) {
        println!("Compiling '{}'", arg);
        let regex_compilation = Regex::with_options(
            &arg,
            onig::RegexOptions::REGEX_OPTION_SINGLELINE,
            onig::Syntax::emacs(),
        );
        match regex_compilation {
            Ok(regex) => {
                regexes.insert(arg, regex);
            }
            Err(error) => {
                panic!("{:?}", error);
            }
        }
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        if let Ok(line) = line {
            for (name, regex) in regexes.iter() {
                let res = regex.search_with_options(
                    &line,
                    0,
                    line.len(),
                    onig::SearchOptions::SEARCH_OPTION_NONE,
                    None,
                );
                match res {
                    Some(pos) => println!("{} => matched @ {}", name, pos),
                    None => println!("{} => did not match", name),
                }
            }
        }
    }
    println!("done");
}
