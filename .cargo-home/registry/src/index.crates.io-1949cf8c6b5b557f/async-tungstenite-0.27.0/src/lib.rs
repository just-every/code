//! Async WebSockets.
//!
//! This crate is based on [tungstenite](https://crates.io/crates/tungstenite)
//! Rust WebSocket library and provides async bindings and wrappers for it, so you
//! can use it with non-blocking/asynchronous `TcpStream`s from and couple it
//! together with other crates from the async stack. In addition, optional
//! integration with various other crates can be enabled via feature flags
//!
//!  * `async-tls`: Enables the `async_tls` module, which provides integration
//!    with the [async-tls](https://crates.io/crates/async-tls) TLS stack and can
//!    be used independent of any async runtime.
//!  * `async-std-runtime`: Enables the `async_std` module, which provides
//!    integration with the [async-std](https://async.rs) runtime.
//!  * `async-native-tls`: Enables the additional functions in the `async_std`
//!    module to implement TLS via
//!    [async-native-tls](https://crates.io/crates/async-native-tls).
//!  * `tokio-runtime`: Enables the `tokio` module, which provides integration
//!    with the [tokio](https://tokio.rs) runtime.
//!  * `tokio-native-tls`: Enables the additional functions in the `tokio` module to
//!    implement TLS via [tokio-native-tls](https://crates.io/crates/tokio-native-tls).
//!  * `tokio-rustls-native-certs`: Enables the additional functions in the `tokio`
//!    module to implement TLS via [tokio-rustls](https://crates.io/crates/tokio-rustls)
//!    and uses native system certificates found with
//!    [rustls-native-certs](https://github.com/rustls/rustls-native-certs).
//!  * `tokio-rustls-webpki-roots`: Enables the additional functions in the `tokio`
//!    module to implement TLS via [tokio-rustls](https://crates.io/crates/tokio-rustls)
//!    and uses the certificates [webpki-roots](https://github.com/rustls/webpki-roots)
//!    provides.
//!  * `tokio-openssl`: Enables the additional functions in the `tokio` module to
//!    implement TLS via [tokio-openssl](https://crates.io/crates/tokio-openssl).
//!  * `gio-runtime`: Enables the `gio` module, which provides integration with
//!    the [gio](https://www.gtk-rs.org) runtime.
//!
//! Each WebSocket stream implements the required `Stream` and `Sink` traits,
//! making the socket a stream of WebSocket messages coming in and going out.

#![deny(
    missing_docs,
    unused_must_use,
    unused_mut,
    unused_imports,
    unused_import_braces
)]

pub use tungstenite;

mod compat;
mod handshake;

#[cfg(any(
    feature = "async-tls",
    feature = "async-native-tls",
    feature = "tokio-native-tls",
    feature = "tokio-rustls-manual-roots",
    feature = "tokio-rustls-native-certs",
    feature = "tokio-rustls-webpki-roots",
    feature = "tokio-openssl",
))]
pub mod stream;

use std::io::{Read, Write};

use compat::{cvt, AllowStd, ContextWaker};
use futures_io::{AsyncRead, AsyncWrite};
use futures_util::{
    sink::{Sink, SinkExt},
    stream::{FusedStream, Stream},
};
use log::*;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(feature = "handshake")]
use tungstenite::{
    client::IntoClientRequest,
    handshake::{
        client::{ClientHandshake, Response},
        server::{Callback, NoCallback},
        HandshakeError,
    },
};
use tungstenite::{
    error::Error as WsError,
    protocol::{Message, Role, WebSocket, WebSocketConfig},
};

#[cfg(feature = "async-std-runtime")]
pub mod async_std;
#[cfg(feature = "async-tls")]
pub mod async_tls;
#[cfg(feature = "gio-runtime")]
pub mod gio;
#[cfg(feature = "tokio-runtime")]
pub mod tokio;

use tungstenite::protocol::CloseFrame;

