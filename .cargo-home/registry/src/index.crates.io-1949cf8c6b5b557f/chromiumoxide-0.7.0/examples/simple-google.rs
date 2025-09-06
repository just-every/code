use chromiumoxide::browser::BrowserConfigBuilder;
use chromiumoxide::Browser;
use futures::StreamExt;
use std::time::Duration;
use tokio::task;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let (browser, mut handler) = Browser::launch(
        BrowserConfigBuilder::default()
            .request_timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
    )
    .await
    .unwrap();

    let h = task::spawn(async move {
        while let Some(h) = handler.next().await {
            h.unwrap();
        }
    });

    let page = browser.new_page("https://www.google.com").await.unwrap();

    println!("loaded page {:?}", page);
    h.await.unwrap();
}
