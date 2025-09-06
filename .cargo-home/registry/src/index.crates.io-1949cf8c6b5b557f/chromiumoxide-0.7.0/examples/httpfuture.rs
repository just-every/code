use futures::StreamExt;
use futures::TryFutureExt;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::NavigateParams;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let (browser, mut handler) =
        Browser::launch(BrowserConfig::builder().with_head().build()?).await?;

    let handle = tokio::task::spawn(async move {
        loop {
            let _ = handler.next().await.unwrap();
        }
    });

    let page = browser.new_page("https://en.wikipedia.org").await?;

    let _response1 = page
        .http_future(NavigateParams {
            url: "https://en.wikipedia.org".to_string(),
            transition_type: None,
            frame_id: None,
            referrer: None,
            referrer_policy: None,
        })?
        .and_then(|request| async { Ok(request.map(|r| r.response.clone())) })
        .await?;

    let _html = page.wait_for_navigation().await?.content().await?;

    handle.await?;
    Ok(())
}
