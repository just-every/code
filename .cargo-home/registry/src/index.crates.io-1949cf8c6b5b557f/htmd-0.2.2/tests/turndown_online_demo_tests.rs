use htmd::{
    options::{CodeBlockStyle, HeadingStyle, HrStyle, Options},
    HtmlToMarkdownBuilder,
};

/// Contents are copied from https://mixmark-io.github.io/turndown/
#[test]
fn run_cases() {
    let html = r#"<h1>Turndown Demo</h1>

<p>This demonstrates <a href="https://github.com/mixmark-io/turndown">turndown</a> – an HTML to Markdown converter in JavaScript.</p>

<h2>Usage</h2>

<pre><code class="language-js">var turndownService = new TurndownService()
console.log(
  turndownService.turndown('&lt;h1&gt;Hello world&lt;/h1&gt;')
)</code></pre>

<hr />

<p>It aims to be <a href="http://commonmark.org/">CommonMark</a>
 compliant, and includes options to style the output. These options include:</p>

<ul>
  <li>headingStyle (setext or atx)</li>
  <li>horizontalRule (*, -, or _)</li>
  <li>bullet (*, -, or +)</li>
  <li>codeBlockStyle (indented or fenced)</li>
  <li>fence (` or ~)</li>
  <li>emDelimiter (_ or *)</li>
  <li>strongDelimiter (** or __)</li>
  <li>linkStyle (inlined or referenced)</li>
  <li>linkReferenceStyle (full, collapsed, or shortcut)</li>
</ul>"#;
    let expected = r#"Turndown Demo
=============

This demonstrates [turndown](https://github.com/mixmark-io/turndown) – an HTML to Markdown converter in JavaScript.

Usage
-----

    var turndownService = new TurndownService()
    console.log(
      turndownService.turndown('<h1>Hello world</h1>')
    )

* * *

It aims to be [CommonMark](http://commonmark.org/) compliant, and includes options to style the output. These options include:

*   headingStyle (setext or atx)
*   horizontalRule (\*, -, or \_)
*   bullet (\*, -, or +)
*   codeBlockStyle (indented or fenced)
*   fence (\` or ~)
*   emDelimiter (\_ or \*)
*   strongDelimiter (\*\* or \_\_)
*   linkStyle (inlined or referenced)
*   linkReferenceStyle (full, collapsed, or shortcut)"#;

    let options = Options {
        heading_style: HeadingStyle::Setex,
        hr_style: HrStyle::Asterisks,
        code_block_style: CodeBlockStyle::Indented,
        ..Default::default()
    };
    let converter = HtmlToMarkdownBuilder::new().options(options).build();
    let md = converter.convert(html).unwrap();
    assert_eq!(expected, md);
}
