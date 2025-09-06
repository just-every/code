use std::collections::VecDeque;
use std::iter::FromIterator;
use std::time::{Duration, Instant};

use futures::channel::oneshot::Sender as OneshotSender;
use futures::task::Poll;
use serde::Serialize;

use chromiumoxide_cdp::cdp::browser_protocol::page::NavigateParams;
use chromiumoxide_cdp::cdp::browser_protocol::target::SessionId;
use chromiumoxide_types::{Command, CommandResponse, Method, MethodId, Request, Response};

use crate::error::{CdpError, DeadlineExceeded, Result};
use crate::handler::REQUEST_TIMEOUT;

/// Deserialize a response
pub(crate) fn to_command_response<T: Command>(
    resp: Response,
    method: MethodId,
) -> Result<CommandResponse<T::Response>> {
    if let Some(res) = resp.result {
        let result = serde_json::from_value(res)?;
        Ok(CommandResponse {
            id: resp.id,
            result,
            method,
        })
    } else if let Some(err) = resp.error {
        Err(err.into())
    } else {
        Err(CdpError::NoResponse)
    }
}

/// Messages used internally to communicate with the connection, which is
/// executed in the the background task.
#[derive(Debug, Serialize)]
pub struct CommandMessage<T = Result<Response>> {
    pub method: MethodId,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    pub params: serde_json::Value,
    #[serde(skip_serializing)]
    pub sender: OneshotSender<T>,
}

impl<T> CommandMessage<T> {
    pub fn new<C: Command>(cmd: C, sender: OneshotSender<T>) -> serde_json::Result<Self> {
        Ok(Self {
            method: cmd.identifier(),
            session_id: None,
            params: serde_json::to_value(cmd)?,
            sender,
        })
    }

    /// Whether this command is a navigation
    pub fn is_navigation(&self) -> bool {
        self.method.as_ref() == NavigateParams::IDENTIFIER
    }

    pub fn with_session<C: Command>(
        cmd: C,
        sender: OneshotSender<T>,
        session_id: Option<SessionId>,
    ) -> serde_json::Result<Self> {
        Ok(Self {
            method: cmd.identifier(),
            session_id,
            params: serde_json::to_value(cmd)?,
            sender,
        })
    }

    pub fn split(self) -> (Request, OneshotSender<T>) {
        (
            Request {
                method: self.method,
                session_id: self.session_id.map(Into::into),
                params: self.params,
            },
            self.sender,
        )
    }
}

impl Method for CommandMessage {
    fn identifier(&self) -> MethodId {
        self.method.clone()
    }
}

#[derive(Debug)]
pub struct CommandChain {
    /// The commands to process: (method identifier, params)
    cmds: VecDeque<(MethodId, serde_json::Value)>,
    /// The last issued command we currently waiting for its completion
    waiting: Option<(MethodId, Instant)>,
    /// The window a response after issuing a request must arrive
    timeout: Duration,
}

pub type NextCommand = Poll<Option<Result<(MethodId, serde_json::Value), DeadlineExceeded>>>;

impl CommandChain {
    /// Creates a new `CommandChain` from an `Iterator`.
    ///
    /// The order of the commands corresponds to the iterator's
    pub fn new<I>(cmds: I, timeout: Duration) -> Self
    where
        I: IntoIterator<Item = (MethodId, serde_json::Value)>,
    {
        Self {
            cmds: VecDeque::from_iter(cmds),
            waiting: None,
            timeout,
        }
    }

    /// queue in another request
    pub fn push_back(&mut self, method: MethodId, params: serde_json::Value) {
        self.cmds.push_back((method, params))
    }

    /// Removes the waiting state if the identifier matches that of the last
    /// issued command
    pub fn received_response(&mut self, identifier: &str) -> bool {
        if self.waiting.as_ref().map(|(c, _)| c.as_ref()) == Some(identifier) {
            self.waiting.take();
            true
        } else {
            false
        }
    }

    /// Return the next command to process or `None` if done.
    /// If the response timeout an error is returned instead
    pub fn poll(&mut self, now: Instant) -> NextCommand {
        if let Some((cmd, deadline)) = self.waiting.as_ref() {
            if now > *deadline {
                tracing::error!(
                    "Command {:?} exceeded deadline by {:?}",
                    cmd,
                    now - *deadline
                );
                Poll::Ready(Some(Err(DeadlineExceeded::new(now, *deadline))))
            } else {
                Poll::Pending
            }
        } else if let Some((method, val)) = self.cmds.pop_front() {
            self.waiting = Some((method.clone(), now + self.timeout));
            Poll::Ready(Some(Ok((method, val))))
        } else {
            Poll::Ready(None)
        }
    }
}

impl Default for CommandChain {
    fn default() -> Self {
        Self {
            cmds: Default::default(),
            waiting: None,
            timeout: Duration::from_millis(REQUEST_TIMEOUT),
        }
    }
}
