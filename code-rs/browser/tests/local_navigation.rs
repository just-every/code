use code_browser::BrowserConfig;
use code_browser::BrowserManager;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

fn spawn_http_server() -> (String, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    listener
        .set_nonblocking(true)
        .expect("set non-blocking listener");
    let addr = listener.local_addr().expect("listener addr");
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        while !stop_thread.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0_u8; 2048];
                    let _ = stream.read(&mut buf);
                    let body = "<html><head><title>Code Browser Local Test</title></head><body><h1>browser-ok</h1></body></html>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(), body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
    });

    (format!("http://127.0.0.1:{}", addr.port()), stop, handle)
}

async fn assert_manager_can_open_local_http_server(headless: bool) {
    let (url, stop, handle) = spawn_http_server();

    let mut config = BrowserConfig::default();
    config.enabled = true;
    config.headless = headless;
    config.idle_timeout_ms = 300_000;

    let manager = BrowserManager::new(config);
    manager.goto(&url).await.expect("navigate to local server");

    let current_url = manager
        .get_current_url()
        .await
        .expect("manager current url after goto");
    assert!(current_url == url || current_url == format!("{url}/"));

    let page = manager.get_or_create_page().await.expect("page after goto");
    let href = page.inject_js("location.href").await.expect("raw href");
    let href_text = href.as_str().unwrap_or_default();
    assert!(href_text == url || href_text == format!("{url}/"));

    let body = page
        .execute_javascript("document.body && document.body.innerText")
        .await
        .expect("read page body");
    let body_text = body
        .get("value")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    assert!(body_text.contains("browser-ok"), "unexpected page body: {body_text}");

    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();
    let _ = manager.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn internal_browser_can_open_local_http_server() {
    assert_manager_can_open_local_http_server(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn headed_internal_browser_can_open_local_http_server() {
    assert_manager_can_open_local_http_server(false).await;
}
