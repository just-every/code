use std::path::PathBuf;

use htmd::HtmlToMarkdown;

fn main() {
    convert(
        "examples/page-to-markdown/html/Hacker News.html",
        "output_hacker_news.md",
    );

    println!();

    convert(
        "examples/page-to-markdown/html/Elon Musk - Wikipedia.html",
        "output_wikipedia.md",
    );
}

fn convert(html_path: &str, output_filename: &str) {
    let path = PathBuf::from(html_path);

    let html = std::fs::read_to_string(path.clone()).unwrap();

    let now = std::time::Instant::now();

    let md = HtmlToMarkdown::new().convert(&html).unwrap();
    println!(
        "Converted '{}' in {}ms",
        path.file_name().unwrap().to_str().unwrap(),
        now.elapsed().as_millis()
    );

    std::fs::write(output_filename, md).unwrap();
    println!("Saved as '{}'", output_filename);
}
