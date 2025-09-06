# htmd

[![crates.io version](https://img.shields.io/crates/v/htmd)](https://crates.io/crates/htmd)

An HTML to Markdown converter for Rust, inspired by [turndown.js](https://github.com/mixmark-io/turndown).

### Features

- Rich options, same as turndown.js
- Reliable, it passes [all test cases](https://github.com/mixmark-io/turndown/blob/master/test/index.html) of turndown.js
- HTML table to Markdown table conversion
- Minimum dependencies, it uses only [html5ever](https://github.com/servo/html5ever)
- Fast, it takes ~70ms to convert a 1.37MB Wikipedia page on an i5 7th gen CPU (See [Bench README](benches/README.md))

*Looking for the cli tool? Try [htmd-cli](https://github.com/letmutex/htmd-cli) now!*

# Usages

Add the dependency

```toml
htmd = "0.1"
```

### Basic

```rust
fn main() {
    assert_eq!("# Heading", htmd::convert("<h1>Heading</h1>").unwrap());
}
```

### Skip tags

```rust
use htmd::HtmlToMarkdown;

let converter = HtmlToMarkdown::builder()
    .skip_tags(vec!["script", "style"])
    .build();
assert_eq!("", converter.convert("<script>let x = 0;</script>").unwrap());
```

### Options

```rust
use htmd::{options::Options, HtmlToMarkdown};

let converter = HtmlToMarkdown::builder()
    .options(Options {
        heading_style: htmd::options::HeadingStyle::Setex,
        ..Default::default()
    })
    .build();
assert_eq!("Heading\n=======", converter.convert("<h1>Heading</h1>").unwrap());
```

### Custom tag handlers

```rust
use htmd::HtmlToMarkdown;

let converter = HtmlToMarkdown::builder()
    .add_handler(vec!["svg"], |_: Element| Some("[Svg Image]".to_string()))
    .build();
assert_eq!("[Svg Image]", converter.convert("<svg></svg>").unwrap());
```

### Tables

```rust
use htmd::convert;

let html = r#"
<table>
    <thead>
        <tr>
            <th>Language</th>
            <th>Type</th>
            <th>Year</th>
        </tr>
    </thead>
    <tbody>
        <tr>
            <td>Rust</td>
            <td>Systems</td>
            <td>2010</td>
        </tr>
        <tr>
            <td>Python</td>
            <td>Interpreted</td>
            <td>1991</td>
        </tr>
    </tbody>
</table>
"#;

println!("{}", convert(html).unwrap());
// Output:
// | Language | Type        | Year |
// | -------- | ----------- | ---- |
// | Rust     | Systems     | 2010 |
// | Python   | Interpreted | 1991 |
```

### Multithreading

You can safely share `HtmlToMarkdown` between multiple threads when only using built-in tag handlers.

```rust
let converter = Arc::new(HtmlToMarkdown::new());

for _ in 0..10 {
    let converter_clone = converter.clone();
    let handle = std::thread::spawn(move || {
        let md = converter_clone.convert("<h1>Hello</h1>").unwrap();
    });
}
```

If you have custom tag handlers that are not stateless, you likely need a thread-safe mechanism. See [AnchorElementHandler](./src/element_handler/anchor.rs) for example.

# Bindings

- Python: [htmd](https://github.com/lmmx/htmd) by [@lmmx](https://github.com/lmmx)

# Credits

- [turndown.js](https://github.com/mixmark-io/turndown)
- [html5ever](https://github.com/servo/html5ever)

# License

```
Copyright 2024 letmutex

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
