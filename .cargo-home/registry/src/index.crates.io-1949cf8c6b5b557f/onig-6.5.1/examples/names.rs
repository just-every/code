use onig::*;

fn main() {
    let pattern = "(?<foo>a*)(?<bar>b*)(?<foo>c*)";
    let string = "aaabbbbcc";

    let r = Regex::new(pattern).unwrap();

    println!("has {} group names:", r.capture_names_len());

    r.foreach_name(|name, indices| {
        println!("- {}: {:?}", name, indices);
        true
    });

    let mut region = Region::new();

    if let Some(position) = r.search_with_options(
        string,
        0,
        string.len(),
        SearchOptions::SEARCH_OPTION_NONE,
        Some(&mut region),
    ) {
        println!("match at {} in {:?}", position, string);

        r.foreach_name(|name, groups| {
            for group in groups {
                let pos = region.pos(*group as usize).unwrap();
                println!("- {} ({}): {} - {}", name, group, pos.0, pos.1);
            }

            true
        });
    } else {
        println!("search fail")
    }
}
