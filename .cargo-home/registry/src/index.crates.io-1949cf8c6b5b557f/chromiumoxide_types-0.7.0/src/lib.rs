use std::borrow::Cow;
use std::fmt;
use std::fmt::Debug;
use std::ops::Deref;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub type MethodId = Cow<'static, str>;

/// A Request sent by the client, identified by the `id`
#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct MethodCall {
    /// Identifier for this method call
    ///
    /// [`MethodCall`] id's must be unique for every session
    pub id: CallId,
    /// The method identifier
    pub method: MethodId,
    /// The CDP session id of any
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The payload of the request
    pub params: serde_json::Value,
}

/// Identifier for a request send to the chromium server
///
/// All requests (`MethodCall`) must contain a unique identifier.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallId(usize);

impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CallId({})", self.0)
    }
}

impl CallId {
    /// Create a new id
    pub fn new(id: usize) -> Self {
        CallId(id)
    }
}

/// Trait that all the request types have to implement.
pub trait Command: serde::ser::Serialize + Method {
    /// The type of the response this request triggers on the chromium server
    type Response: serde::de::DeserializeOwned + fmt::Debug;

    /// deserialize the response from json
    fn response_from_value(response: serde_json::Value) -> serde_json::Result<Self::Response> {
        serde_json::from_value(response)
    }
}

/// A generic, successful,  response of a request where the `result` has been
/// serialized into the `Command::Response` type.
pub struct CommandResponse<T>
where
    T: fmt::Debug,
{
    pub id: CallId,
    pub result: T,
    pub method: MethodId,
}

/// Represents the successfully deserialization of an incoming response.
///
/// A response can either contain the result (`Command::Response`) are an error
/// `Error`.
pub type CommandResult<T> = Result<CommandResponse<T>, Error>;

impl<T: fmt::Debug> Deref for CommandResponse<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

/// A received `Event` from the websocket where the `params` is deserialized as
/// json
#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct CdpJsonEventMessage {
    /// Name of the method
    pub method: MethodId,
    /// The session this event is meant for.
    pub session_id: Option<String>,
    /// Json payload of the event
    pub params: serde_json::Value,
}

impl Method for CdpJsonEventMessage {
    fn identifier(&self) -> MethodId {
        self.method.clone()
    }
}

impl EventMessage for CdpJsonEventMessage {
    fn session_id(&self) -> Option<&str> {
        self.params.get("sessionId").and_then(|x| x.as_str())
    }
}

/// A trait that mark
pub trait EventMessage: Method + DeserializeOwned + Debug {
    /// The identifier of the session this event was meant for.
    fn session_id(&self) -> Option<&str>;
}

/// `Method`s are message types that contain the field `method =
/// Self::identifier()` in their json body.
pub trait Method {
    /// The whole string identifier for this method like: `DOM.removeNode`
    fn identifier(&self) -> MethodId;

    /// The name of the domain this method belongs to: `DOM`
    fn domain_name(&self) -> MethodId {
        self.split().0
    }

    /// The standalone identifier of the method inside the domain: `removeNode`
    fn method_name(&self) -> MethodId {
        self.split().1
    }

    /// Tuple of (`domain_name`, `method_name`) : (`DOM`, `removeNode`)
    fn split(&self) -> (MethodId, MethodId) {
        match self.identifier() {
            Cow::Borrowed(id) => {
                let mut iter = id.split('.');
                (iter.next().unwrap().into(), iter.next().unwrap().into())
            }
            Cow::Owned(id) => {
                let mut iter = id.split('.');
                (
                    Cow::Owned(iter.next().unwrap().into()),
                    Cow::Owned(iter.next().unwrap().into()),
                )
            }
        }
    }
}

/// A trait that identifies a method on type level
pub trait MethodType {
    /// The identifier for this event's `method` field
    fn method_id() -> MethodId
    where
        Self: Sized;
}

/// A Wrapper for json serialized requests
#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Request {
    /// The identifier for the type of this request.
    pub method: MethodId,
    /// The session this request targets
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The serialized `Command` payload
    pub params: serde_json::Value,
}

impl Request {
    pub fn new(method: MethodId, params: serde_json::Value) -> Self {
        Self {
            method,
            params,
            session_id: None,
        }
    }

    pub fn with_session(
        method: MethodId,
        params: serde_json::Value,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            method,
            params,
            session_id: Some(session_id.into()),
        }
    }
}

/// A response to a [`MethodCall`] from the chromium instance
#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Response {
    /// Numeric identifier for the exact request
    pub id: CallId,
    /// The response payload
    pub result: Option<serde_json::Value>,
    /// The Reason why the [`MethodCall`] failed.
    pub error: Option<Error>,
}

/// An incoming message read from the web socket can either be a response to a
/// previously submitted `Request`, identified by an identifier `id`, or an
/// `Event` emitted by the server.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum Message<T = CdpJsonEventMessage> {
    /// A response for a request
    Response(Response),
    /// An emitted event from the server
    Event(T),
}

/// A response can either contain the `Command::Response` type in the `result`
/// field of the payload or an `Error` in the `error` field if the request
/// resulted in an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseError {
    pub id: CallId,
    /// Error code
    pub code: usize,
    /// Error Message
    pub message: String,
}

/// Represents the error type emitted by the chromium server for failed
/// requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    /// Error code
    pub code: i64,
    /// Error Message
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

/// Represents a binary type as defined in the CDP protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binary(String);

impl AsRef<str> for Binary {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<[u8]> for Binary {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl From<Binary> for String {
    fn from(b: Binary) -> String {
        b.0
    }
}

impl From<String> for Binary {
    fn from(expr: String) -> Self {
        Self(expr)
    }
}
