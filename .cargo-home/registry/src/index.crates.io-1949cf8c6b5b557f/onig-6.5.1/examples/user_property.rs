use onig::*;
use std::str;

fn main() {
    define_user_property(
        "HandakuonHiragana",
        &[
            (0x3071, 0x3071), // PA
            (0x3074, 0x3074), // PI
            (0x3077, 0x3077), // PU
            (0x307a, 0x307a), // PE
            (0x307d, 0x307d), // PO
        ],
    );

    // "PA PI PU PE PO a"
    let hay = [
        0xe3, 0x81, 0xb1, 0xe3, 0x81, 0xb4, 0xe3, 0x81, 0xb7, 0xe3, 0x81, 0xba, 0xe3, 0x81, 0xbd,
        'a' as u8,
    ];
    let hay = str::from_utf8(&hay).unwrap();
    let reg = Regex::new("\\A(\\p{HandakuonHiragana}{5})\\p{^HandakuonHiragana}\\z").unwrap();

    match reg.captures(hay) {
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
