use onig::*;

fn exec(syntax: &Syntax, pattern: &str, to_search: &str) {
    let reg = Regex::with_options(pattern, RegexOptions::REGEX_OPTION_NONE, syntax).unwrap();

    match reg.captures(to_search) {
        Some(caps) => {
            println!("match at {}", caps.offset());
            for (i, cap) in caps.iter_pos().enumerate() {
                match cap {
                    Some(pos) => println!("{}: {:?}", i, pos),
                    None => println!("{}: did not capture", i),
                }
            }
        }
        None => println!("search fail"),
    }
}

fn main() {
    exec(
        Syntax::perl(),
        r"\p{XDigit}\P{XDigit}\p{^XDigit}\P{^XDigit}\p{XDigit}",
        "bgh3a",
    );

    exec(Syntax::java(), r"\p{XDigit}\P{XDigit}[a-c&&b-g]", "bgc");

    exec(
        Syntax::asis(),
        r"abc def* e+ g?ddd[a-rvvv] (vv){3,7}hv\dvv(?:aczui ss)\W\w$",
        r"abc def* e+ g?ddd[a-rvvv] (vv){3,7}hv\dvv(?:aczui ss)\W\w$",
    );
}
