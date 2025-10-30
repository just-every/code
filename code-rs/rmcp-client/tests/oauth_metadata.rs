use std::convert::Infallible;
use std::sync::Arc;

use anyhow::Result;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use rmcp::transport::auth::AuthorizationManager;
use tokio::sync::oneshot;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn discover_metadata_preserves_query_parameters() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let std_listener = listener.into_std()?;
    std_listener.set_nonblocking(true)?;

    let expected_query = "project_ref=test&read_only=true".to_string();
    let authorize_url = Arc::new(format!(
        "http://{addr}/oauth/authorize?{}",
        expected_query
    ));
    let token_url = Arc::new(format!("http://{addr}/oauth/token?{}", expected_query));
    let registration_url = Arc::new(format!(
        "http://{addr}/oauth/register?{}",
        expected_query
    ));

    let closure_authorize_url = Arc::clone(&authorize_url);
    let closure_token_url = Arc::clone(&token_url);
    let closure_registration_url = Arc::clone(&registration_url);

    let make_svc = make_service_fn(move |_| {
        let authorize_url = Arc::clone(&closure_authorize_url);
        let token_url = Arc::clone(&closure_token_url);
        let registration_url = Arc::clone(&closure_registration_url);
        async move {
            let authorize_url = Arc::clone(&authorize_url);
            let token_url = Arc::clone(&token_url);
            let registration_url = Arc::clone(&registration_url);
            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                let authorize_url = Arc::clone(&authorize_url);
                let token_url = Arc::clone(&token_url);
                let registration_url = Arc::clone(&registration_url);
                async move {
                    let path = req.uri().path();
                    let query = req.uri().query().unwrap_or("");

                    let response = if path == "/.well-known/oauth-authorization-server/mcp" {
                        if query.contains("project_ref=test") && query.contains("read_only=true") {
                            let body = serde_json::json!({
                                "authorization_endpoint": authorize_url.as_str(),
                                "token_endpoint": token_url.as_str(),
                                "registration_endpoint": registration_url.as_str(),
                            });
                            Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", "application/json")
                                .body(Body::from(body.to_string()))
                                .unwrap()
                        } else {
                            Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Body::empty())
                                .unwrap()
                        }
                    } else {
                        Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::empty())
                            .unwrap()
                    };

                    Ok::<_, Infallible>(response)
                }
            }))
        }
    });

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server = Server::from_tcp(std_listener)?
        .http1_only(true)
        .serve(make_svc)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });

    let server_handle = tokio::spawn(server);

    let base_url = format!("http://{addr}/mcp?{}", expected_query);
    let manager = AuthorizationManager::new(&base_url).await?;
    let metadata = manager.discover_metadata().await?;

    assert_eq!(metadata.authorization_endpoint, authorize_url.as_str());
    assert_eq!(metadata.token_endpoint, token_url.as_str());
    assert_eq!(metadata.registration_endpoint, registration_url.as_str());

    let _ = shutdown_tx.send(());
    server_handle.await??;

    Ok(())
}
