use htmd::{
    options::{
        BrStyle, BulletListMarker, CodeBlockFence, CodeBlockStyle, HeadingStyle, HrStyle,
        LinkReferenceStyle, LinkStyle, Options,
    },
    HtmlToMarkdown,
};
use scraper::{Html, Selector};

struct TestCase {
    pub name: String,
    pub html: String,
    pub md: String,
    pub data_options: Option<String>,
}

#[test]
fn run_cases() {
    let cases = load_test_cases();
    for (index, case) in cases.iter().enumerate() {
        let opt = case.data_options.as_ref();

        let is_atx_heading = opt.is_some_and(|opt| opt == r#"{"headingStyle":"atx"}"#);
        let heading_style = if is_atx_heading {
            HeadingStyle::Atx
        } else {
            HeadingStyle::Setex
        };

        let is_dashes_hr = opt.is_some_and(|opt| opt == r#"{"hr": "- - -"}"#);
        let hr_style = if is_dashes_hr {
            HrStyle::Dashes
        } else {
            HrStyle::Asterisks
        };

        let is_referenced_link =
            opt.is_some_and(|opt| opt.starts_with(r#"{"linkStyle": "referenced""#));
        let link_style = if is_referenced_link {
            LinkStyle::Referenced
        } else {
            LinkStyle::Inlined
        };

        let link_reference_style = match opt.map(|opt| opt.as_str()) {
            Some(r#"{"linkStyle": "referenced", "linkReferenceStyle": "collapsed"}"#) => {
                LinkReferenceStyle::Collapsed
            }
            Some(r#"{"linkStyle": "referenced", "linkReferenceStyle": "shortcut"}"#) => {
                LinkReferenceStyle::Shortcut
            }
            _ => LinkReferenceStyle::Full,
        };

        let is_backslash_br = opt.is_some_and(|opt| opt == r#"{"br": "\\"}"#);
        let br_style = if is_backslash_br {
            BrStyle::Backslash
        } else {
            BrStyle::TwoSpaces
        };

        let is_fenced_code_block =
            opt.is_some_and(|opt| opt.starts_with(r#"{"codeBlockStyle": "fenced""#));
        let code_block_style = if is_fenced_code_block {
            CodeBlockStyle::Fenced
        } else {
            CodeBlockStyle::Indented
        };

        let is_tildes_fence = opt.is_some_and(|opt| opt.contains(r#""fence": "~~~""#));
        let code_block_fence = if is_tildes_fence {
            CodeBlockFence::Tildes
        } else {
            CodeBlockFence::Backticks
        };

        let is_dash_bullet_list_marker =
            opt.is_some_and(|opt| opt == r#"{"bulletListMarker": "-"}"#);
        let bullet_list_marker = if is_dash_bullet_list_marker {
            BulletListMarker::Dash
        } else {
            BulletListMarker::Asterisk
        };

        let ul_bullet_spacing = 3;
        let ol_number_spacing = 2;

        let preformatted_code = opt.is_some_and(|opt| opt == r#"{"preformattedCode": true}"#);

        let converter = HtmlToMarkdown::builder()
            .options(Options {
                heading_style,
                hr_style,
                br_style,
                link_style,
                link_reference_style,
                code_block_style,
                code_block_fence,
                bullet_list_marker,
                ul_bullet_spacing,
                ol_number_spacing,
                preformatted_code,
            })
            .build();

        let md = converter.convert(&case.html).unwrap();

        assert_eq!(
            case.md
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&nbsp;", "\u{a0}")
                // For case: list-like text with non-breaking spaces
                .replace("<!-- hard break -->", ""),
            md,
            "Failed on test case '{}' ({}/{})",
            case.name,
            index + 1,
            cases.len()
        );
    }
}

fn load_test_cases() -> Vec<TestCase> {
    let mut cases = Vec::<TestCase>::new();

    let index_html = std::fs::read_to_string("tests/html/turndown_test_index.html").unwrap();
    let document = Html::parse_document(&index_html);

    let input_selector = Selector::parse("div.input").unwrap();
    let expected_selector = Selector::parse("pre.expected").unwrap();

    for case_element in document.select(&Selector::parse("div.case").unwrap()) {
        let name = case_element.attr("data-name").unwrap();
        let case_input = case_element
            .select(&input_selector)
            .next()
            .unwrap()
            .inner_html();
        let case_expected = case_element
            .select(&expected_selector)
            .next()
            .unwrap()
            .inner_html();
        let data_options = case_element
            .attr("data-options")
            .map(|attr| attr.to_string());
        cases.push(TestCase {
            name: name.to_string(),
            html: case_input,
            md: case_expected,
            data_options,
        })
    }

    cases
}
