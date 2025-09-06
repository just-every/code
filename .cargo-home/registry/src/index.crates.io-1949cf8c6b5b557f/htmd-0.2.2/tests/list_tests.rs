use htmd::{convert, options::Options, HtmlToMarkdown};

#[test]
fn unordered_lists() {
    let html = r#"
        <ul>
            <li>Item 1</li>
            <li>Item 2</li>
            <li>Item 3</li>
        </ul>
        "#;
    assert_eq!("*   Item 1\n*   Item 2\n*   Item 3", convert(html).unwrap())
}

#[test]
fn unordered_lists_custom_bullet_spacing() {
    let html = r#"
        <ul>
            <li>Item 1</li>
            <li>Item 2</li>
            <li>Item 3</li>
        </ul>
        "#;
    let ul_bullet_spacing = 2;
    let md = HtmlToMarkdown::builder()
        .options(Options {
            ul_bullet_spacing,
            ..Default::default()
        })
        .build()
        .convert(html)
        .unwrap();
    assert_eq!("*  Item 1\n*  Item 2\n*  Item 3", md)
}

#[test]
fn ordered_lists() {
    let html = r#"
        <ol>
            <li>Item 1</li>
            <li>Item 2</li>
            <li>Item 3</li>
        </ol>
        "#;
    assert_eq!("1.  Item 1\n2.  Item 2\n3.  Item 3", convert(html).unwrap())
}

#[test]
fn ordered_lists_custom_bullet_spacing() {
    let html = r#"
        <ol>
            <li>Item 1</li>
            <li>Item 2</li>
            <li>Item 3</li>
        </ol>
        "#;
    let ol_number_spacing = 1;
    let md = HtmlToMarkdown::builder()
        .options(Options {
            ol_number_spacing,
            ..Default::default()
        })
        .build()
        .convert(html)
        .unwrap();
    assert_eq!("1. Item 1\n2. Item 2\n3. Item 3", md)
}
