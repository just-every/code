use futures::StreamExt;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide_cdp::cdp::js_protocol::runtime::{
    CallArgument, CallFunctionOnParams, EvaluateParams,
};

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut browser, mut handler) = Browser::launch(BrowserConfig::builder().build()?).await?;

    let handle = async_std::task::spawn(async move {
        while let Some(h) = handler.next().await {
            match h {
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    });

    let page = browser.new_page("about:blank").await?;

    let sum: usize = page.evaluate("1 + 2").await?.into_value()?;
    assert_eq!(sum, 3);
    println!("1 + 2 = {sum}");

    let mult: usize = page
        .evaluate("() => { return 21 * 2; }")
        .await?
        .into_value()?;
    assert_eq!(mult, 42);
    println!("21 * 2 = {mult}");

    let promise_div: usize = page
        .evaluate("() => Promise.resolve(100 / 25)")
        .await?
        .into_value()?;
    assert_eq!(promise_div, 4);
    println!("100 / 25 = {promise_div}");

    let call = CallFunctionOnParams::builder()
        .function_declaration("(a,b) => { return a + b;}")
        .argument(CallArgument::builder().value(serde_json::json!(1)).build())
        .argument(CallArgument::builder().value(serde_json::json!(2)).build())
        .build()
        .unwrap();
    let sum: usize = page.evaluate_function(call).await?.into_value()?;
    assert_eq!(sum, 3);
    println!("1 + 2 = {sum}");

    let sum: usize = page
        .evaluate_expression("((a,b) => {return a + b;})(1,2)")
        .await?
        .into_value()?;
    assert_eq!(sum, 3);
    println!("1 + 2 = {sum}");

    let val: usize = page
        .evaluate_function("async function() {return 42;}")
        .await?
        .into_value()?;
    assert_eq!(val, 42);
    println!("42 = {val}");

    let eval = EvaluateParams::builder().expression("() => {return 42;}");
    // this will fail because the `EvaluationResult` returned by the browser will be
    // of type `Function`
    assert!(page
        .evaluate(eval.clone().build().unwrap())
        .await?
        .into_value::<usize>()
        .is_err());

    let val: usize = page
        .evaluate(eval.eval_as_function_fallback(true).build().unwrap())
        .await?
        .into_value()?;
    assert_eq!(val, 42);

    browser.close().await?;
    handle.await;
    Ok(())
}
