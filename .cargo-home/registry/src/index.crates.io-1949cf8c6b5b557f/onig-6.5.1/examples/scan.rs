use onig::*;

fn scan_callback<'t>(n: i32, caps: Captures<'t>) -> bool {
    println!("scan: {}", n);
    println!("match at {}", caps.offset());

    for (i, cap) in caps.iter_pos().enumerate() {
        match cap {
            Some(pos) => println!("{}: {:?}", i, pos),
            None => println!("{}: did not capture", i),
        }
    }

    true
}

fn exec(pattern: &str, to_match: &str) {
    let reg = Regex::new(pattern).unwrap();
    reg.scan(to_match, scan_callback);
}

fn main() {
    exec("\\Ga+\\s*", "a aa aaa baaa");
    exec("a+\\s*", "a aa aaa baaa");
}
