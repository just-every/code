use onig::*;
use std::collections::HashMap;
use std::env;
use std::io;
use std::io::prelude::*;

fn main() {
    let mut regexes = HashMap::new();
    for arg in env::args().skip(1) {
        println!("Compiling '{}'", arg);
        let regex_compilation = Regex::new(&arg);
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
                let res = regex.captures(&line);
                match res {
                    Some(captures) => {
                        for (i, mat) in captures.iter().enumerate() {
                            println!("{} => '{}'", i, mat.unwrap());
                        }
                    }
                    None => println!("{} => did not match", name),
                }
            }
        }
    }
}