/// Creates a WebSocket handshake from a request and a stream.
/// For convenience, the user may call this with a url string, a URL,
/// or a `Request`. Calling with `Request` allows the user to add
/// a WebSocket protocol or other custom headers.
///
/// Internally, this custom creates a handshake representation and returns
/// a future representing the resolution of the WebSocket handshake. The
/// returned future will resolve to either `WebSocketStream<S>` or `Error`
/// depending on whether the handshake is successful.
///
/// This is typically used for clients who have already established, for
/// example, a TCP connection to the remote server.
#[cfg(feature = "handshake")]
pub async fn client_async<'a, R, S>(
    request: R,
    stream: S,
) -> Result<(WebSocketStream<S>, Response), WsError>
where
    R: IntoClientRequest + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    client_async_with_config(request, stream, None).await
}

/// The same as `client_async()` but the one can specify a websocket configuration.
/// Please refer to `client_async()` for more details.
#[cfg(feature = "handshake")]
pub async fn client_async_with_config<'a, R, S>(
    request: R,
    stream: S,
    config: Option<WebSocketConfig>,
) -> Result<(WebSocketStream<S>, Response), WsError>
where
    R: IntoClientRequest + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let f = handshake::client_handshake(stream, move |allow_std| {
        let request = request.into_client_request()?;
        let cli_handshake = ClientHandshake::start(allow_std, request, config)?;
        cli_handshake.handshake()
    });
    f.await.map_err(|e| match e {
        HandshakeError::Failure(e) => e,
        e => WsError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )),
    })
}

/// Accepts a new WebSocket connection with the provided stream.
///
/// This function will internally call `server::accept` to create a
/// handshake representation and returns a future representing the
/// resolution of the WebSocket handshake. The returned future will resolve
/// to either `WebSocketStream<S>` or `Error` depending if it's successful
/// or not.
///
/// This is typically used after a socket has been accepted from a
/// `TcpListener`. That socket is then passed to this function to perform
/// the server half of the accepting a client's websocket connection.
#[cfg(feature = "handshake")]
pub async fn accept_async<S>(stream: S) -> Result<WebSocketStream<S>, WsError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    accept_hdr_async(stream, NoCallback).await
}

/// The same as `accept_async()` but the one can specify a websocket configuration.
/// Please refer to `accept_async()` for more details.
#[cfg(feature = "handshake")]
pub async fn accept_async_with_config<S>(
    stream: S,
    config: Option<WebSocketConfig>,
) -> Result<WebSocketStream<S>, WsError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    accept_hdr_async_with_config(stream, NoCallback, config).await
}

/// Accepts a new WebSocket connection with the provided stream.
///
/// This function does the same as `accept_async()` but accepts an extra callback
/// for header processing. The callback receives headers of the incoming
/// requests and is able to add extra headers to the reply.
#[cfg(feature = "handshake")]
pub async fn accept_hdr_async<S, C>(stream: S, callback: C) -> Result<WebSocketStream<S>, WsError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    C: Callback + Unpin,
{
    accept_hdr_async_with_config(stream, callback, None).await
}

/// The same as `accept_hdr_async()` but the one can specify a websocket configuration.
/// Please refer to `accept_hdr_async()` for more details.
#[cfg(feature = "handshake")]
pub async fn accept_hdr_async_with_config<S, C>(
    stream: S,
    callback: C,
    config: Option<WebSocketConfig>,
) -> Result<WebSocketStream<S>, WsError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    C: Callback + Unpin,
{
    let f = handshake::server_handshake(stream, move |allow_std| {
        tungstenite::accept_hdr_with_config(allow_std, callback, config)
    });
    f.await.map_err(|e| match e {
        HandshakeError::Failure(e) => e,
        e => WsError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )),
    })
}

