use onig::*;

fn main() {
    let pattern = "a(.*)b|[e-f]+";
    let string = "zzzzaffffffffb";

    let r = Regex::new(pattern).unwrap();

    match r.captures(string) {
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
