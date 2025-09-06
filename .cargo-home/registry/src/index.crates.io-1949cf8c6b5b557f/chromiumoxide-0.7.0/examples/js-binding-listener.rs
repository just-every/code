use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide_cdp::cdp::js_protocol::runtime::{AddBindingParams, EventBindingCalled};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let (mut browser, mut handler) =
        Browser::launch(BrowserConfig::builder().with_head().build()?).await?;

    let handle = async_std::task::spawn(async move {
        while let Some(h) = handler.next().await {
            match h {
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    });

    let page = browser.new_page("about:blank").await?;

    let mut listener = page.event_listener::<EventBindingCalled>().await?;

    let value_from_js: Arc<Mutex<String>> = Arc::new(Mutex::new("".to_string()));
    let value_from_js_clone = Arc::clone(&value_from_js);

    tokio::spawn(async move {
        while let Some(event) = listener.next().await {
            if event.name == "testFunc1" {
                let mut locked_value = value_from_js_clone.lock().await;
                *locked_value = event.payload.clone();
            }
        }
    });

    page.execute(AddBindingParams::new("testFunc1")).await?;

    page.evaluate("window.testFunc1('30');").await?;

    let value = value_from_js.lock().await;
    assert_eq!(*value, "30");

    browser.close().await?;
    handle.await;

    Ok(())
}
