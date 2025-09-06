use std::{sync::Arc, thread::JoinHandle};

use htmd::{
    convert,
    options::{BrStyle, LinkStyle, Options},
    Element, HtmlToMarkdown,
};

#[test]
fn links_with_spaces() {
    let html = r#"
        <a href="https://example.com/Some Page.html">Example</a>
        "#;
    assert_eq!(
        "[Example](<https://example.com/Some Page.html>)",
        convert(html).unwrap(),
    )
}

#[test]
fn referenced_links_with_title() {
    let html = r#"
        <a href="https://example.com" title="Some title">Example</a>
        "#;
    let md = HtmlToMarkdown::builder()
        .options(Options {
            link_style: LinkStyle::Referenced,
            ..Default::default()
        })
        .build()
        .convert(html)
        .unwrap();
    assert_eq!(
        "[Example][1]\n\n[1]: https://example.com \"Some title\"",
        &md
    )
}

#[test]
fn images() {
    let html = r#"
        <img src="https://example.com" />
        <img src="https://example.com" alt="Image 1" />
        <img src="https://example.com" alt="Image 2" title="Hello" />
        "#;
    assert_eq!(
        "![](https://example.com)![Image 1](https://example.com)\
            ![Image 2](https://example.com \"Hello\")",
        convert(html).unwrap(),
    )
}

#[test]
fn images_with_spaces_in_url() {
    let html = r#"
        <img src="https://example.com/Some Image.jpg" />
        "#;
    assert_eq!(
        "![](<https://example.com/Some Image.jpg>)",
        convert(html).unwrap(),
    )
}

#[test]
fn headings() {
    let html = r#"
        <h1>Heading 1</h1>
        <h2>Heading 2</h2>
        <h3>Heading 3</h3>
        <h4>Heading 4</h4>
        <h5>Heading 5</h5>
        <h6>Heading 6</h6>
        "#;
    assert_eq!(
        "# Heading 1\n\n## Heading 2\n\n### Heading 3\n\n\
             #### Heading 4\n\n##### Heading 5\n\n###### Heading 6",
        convert(html).unwrap(),
    )
}

#[test]
fn code_blocks() {
    let html = r#"
        <pre><code>println!("Hello");</code></pre>
        "#;
    assert_eq!("```\nprintln!(\"Hello\");\n```", convert(html).unwrap());
}

#[test]
fn code_blocks_with_lang_class() {
    let html = r#"
        <pre><code class="language-rust">println!("Hello");</code></pre>
        "#;
    assert_eq!("```rust\nprintln!(\"Hello\");\n```", convert(html).unwrap());
}

#[test]
fn code_blocks_with_lang_class_on_pre_tag() {
    let html = r#"
        <pre class="language-rust"><code>println!("Hello");</code></pre>
        "#;
    assert_eq!("```rust\nprintln!(\"Hello\");\n```", convert(html).unwrap());
}

#[test]
fn paragraphs() {
    let html = r#"
        <p>The first.</p>
        <p>The <span>second.</span></p>
        "#;
    assert_eq!("The first.\n\nThe second.", convert(html).unwrap());
}

#[test]
fn quotes() {
    let html = r#"
        <blockquote>Once upon a time</blockquote>
        "#;
    assert_eq!("> Once upon a time", convert(html).unwrap());
}

#[test]
fn br() {
    let html = r#"
        Hi<br>there<br><br>!
        "#;
    assert_eq!("Hi  \nthere  \n  \n!", convert(html).unwrap());

    let md = HtmlToMarkdown::builder()
        .options(Options {
            br_style: BrStyle::Backslash,
            ..Default::default()
        })
        .build()
        .convert(html)
        .unwrap();
    assert_eq!("Hi\\\nthere\\\n\\\n!", &md);
}

#[test]
fn hr() {
    let html = r#"Hi <hr/> there"#;
    assert_eq!("Hi\n\n* * *\n\nthere", convert(html).unwrap());
}

