use onig::*;

fn ex(hay: &str, pattern: &str, syntax: &Syntax) {
    let reg = Regex::with_options(pattern, RegexOptions::REGEX_OPTION_NONE, syntax).unwrap();

    println!("number of captures: {}", reg.captures_len());
    println!(
        "number of capture histories: {}",
        reg.capture_histories_len()
    );

    let mut region = Region::new();

    let r = reg.search_with_options(
        hay,
        0,
        hay.len(),
        SearchOptions::SEARCH_OPTION_NONE,
        Some(&mut region),
    );
    if let Some(pos) = r {
        println!("match at {}", pos);
        for (i, (start, end)) in region.iter().enumerate() {
            println!("{}: ({}-{})", i, start, end);
        }
        region.tree_traverse(|i, (start, end), level| {
            println!("{}{}: ({}-{})", " ".repeat(level as usize), i, start, end);
            true
        });
    } else {
        println!("search fail");
    }
}

fn main() {
    let mut syn = Syntax::default().clone();
    syn.enable_operators(SyntaxOperator::SYNTAX_OPERATOR_ATMARK_CAPTURE_HISTORY);

    ex(
        "((())())",
        "\\g<p>(?@<p>\\(\\g<s>\\)){0}(?@<s>(?:\\g<p>)*|){0}",
        &syn,
    );
    ex("x00x00x00", "(?@x(?@\\d+))+", &syn);
    ex("0123", "(?@.)(?@.)(?@.)(?@.)", &syn);
}
