use std::sync::Arc;

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use chromiumoxide::cdp::browser_protocol::fetch::{
    ContinueRequestParams, EventRequestPaused, FulfillRequestParams,
};
use futures::StreamExt;

use chromiumoxide::browser::{Browser, BrowserConfig};

const CONTENT: &str = "<html><head></head><body><h1>TEST</h1></body></html>";
const TARGET: &str = "https://news.ycombinator.com/";

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Spawn browser
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .enable_request_intercept()
            .disable_cache()
            .build()?,
    )
    .await?;

    let browser_handle = async_std::task::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    // Setup request interception
    let page = Arc::new(browser.new_page("about:blank").await?);

    let mut request_paused = page.event_listener::<EventRequestPaused>().await.unwrap();
    let intercept_page = page.clone();
    let intercept_handle = async_std::task::spawn(async move {
        while let Some(event) = request_paused.next().await {
            if event.request.url == TARGET {
                if let Err(e) = intercept_page
                    .execute(
                        FulfillRequestParams::builder()
                            .request_id(event.request_id.clone())
                            .body(BASE64_STANDARD.encode(CONTENT))
                            .response_code(200)
                            .build()
                            .unwrap(),
                    )
                    .await
                {
                    println!("Failed to fullfill request: {e}");
                }
            } else if let Err(e) = intercept_page
                .execute(ContinueRequestParams::new(event.request_id.clone()))
                .await
            {
                println!("Failed to continue request: {e}");
            }
        }
    });

    // Navigate to target
    page.goto(TARGET).await?;
    page.wait_for_navigation().await?;
    let content = page.content().await?;
    if content == CONTENT {
        println!("Content overriden!")
    }

    // Navigate to other
    page.goto("https://google.com").await?;
    page.wait_for_navigation().await?;
    let content = page.content().await?;
    if content != CONTENT {
        println!("Content not overriden!")
    }

    browser.close().await?;
    browser_handle.await;
    intercept_handle.await;
    Ok(())
}
