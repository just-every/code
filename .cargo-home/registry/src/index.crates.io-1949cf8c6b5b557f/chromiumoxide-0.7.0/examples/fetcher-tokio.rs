use std::path::Path;

use futures::StreamExt;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::fetcher::{BrowserFetcher, BrowserFetcherOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Fetcher browser
    let download_path = Path::new("./download");
    tokio::fs::create_dir_all(&download_path).await?;
    let fetcher = BrowserFetcher::new(
        BrowserFetcherOptions::builder()
            .with_path(&download_path)
            .build()?,
    );
    let info = fetcher.fetch().await?;

    // Verify browser
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .chrome_executable(info.executable_path)
            .build()?,
    )
    .await?;

    let handle = tokio::task::spawn(async move {
        loop {
            match handler.next().await {
                Some(h) => match h {
                    Ok(_) => continue,
                    Err(_) => break,
                },
                None => break,
            }
        }
    });

    let page = browser.new_page("about:blank").await?;

    let sum: usize = page.evaluate("1 + 2").await?.into_value()?;
    assert_eq!(sum, 3);
    println!("it worked!");

    browser.close().await?;
    handle.await?;
    Ok(())
}
