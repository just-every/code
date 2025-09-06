use onig::*;

fn main() {
    let mut syntax = Syntax::default().clone();

    syntax.set_operators(SyntaxOperator::SYNTAX_OPERATOR_VARIABLE_META_CHARACTERS);
    syntax.set_behavior(SyntaxBehavior::empty());
    syntax.set_options(RegexOptions::REGEX_OPTION_MULTILINE);

    syntax.set_meta_char(MetaCharType::META_CHAR_ESCAPE, MetaChar::Character('\\'));
    syntax.set_meta_char(MetaCharType::META_CHAR_ANYCHAR, MetaChar::Character('_'));
    syntax.set_meta_char(MetaCharType::META_CHAR_ANYTIME, MetaChar::Ineffective);
    syntax.set_meta_char(
        MetaCharType::META_CHAR_ZERO_OR_ONE_TIME,
        MetaChar::Ineffective,
    );
    syntax.set_meta_char(
        MetaCharType::META_CHAR_ONE_OR_MORE_TIME,
        MetaChar::Ineffective,
    );
    syntax.set_meta_char(
        MetaCharType::META_CHAR_ANYCHAR_ANYTIME,
        MetaChar::Character('%'),
    );

    let reg =
        Regex::with_options("\\_%\\\\__zz", RegexOptions::REGEX_OPTION_NONE, &syntax).unwrap();

    match reg.captures("a_abcabcabc\\ppzz") {
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