/// A wrapper around an underlying raw stream which implements the WebSocket
/// protocol.
///
/// A `WebSocketStream<S>` represents a handshake that has been completed
/// successfully and both the server and the client are ready for receiving
/// and sending data. Message from a `WebSocketStream<S>` are accessible
/// through the respective `Stream` and `Sink`. Check more information about
/// them in `futures-rs` crate documentation or have a look on the examples
/// and unit tests for this crate.
#[derive(Debug)]
pub struct WebSocketStream<S> {
    inner: WebSocket<AllowStd<S>>,
    closing: bool,
    ended: bool,
    /// Tungstenite is probably ready to receive more data.
    ///
    /// `false` once start_send hits `WouldBlock` errors.
    /// `true` initially and after `flush`ing.
    ready: bool,
}

impl<S> WebSocketStream<S> {
    /// Convert a raw socket into a WebSocketStream without performing a
    /// handshake.
    pub async fn from_raw_socket(stream: S, role: Role, config: Option<WebSocketConfig>) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        handshake::without_handshake(stream, move |allow_std| {
            WebSocket::from_raw_socket(allow_std, role, config)
        })
        .await
    }

    /// Convert a raw socket into a WebSocketStream without performing a
    /// handshake.
    pub async fn from_partially_read(
        stream: S,
        part: Vec<u8>,
        role: Role,
        config: Option<WebSocketConfig>,
    ) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        handshake::without_handshake(stream, move |allow_std| {
            WebSocket::from_partially_read(allow_std, part, role, config)
        })
        .await
    }

    pub(crate) fn new(ws: WebSocket<AllowStd<S>>) -> Self {
        Self {
            inner: ws,
            closing: false,
            ended: false,
            ready: true,
        }
    }

    fn with_context<F, R>(&mut self, ctx: Option<(ContextWaker, &mut Context<'_>)>, f: F) -> R
    where
        S: Unpin,
        F: FnOnce(&mut WebSocket<AllowStd<S>>) -> R,
        AllowStd<S>: Read + Write,
    {
        #[cfg(feature = "verbose-logging")]
        trace!("{}:{} WebSocketStream.with_context", file!(), line!());
        if let Some((kind, ctx)) = ctx {
            self.inner.get_mut().set_waker(kind, ctx.waker());
        }
        f(&mut self.inner)
    }

    /// Returns a shared reference to the inner stream.
    pub fn get_ref(&self) -> &S
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        self.inner.get_ref().get_ref()
    }

    /// Returns a mutable reference to the inner stream.
    pub fn get_mut(&mut self) -> &mut S
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        self.inner.get_mut().get_mut()
    }

    /// Returns a reference to the configuration of the tungstenite stream.
    pub fn get_config(&self) -> &WebSocketConfig {
        self.inner.get_config()
    }

    /// Close the underlying web socket
    pub async fn close(&mut self, msg: Option<CloseFrame<'_>>) -> Result<(), WsError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let msg = msg.map(|msg| msg.into_owned());
        self.send(Message::Close(msg)).await
    }
}

impl<T> Stream for WebSocketStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    type Item = Result<Message, WsError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        #[cfg(feature = "verbose-logging")]
        trace!("{}:{} Stream.poll_next", file!(), line!());

        // The connection has been closed or a critical error has occurred.
        // We have already returned the error to the user, the `Stream` is unusable,
        // so we assume that the stream has been "fused".
        if self.ended {
            return Poll::Ready(None);
        }

        match futures_util::ready!(self.with_context(Some((ContextWaker::Read, cx)), |s| {
            #[cfg(feature = "verbose-logging")]
            trace!(
                "{}:{} Stream.with_context poll_next -> read()",
                file!(),
                line!()
            );
            cvt(s.read())
        })) {
            Ok(v) => Poll::Ready(Some(Ok(v))),
            Err(e) => {
                self.ended = true;
                if matches!(e, WsError::AlreadyClosed | WsError::ConnectionClosed) {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(e)))
                }
            }
        }
    }
}

impl<T> FusedStream for WebSocketStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn is_terminated(&self) -> bool {
        self.ended
    }
}