#[test]
fn strong_italic() {
    let html = r#"<i>Italic</i> <em>Also italic</em> <strong>Strong</strong>"#;
    assert_eq!("_Italic__Also italic_**Strong**", convert(html).unwrap());
}

#[test]
fn raw_text() {
    let html = r#"Hello world!"#;
    assert_eq!("Hello world!", convert(html).unwrap());
}

#[test]
fn nested_divs() {
    let html = r#"
    <div>
        <div>
            <div>Hi</div>
        </div>
        <div></div>
        <div>there</div>
    </div>
    "#;
    assert_eq!("Hi\n\nthere", convert(html).unwrap());
}

#[test]
fn with_head() {
    let html = r#"
    <html>
        <head>
            <title>Demo</title>
            <script>console.log('Hello');</script>
            <style>body {}</style>
        </head>
        <body>
            Content
        </body>
    </html> 
    "#;
    assert_eq!(
        "Demo\n\nconsole.log('Hello');\n\nbody {}\n\nContent",
        convert(html).unwrap()
    );
}

#[test]
fn with_custom_rules() {
    // Remove element
    let html = r#"<img src="https://example.com"/>"#;
    let md = HtmlToMarkdown::builder()
        .add_handler(vec!["img"], |_: Element| None)
        .build()
        .convert(html)
        .unwrap();
    assert_eq!("", &md);
}

#[test]
fn upper_case_tags() {
    let html = r#"<H1>Hello</H1> <P>World</P>"#;
    assert_eq!("# Hello\n\nWorld", convert(html).unwrap());
}

#[test]
fn html_entities() {
    let html = r#"<p><a href="/my%20&amp;uri" title="my%20&amp;title">my%20&amp;link</a></p>"#;
    assert_eq!(
        r#"[my%20&link](/my%20&uri "my%20&title")"#,
        convert(html).unwrap()
    );

    let html_plain = r#"<p>This &amp; that, then &lt; &gt; now.</p>"#;
    assert_eq!(
        r#"This & that, then < > now."#,
        convert(html_plain).unwrap()
    );

    let html_pre = r#"<pre><code>let x = 5 &amp;&amp; y &lt; 10;</code></pre>"#;
    assert_eq!("```\nlet x = 5 && y < 10;\n```", convert(html_pre).unwrap());
}

#[test]
fn scripting_option() {
    let html = r#"<noscript><p>Hello</p></noscript>"#;
    let md = HtmlToMarkdown::builder()
        .scripting_enabled(true)
        .build()
        .convert(html)
        .unwrap();
    assert_eq!("<p>Hello</p>", md);

    let md = HtmlToMarkdown::builder()
        .scripting_enabled(false)
        .build()
        .convert(html)
        .unwrap();
    assert_eq!("Hello", md);
}

#[test]
fn multithreading() {
    let html = r#"<a href="https://example.com">Example</a>
    <a href="https://example.com">Example</a>
    <a href="https://example.com">Example</a>
    <a href="https://example.com">Example</a>
    <a href="https://example.com">Example</a>
    "#;
    let expected = "[Example][1][Example][2][Example][3][Example][4][Example][5]\n\n\
    [1]: https://example.com\n[2]: https://example.com\n[3]: https://example.com\n\
    [4]: https://example.com\n[5]: https://example.com";
    let converter = HtmlToMarkdown::builder()
        .options(Options {
            // We use a global vec to store all referenced links of the doc in the anchor
            // element handler, this is unsafe for multithreading usage if we do nothing
            link_style: LinkStyle::Referenced,
            ..Default::default()
        })
        .build();
    let converter = Arc::new(converter);
    let mut handlers: Vec<JoinHandle<()>> = vec![];
    for _ in 0..20 {
        let converter_clone = converter.clone();
        let handle = std::thread::spawn(move || {
            let md = converter_clone.convert(html).unwrap();
            assert_eq!(expected, md);
        });
        handlers.push(handle);
    }
    for handle in handlers {
        handle.join().unwrap();
    }
}
