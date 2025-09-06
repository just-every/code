use futures::StreamExt;

use chromiumoxide::browser::{Browser, BrowserConfig};

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let (mut browser, mut handler) =
        Browser::launch(BrowserConfig::builder().with_head().build()?).await?;

    let handle = async_std::task::spawn(async move {
        loop {
            let _ = handler.next().await.unwrap();
        }
    });

    // switch to incognito mode and goto the url
    let _incognito_page = browser
        .start_incognito_context()
        .await?
        .new_page("https://en.wikipedia.org")
        .await?;

    handle.await;
    Ok(())
}