impl<T> Sink<Message> for WebSocketStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    type Error = WsError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.ready {
            Poll::Ready(Ok(()))
        } else {
            // Currently blocked so try to flush the blockage away
            (*self)
                .with_context(Some((ContextWaker::Write, cx)), |s| cvt(s.flush()))
                .map(|r| {
                    self.ready = true;
                    r
                })
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        match (*self).with_context(None, |s| s.write(item)) {
            Ok(()) => {
                self.ready = true;
                Ok(())
            }
            Err(WsError::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock => {
                // the message was accepted and queued so not an error
                // but `poll_ready` will now start trying to flush the block
                self.ready = false;
                Ok(())
            }
            Err(e) => {
                self.ready = true;
                debug!("websocket start_send error: {}", e);
                Err(e)
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        (*self)
            .with_context(Some((ContextWaker::Write, cx)), |s| cvt(s.flush()))
            .map(|r| {
                self.ready = true;
                match r {
                    // WebSocket connection has just been closed. Flushing completed, not an error.
                    Err(WsError::ConnectionClosed) => Ok(()),
                    other => other,
                }
            })
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.ready = true;
        let res = if self.closing {
            // After queueing it, we call `flush` to drive the close handshake to completion.
            (*self).with_context(Some((ContextWaker::Write, cx)), |s| s.flush())
        } else {
            (*self).with_context(Some((ContextWaker::Write, cx)), |s| s.close(None))
        };

        match res {
            Ok(()) => Poll::Ready(Ok(())),
            Err(WsError::ConnectionClosed) => Poll::Ready(Ok(())),
            Err(WsError::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock => {
                trace!("WouldBlock");
                self.closing = true;
                Poll::Pending
            }
            Err(err) => {
                debug!("websocket close error: {}", err);
                Poll::Ready(Err(err))
            }
        }
    }
}

#[cfg(any(
    feature = "async-tls",
    feature = "async-std-runtime",
    feature = "tokio-runtime",
    feature = "gio-runtime"
))]
/// Get a domain from an URL.
#[inline]
pub(crate) fn domain(
    request: &tungstenite::handshake::client::Request,
) -> Result<String, tungstenite::Error> {
    request
        .uri()
        .host()
        .map(|host| {
            // If host is an IPv6 address, it might be surrounded by brackets. These brackets are
            // *not* part of a valid IP, so they must be stripped out.
            //
            // The URI from the request is guaranteed to be valid, so we don't need a separate
            // check for the closing bracket.
            let host = if host.starts_with('[') {
                &host[1..host.len() - 1]
            } else {
                host
            };

            host.to_owned()
        })
        .ok_or(tungstenite::Error::Url(
            tungstenite::error::UrlError::NoHostName,
        ))
}

#[cfg(any(
    feature = "async-std-runtime",
    feature = "tokio-runtime",
    feature = "gio-runtime"
))]
/// Get the port from an URL.
#[inline]
pub(crate) fn port(
    request: &tungstenite::handshake::client::Request,
) -> Result<u16, tungstenite::Error> {
    request
        .uri()
        .port_u16()
        .or_else(|| match request.uri().scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or(tungstenite::Error::Url(
            tungstenite::error::UrlError::UnsupportedUrlScheme,
        ))
}

#[cfg(test)]
mod tests {
    #[cfg(any(
        feature = "async-tls",
        feature = "async-std-runtime",
        feature = "tokio-runtime",
        feature = "gio-runtime"
    ))]
    #[test]
    fn domain_strips_ipv6_brackets() {
        use tungstenite::client::IntoClientRequest;

        let request = "ws://[::1]:80".into_client_request().unwrap();
        assert_eq!(crate::domain(&request).unwrap(), "::1");
    }

    #[cfg(feature = "handshake")]
    #[test]
    fn requests_cannot_contain_invalid_uris() {
        use tungstenite::client::IntoClientRequest;

        assert!("ws://[".into_client_request().is_err());
        assert!("ws://[blabla/bla".into_client_request().is_err());
        assert!("ws://[::1/bla".into_client_request().is_err());
    }
}
