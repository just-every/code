use std::fmt;
use std::io;
use std::process::ExitStatus;
use std::time::Instant;

use async_tungstenite::tungstenite;
use async_tungstenite::tungstenite::Message;
use base64::DecodeError;
use futures::channel::mpsc::SendError;
use futures::channel::oneshot::Canceled;
use thiserror::Error;

use chromiumoxide_cdp::cdp::browser_protocol::page::FrameId;

use crate::handler::frame::NavigationError;
use chromiumoxide_cdp::cdp::js_protocol::runtime::ExceptionDetails;

pub type Result<T, E = CdpError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum CdpError {
    #[error("{0}")]
    Ws(#[from] tungstenite::Error),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Chrome(#[from] chromiumoxide_types::Error),
    #[error("Received no response from the chromium instance.")]
    NoResponse,
    #[error("Received unexpected ws message: {0:?}")]
    UnexpectedWsMessage(Message),
    #[error("{0}")]
    ChannelSendError(#[from] ChannelError),
    #[error("Browser process exited with status {0:?} before websocket URL could be resolved, stderr: {1:?}")]
    LaunchExit(ExitStatus, BrowserStderr),
    #[error("Timeout while resolving websocket URL from browser process, stderr: {0:?}")]
    LaunchTimeout(BrowserStderr),
    #[error(
        "Input/Output error while resolving websocket URL from browser process, stderr: {1:?}: {0}"
    )]
    LaunchIo(#[source] io::Error, BrowserStderr),
    #[error("Request timed out.")]
    Timeout,
    #[error("FrameId {0:?} not found.")]
    FrameNotFound(FrameId),
    /// Error message related to a cdp response that is not a
    /// `chromiumoxide_types::Error`
    #[error("{0}")]
    ChromeMessage(String),
    #[error("{0}")]
    DecodeError(#[from] DecodeError),
    #[error("{0}")]
    ScrollingFailed(String),
    #[error("Requested value not found.")]
    NotFound,
    /// Detailed information about exception (or error) that was thrown during
    /// script compilation or execution
    #[error("{0:?}")]
    JavascriptException(Box<ExceptionDetails>),
    #[error("{0}")]
    Url(#[from] url::ParseError),
}
impl CdpError {
    pub fn msg(msg: impl Into<String>) -> Self {
        CdpError::ChromeMessage(msg.into())
    }
}

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("{0}")]
    Send(#[from] SendError),
    #[error("{0}")]
    Canceled(#[from] Canceled),
}

impl From<Canceled> for CdpError {
    fn from(err: Canceled) -> Self {
        ChannelError::from(err).into()
    }
}

impl From<SendError> for CdpError {
    fn from(err: SendError) -> Self {
        ChannelError::from(err).into()
    }
}

impl From<NavigationError> for CdpError {
    fn from(err: NavigationError) -> Self {
        match err {
            NavigationError::Timeout { .. } => CdpError::Timeout,
            NavigationError::FrameNotFound { frame, .. } => CdpError::FrameNotFound(frame),
        }
    }
}

/// An Error where `now > deadline`
#[derive(Debug, Clone)]
pub struct DeadlineExceeded {
    /// The deadline that was set.
    pub deadline: Instant,
    /// The current time
    pub now: Instant,
}

impl DeadlineExceeded {
    /// Creates a new instance
    ///
    /// panics if `now > deadline`
    pub fn new(now: Instant, deadline: Instant) -> Self {
        // assert!(now > deadline);
        Self { deadline, now }
    }
}

/// `stderr` output of the browser child process
///
/// This implements a custom `Debug` formatter similar to [`std::process::Output`]. If the output
/// is valid UTF-8, format as a string; otherwise format the byte sequence.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BrowserStderr(Vec<u8>);

impl BrowserStderr {
    pub fn new(stderr: Vec<u8>) -> Self {
        Self(stderr)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for BrowserStderr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stderr_utf8 = std::str::from_utf8(&self.0);
        let stderr_debug: &dyn fmt::Debug = match stderr_utf8 {
            Ok(ref str) => str,
            Err(_) => &self.0,
        };

        fmt.debug_tuple("BrowserStderr")
            .field(stderr_debug)
            .finish()
    }
}
