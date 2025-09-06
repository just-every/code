use futures::StreamExt;

use chromiumoxide::browser::Browser;
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::cdp::browser_protocol::network::CookieParam;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let (mut browser, mut handler) =
        Browser::launch(BrowserConfig::builder().with_head().build()?).await?;
    let _ = tokio::spawn(async move { while let Some(_) = handler.next().await {} });

    let _ = browser.new_page("https://setcookie.net/").await?;
    let example_cookie = CookieParam::builder()
        .domain(".setcookie.net")
        .name("set_from_chromiumoxide")
        .value("Test Value")
        .path("/")
        .build()?;

    println!("\x1b[32mType 'c' to clear all cookies, 's' to set a cookie, 'q' to quit the browser\x1b[0m");
    loop {
        // Read Cookies
        println!("All Browser cookies: {:?}", browser.get_cookies().await?);

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if input.trim() == "c" {
            // Clear Cookies
            browser.clear_cookies().await?;
        }
        if input.trim() == "s" {
            // Set Cookies
            browser.set_cookies(vec![example_cookie.clone()]).await?;
        }
        if input.trim() == "q" {
            break;
        }
    }

    browser.close().await?;
    browser.wait().await?;

    Ok(())
}
