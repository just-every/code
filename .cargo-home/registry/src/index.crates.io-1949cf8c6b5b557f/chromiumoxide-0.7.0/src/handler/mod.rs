use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::time::{Duration, Instant};

use fnv::FnvHashMap;
use futures::channel::mpsc::Receiver;
use futures::channel::oneshot::Sender as OneshotSender;
use futures::stream::{Fuse, Stream, StreamExt};
use futures::task::{Context, Poll};

use crate::listeners::{EventListenerRequest, EventListeners};
use chromiumoxide_cdp::cdp::browser_protocol::browser::*;
use chromiumoxide_cdp::cdp::browser_protocol::target::*;
use chromiumoxide_cdp::cdp::events::CdpEvent;
use chromiumoxide_cdp::cdp::events::CdpEventMessage;
use chromiumoxide_types::{CallId, Message, Method, Response};
use chromiumoxide_types::{MethodId, Request as CdpRequest};
pub(crate) use page::PageInner;

use crate::cmd::{to_command_response, CommandMessage};
use crate::conn::Connection;
use crate::error::{CdpError, Result};
use crate::handler::browser::BrowserContext;
use crate::handler::frame::FrameNavigationRequest;
use crate::handler::frame::{NavigationError, NavigationId, NavigationOk};
use crate::handler::job::PeriodicJob;
use crate::handler::session::Session;
use crate::handler::target::TargetEvent;
use crate::handler::target::{Target, TargetConfig};
use crate::handler::viewport::Viewport;
use crate::page::Page;

/// Standard timeout in MS
pub const REQUEST_TIMEOUT: u64 = 30_000;

pub mod browser;
pub mod commandfuture;
pub mod domworld;
pub mod emulation;
pub mod frame;
pub mod http;
pub mod httpfuture;
mod job;
pub mod network;
mod page;
mod session;
pub mod target;
pub mod target_message_future;
pub mod viewport;

/// The handler that monitors the state of the chromium browser and drives all
/// the requests and events.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Handler {
    /// Commands that are being processed and awaiting a response from the
    /// chromium instance together with the timestamp when the request
    /// started.
    pending_commands: FnvHashMap<CallId, (PendingRequest, MethodId, Instant)>,
    /// Connection to the browser instance
    from_browser: Fuse<Receiver<HandlerMessage>>,
    default_browser_context: BrowserContext,
    browser_contexts: HashSet<BrowserContext>,
    /// Used to loop over all targets in a consistent manner
    target_ids: Vec<TargetId>,
    /// The created and attached targets
    targets: HashMap<TargetId, Target>,
    /// Currently queued in navigations for targets
    navigations: FnvHashMap<NavigationId, NavigationRequest>,
    /// Keeps track of all the current active sessions
    ///
    /// There can be multiple sessions per target.
    sessions: HashMap<SessionId, Session>,
    /// The websocket connection to the chromium instance
    conn: Connection<CdpEventMessage>,
    /// Evicts timed out requests periodically
    evict_command_timeout: PeriodicJob,
    /// The internal identifier for a specific navigation
    next_navigation_id: usize,
    /// How this handler will configure targets etc,
    config: HandlerConfig,
    /// All registered event subscriptions
    event_listeners: EventListeners,
    /// Keeps track is the browser is closing
    closing: bool,
}

impl Handler {
    /// Create a new `Handler` that drives the connection and listens for
    /// messages on the receiver `rx`.
    pub(crate) fn new(
        mut conn: Connection<CdpEventMessage>,
        rx: Receiver<HandlerMessage>,
        config: HandlerConfig,
    ) -> Self {
        let discover = SetDiscoverTargetsParams::new(true);
        let _ = conn.submit_command(
            discover.identifier(),
            None,
            serde_json::to_value(discover).unwrap(),
        );

        let browser_contexts = config
            .context_ids
            .iter()
            .map(|id| BrowserContext::from(id.clone()))
            .collect();

        Self {
            pending_commands: Default::default(),
            from_browser: rx.fuse(),
            default_browser_context: Default::default(),
            browser_contexts,
            target_ids: Default::default(),
            targets: Default::default(),
            navigations: Default::default(),
            sessions: Default::default(),
            conn,
            evict_command_timeout: PeriodicJob::new(config.request_timeout),
            next_navigation_id: 0,
            config,
            event_listeners: Default::default(),
            closing: false,
        }
    }

    /// Return the target with the matching `target_id`
    pub fn get_target(&self, target_id: &TargetId) -> Option<&Target> {
        self.targets.get(target_id)
    }

    /// Iterator over all currently attached targets
    pub fn targets(&self) -> impl Iterator<Item = &Target> + '_ {
        self.targets.values()
    }

    /// The default Browser context
    pub fn default_browser_context(&self) -> &BrowserContext {
        &self.default_browser_context
    }

    /// Iterator over all currently available browser contexts
    pub fn browser_contexts(&self) -> impl Iterator<Item = &BrowserContext> + '_ {
        self.browser_contexts.iter()
    }

    /// received a response to a navigation request like `Page.navigate`
    fn on_navigation_response(&mut self, id: NavigationId, resp: Response) {
        if let Some(nav) = self.navigations.remove(&id) {
            match nav {
                NavigationRequest::Navigate(mut nav) => {
                    if nav.navigated {
                        let _ = nav.tx.send(Ok(resp));
                    } else {
                        nav.set_response(resp);
                        self.navigations
                            .insert(id, NavigationRequest::Navigate(nav));
                    }
                }
            }
        }
    }

    /// A navigation has finished.
    fn on_navigation_lifecycle_completed(&mut self, res: Result<NavigationOk, NavigationError>) {
        match res {
            Ok(ok) => {
                let id = *ok.navigation_id();
                if let Some(nav) = self.navigations.remove(&id) {
                    match nav {
                        NavigationRequest::Navigate(mut nav) => {
                            if let Some(resp) = nav.response.take() {
                                let _ = nav.tx.send(Ok(resp));
                            } else {
                                nav.set_navigated();
                                self.navigations
                                    .insert(id, NavigationRequest::Navigate(nav));
                            }
                        }
                    }
                }
            }
            Err(err) => {
                if let Some(nav) = self.navigations.remove(err.navigation_id()) {
                    match nav {
                        NavigationRequest::Navigate(nav) => {
                            let _ = nav.tx.send(Err(err.into()));
                        }
                    }
                }
            }
        }
    }

    /// Received a response to a request.
    fn on_response(&mut self, resp: Response) {
        if let Some((req, method, _)) = self.pending_commands.remove(&resp.id) {
            match req {
                PendingRequest::CreateTarget(tx) => {
                    match to_command_response::<CreateTargetParams>(resp, method) {
                        Ok(resp) => {
                            if let Some(target) = self.targets.get_mut(&resp.target_id) {
                                // move the sender to the target that sends its page once
                                // initialized
                                target.set_initiator(tx);
                            } else {
                                // TODO can this even happen?
                                panic!("Created target not present")
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Err(err)).ok();
                        }
                    }
                }
                PendingRequest::GetTargets(tx) => {
                    match to_command_response::<GetTargetsParams>(resp, method) {
                        Ok(resp) => {
                            let targets: Vec<TargetInfo> = resp.result.target_infos;
                            let results = targets.clone();
                            for target_info in targets {
                                let target_id = target_info.target_id.clone();
                                let event: EventTargetCreated = EventTargetCreated { target_info };
                                self.on_target_created(event);
                                let attach = AttachToTargetParams::new(target_id);
                                let _ = self.conn.submit_command(
                                    attach.identifier(),
                                    None,
                                    serde_json::to_value(attach).unwrap(),
                                );
                            }

                            let _ = tx.send(Ok(results)).ok();
                        }
                        Err(err) => {
                            let _ = tx.send(Err(err)).ok();
                        }
                    }
                }
                PendingRequest::Navigate(id) => {
                    self.on_navigation_response(id, resp);
                }
                PendingRequest::ExternalCommand(tx) => {
                    let _ = tx.send(Ok(resp)).ok();
                }
                PendingRequest::InternalCommand(target_id) => {
                    if let Some(target) = self.targets.get_mut(&target_id) {
                        target.on_response(resp, method.as_ref());
                    }
                }
                PendingRequest::CloseBrowser(tx) => {
                    self.closing = true;
                    let _ = tx.send(Ok(CloseReturns {})).ok();
                }
            }
        }
    }

    /// Submit a command initiated via channel
    pub(crate) fn submit_external_command(
        &mut self,
        msg: CommandMessage,
        now: Instant,
    ) -> Result<()> {
        let call_id = self
            .conn
            .submit_command(msg.method.clone(), msg.session_id, msg.params)?;
        self.pending_commands.insert(
            call_id,
            (PendingRequest::ExternalCommand(msg.sender), msg.method, now),
        );
        Ok(())
    }

    pub(crate) fn submit_internal_command(
        &mut self,
        target_id: TargetId,
        req: CdpRequest,
        now: Instant,
    ) -> Result<()> {
        let call_id = self.conn.submit_command(
            req.method.clone(),
            req.session_id.map(Into::into),
            req.params,
        )?;
        self.pending_commands.insert(
            call_id,
            (PendingRequest::InternalCommand(target_id), req.method, now),
        );
        Ok(())
    }

    fn submit_fetch_targets(&mut self, tx: OneshotSender<Result<Vec<TargetInfo>>>, now: Instant) {
        let msg = GetTargetsParams { filter: None };
        let method = msg.identifier();
        let call_id = self
            .conn
            .submit_command(method.clone(), None, serde_json::to_value(msg).unwrap())
            .unwrap();

        self.pending_commands
            .insert(call_id, (PendingRequest::GetTargets(tx), method, now));
    }

    /// Send the Request over to the server and store its identifier to handle
    /// the response once received.
    fn submit_navigation(&mut self, id: NavigationId, req: CdpRequest, now: Instant) {
        let call_id = self
            .conn
            .submit_command(
                req.method.clone(),
                req.session_id.map(Into::into),
                req.params,
            )
            .unwrap();

        self.pending_commands
            .insert(call_id, (PendingRequest::Navigate(id), req.method, now));
    }

    fn submit_close(&mut self, tx: OneshotSender<Result<CloseReturns>>, now: Instant) {
        let close_msg = CloseParams::default();
        let method = close_msg.identifier();

        let call_id = self
            .conn
            .submit_command(
                method.clone(),
                None,
                serde_json::to_value(close_msg).unwrap(),
            )
            .unwrap();

        self.pending_commands
            .insert(call_id, (PendingRequest::CloseBrowser(tx), method, now));
    }

    /// Process a message received by the target's page via channel
    fn on_target_message(&mut self, target: &mut Target, msg: CommandMessage, now: Instant) {
        // if let some
        if msg.is_navigation() {
            let (req, tx) = msg.split();
            let id = self.next_navigation_id();
            target.goto(FrameNavigationRequest::new(id, req));
            self.navigations.insert(
                id,
                NavigationRequest::Navigate(NavigationInProgress::new(tx)),
            );
        } else {
            let _ = self.submit_external_command(msg, now);
        }
    }

    /// An identifier for queued `NavigationRequest`s.
    fn next_navigation_id(&mut self) -> NavigationId {
        let id = NavigationId(self.next_navigation_id);
        self.next_navigation_id = self.next_navigation_id.wrapping_add(1);
        id
    }

    /// Create a new page and send it to the receiver when ready
    ///
    /// First a `CreateTargetParams` is send to the server, this will trigger
    /// `EventTargetCreated` which results in a new `Target` being created.
    /// Once the response to the request is received the initialization process
    /// of the target kicks in. This triggers a queue of initialization requests
    /// of the `Target`, once those are all processed and the `url` fo the
    /// `CreateTargetParams` has finished loading (The `Target`'s `Page` is
    /// ready and idle), the `Target` sends its newly created `Page` as response
    /// to the initiator (`tx`) of the `CreateTargetParams` request.
    fn create_page(&mut self, params: CreateTargetParams, tx: OneshotSender<Result<Page>>) {
        match url::Url::parse(&params.url) {
            Ok(_) => {
                let method = params.identifier();
                match serde_json::to_value(params) {
                    Ok(params) => match self.conn.submit_command(method.clone(), None, params) {
                        Ok(call_id) => {
                            self.pending_commands.insert(
                                call_id,
                                (PendingRequest::CreateTarget(tx), method, Instant::now()),
                            );
                        }
                        Err(err) => {
                            let _ = tx.send(Err(err.into())).ok();
                        }
                    },
                    Err(err) => {
                        let _ = tx.send(Err(err.into())).ok();
                    }
                }
            }
            Err(err) => {
                let _ = tx.send(Err(err.into())).ok();
            }
        }
    }

    /// Process an incoming event read from the websocket
    fn on_event(&mut self, event: CdpEventMessage) {
        if let Some(ref session_id) = event.session_id {
            if let Some(session) = self.sessions.get(session_id.as_str()) {
                if let Some(target) = self.targets.get_mut(session.target_id()) {
                    return target.on_event(event);
                }
            }
        }
        let CdpEventMessage { params, method, .. } = event;
        match params.clone() {
            CdpEvent::TargetTargetCreated(ev) => self.on_target_created(ev),
            CdpEvent::TargetAttachedToTarget(ev) => self.on_attached_to_target(ev),
            CdpEvent::TargetTargetDestroyed(ev) => self.on_target_destroyed(ev),
            CdpEvent::TargetDetachedFromTarget(ev) => self.on_detached_from_target(ev),
            _ => {}
        }
        chromiumoxide_cdp::consume_event!(match params {
            |ev| self.event_listeners.start_send(ev),
            |json| { let _ = self.event_listeners.try_send_custom(&method, json);}
        });
    }

    /// Fired when a new target was created on the chromium instance
    ///
    /// Creates a new `Target` instance and keeps track of it
    fn on_target_created(&mut self, event: EventTargetCreated) {
        let browser_ctx = event
            .target_info
            .browser_context_id
            .clone()
            .map(BrowserContext::from)
            .filter(|id| self.browser_contexts.contains(id))
            .unwrap_or_else(|| self.default_browser_context.clone());
        let target = Target::new(
            event.target_info,
            TargetConfig {
                ignore_https_errors: self.config.ignore_https_errors,
                request_timeout: self.config.request_timeout,
                viewport: self.config.viewport.clone(),
                request_intercept: self.config.request_intercept,
                cache_enabled: self.config.cache_enabled,
            },
            browser_ctx,
        );
        self.target_ids.push(target.target_id().clone());
        self.targets.insert(target.target_id().clone(), target);
    }

    /// A new session is attached to a target
    fn on_attached_to_target(&mut self, event: Box<EventAttachedToTarget>) {
        let session = Session::new(event.session_id.clone(), event.target_info.target_id);
        if let Some(target) = self.targets.get_mut(session.target_id()) {
            target.set_session_id(session.session_id().clone())
        }
        self.sessions.insert(event.session_id, session);
    }

    /// The session was detached from target.
    /// Can be issued multiple times per target if multiple session have been
    /// attached to it.
    fn on_detached_from_target(&mut self, event: EventDetachedFromTarget) {
        // remove the session
        if let Some(session) = self.sessions.remove(&event.session_id) {
            if let Some(target) = self.targets.get_mut(session.target_id()) {
                target.session_id().take();
            }
        }
    }

    /// Fired when the target was destroyed in the browser
    fn on_target_destroyed(&mut self, event: EventTargetDestroyed) {
        if let Some(target) = self.targets.remove(&event.target_id) {
            // TODO shutdown?
            if let Some(session) = target.session_id() {
                self.sessions.remove(session);
            }
        }
    }

    /// House keeping of commands
    ///
    /// Remove all commands where `now` > `timestamp of command starting point +
    /// request timeout` and notify the senders that their request timed out.
    fn evict_timed_out_commands(&mut self, now: Instant) {
        let timed_out = self
            .pending_commands
            .iter()
            .filter(|(_, (_, _, timestamp))| now > (*timestamp + self.config.request_timeout))
            .map(|(k, _)| *k)
            .collect::<Vec<_>>();
        for call in timed_out {
            if let Some((req, _, _)) = self.pending_commands.remove(&call) {
                match req {
                    PendingRequest::CreateTarget(tx) => {
                        let _ = tx.send(Err(CdpError::Timeout));
                    }
                    PendingRequest::GetTargets(tx) => {
                        let _ = tx.send(Err(CdpError::Timeout));
                    }
                    PendingRequest::Navigate(nav) => {
                        if let Some(nav) = self.navigations.remove(&nav) {
                            match nav {
                                NavigationRequest::Navigate(nav) => {
                                    let _ = nav.tx.send(Err(CdpError::Timeout));
                                }
                            }
                        }
                    }
                    PendingRequest::ExternalCommand(tx) => {
                        let _ = tx.send(Err(CdpError::Timeout));
                    }
                    PendingRequest::InternalCommand(_) => {}
                    PendingRequest::CloseBrowser(tx) => {
                        let _ = tx.send(Err(CdpError::Timeout));
                    }
                }
            }
        }
    }

    pub fn event_listeners_mut(&mut self) -> &mut EventListeners {
        &mut self.event_listeners
    }
}

impl Stream for Handler {
    type Item = Result<()>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pin = self.get_mut();

        loop {
            let now = Instant::now();
            // temporary pinning of the browser receiver should be safe as we are pinning
            // through the already pinned self. with the receivers we can also
            // safely ignore exhaustion as those are fused.
            while let Poll::Ready(Some(msg)) = Pin::new(&mut pin.from_browser).poll_next(cx) {
                match msg {
                    HandlerMessage::Command(cmd) => {
                        pin.submit_external_command(cmd, now)?;
                    }
                    HandlerMessage::FetchTargets(tx) => {
                        pin.submit_fetch_targets(tx, now);
                    }
                    HandlerMessage::CloseBrowser(tx) => {
                        pin.submit_close(tx, now);
                    }
                    HandlerMessage::CreatePage(params, tx) => {
                        pin.create_page(params, tx);
                    }
                    HandlerMessage::GetPages(tx) => {
                        let pages: Vec<_> = pin
                            .targets
                            .values_mut()
                            .filter(|p| p.is_page())
                            .filter_map(|target| target.get_or_create_page())
                            .map(|page| Page::from(page.clone()))
                            .collect();
                        let _ = tx.send(pages);
                    }
                    HandlerMessage::InsertContext(ctx) => {
                        pin.browser_contexts.insert(ctx);
                    }
                    HandlerMessage::DisposeContext(ctx) => {
                        pin.browser_contexts.remove(&ctx);
                    }
                    HandlerMessage::GetPage(target_id, tx) => {
                        let page = pin
                            .targets
                            .get_mut(&target_id)
                            .and_then(|target| target.get_or_create_page())
                            .map(|page| Page::from(page.clone()));
                        let _ = tx.send(page);
                    }
                    HandlerMessage::AddEventListener(req) => {
                        pin.event_listeners.add_listener(req);
                    }
                }
            }

            for n in (0..pin.target_ids.len()).rev() {
                let target_id = pin.target_ids.swap_remove(n);
                if let Some((id, mut target)) = pin.targets.remove_entry(&target_id) {
                    while let Some(event) = target.poll(cx, now) {
                        match event {
                            TargetEvent::Request(req) => {
                                let _ = pin.submit_internal_command(
                                    target.target_id().clone(),
                                    req,
                                    now,
                                );
                            }
                            TargetEvent::Command(msg) => {
                                pin.on_target_message(&mut target, msg, now);
                            }
                            TargetEvent::NavigationRequest(id, req) => {
                                pin.submit_navigation(id, req, now);
                            }
                            TargetEvent::NavigationResult(res) => {
                                pin.on_navigation_lifecycle_completed(res)
                            }
                        }
                    }

                    // poll the target's event listeners
                    target.event_listeners_mut().poll(cx);
                    // poll the handler's event listeners
                    pin.event_listeners_mut().poll(cx);

                    pin.targets.insert(id, target);
                    pin.target_ids.push(target_id);
                }
            }

            let mut done = true;

            while let Poll::Ready(Some(ev)) = Pin::new(&mut pin.conn).poll_next(cx) {
                match ev {
                    Ok(Message::Response(resp)) => {
                        pin.on_response(resp);
                        if pin.closing {
                            // handler should stop processing
                            return Poll::Ready(None);
                        }
                    }
                    Ok(Message::Event(ev)) => {
                        pin.on_event(ev);
                    }
                    Err(err) => {
                        tracing::error!("WS Connection error: {:?}", err);
                        return Poll::Ready(Some(Err(err)));
                    }
                }
                done = false;
            }

            if pin.evict_command_timeout.poll_ready(cx) {
                // evict all commands that timed out
                pin.evict_timed_out_commands(now);
            }

            if done {
                // no events/responses were read from the websocket
                return Poll::Pending;
            }
        }
    }
}

/// How to configure the handler
#[derive(Debug, Clone)]
pub struct HandlerConfig {
    /// Whether the `NetworkManager`s should ignore https errors
    pub ignore_https_errors: bool,
    /// Window and device settings
    pub viewport: Option<Viewport>,
    /// Context ids to set from the get go
    pub context_ids: Vec<BrowserContextId>,
    /// default request timeout to use
    pub request_timeout: Duration,
    /// Whether to enable request interception
    pub request_intercept: bool,
    /// Whether to enable cache
    pub cache_enabled: bool,
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self {
            ignore_https_errors: true,
            viewport: Default::default(),
            context_ids: Vec::new(),
            request_timeout: Duration::from_millis(REQUEST_TIMEOUT),
            request_intercept: false,
            cache_enabled: true,
        }
    }
}

/// Wraps the sender half of the channel who requested a navigation
#[derive(Debug)]
pub struct NavigationInProgress<T> {
    /// Marker to indicate whether a navigation lifecycle has completed
    navigated: bool,
    /// The response of the issued navigation request
    response: Option<Response>,
    /// Sender who initiated the navigation request
    tx: OneshotSender<T>,
}

impl<T> NavigationInProgress<T> {
    fn new(tx: OneshotSender<T>) -> Self {
        Self {
            navigated: false,
            response: None,
            tx,
        }
    }

    /// The response to the cdp request has arrived
    fn set_response(&mut self, resp: Response) {
        self.response = Some(resp);
    }

    /// The navigation process has finished, the page finished loading.
    fn set_navigated(&mut self) {
        self.navigated = true;
    }
}

/// Request type for navigation
#[derive(Debug)]
enum NavigationRequest {
    /// Represents a simple `NavigateParams` ("Page.navigate")
    Navigate(NavigationInProgress<Result<Response>>),
    // TODO are there more?
}

/// Different kind of submitted request submitted from the  `Handler` to the
/// `Connection` and being waited on for the response.
#[derive(Debug)]
enum PendingRequest {
    /// A Request to create a new `Target` that results in the creation of a
    /// `Page` that represents a browser page.
    CreateTarget(OneshotSender<Result<Page>>),
    /// A Request to fetch old `Target`s created before connection
    GetTargets(OneshotSender<Result<Vec<TargetInfo>>>),
    /// A Request to navigate a specific `Target`.
    ///
    /// Navigation requests are not automatically completed once the response to
    /// the raw cdp navigation request (like `NavigateParams`) arrives, but only
    /// after the `Target` notifies the `Handler` that the `Page` has finished
    /// loading, which comes after the response.
    Navigate(NavigationId),
    /// A common request received via a channel (`Page`).
    ExternalCommand(OneshotSender<Result<Response>>),
    /// Requests that are initiated directly from a `Target` (all the
    /// initialization commands).
    InternalCommand(TargetId),
    // A Request to close the browser.
    CloseBrowser(OneshotSender<Result<CloseReturns>>),
}

/// Events used internally to communicate with the handler, which are executed
/// in the background
// TODO rename to BrowserMessage
#[derive(Debug)]
pub(crate) enum HandlerMessage {
    CreatePage(CreateTargetParams, OneshotSender<Result<Page>>),
    FetchTargets(OneshotSender<Result<Vec<TargetInfo>>>),
    InsertContext(BrowserContext),
    DisposeContext(BrowserContext),
    GetPages(OneshotSender<Vec<Page>>),
    Command(CommandMessage),
    GetPage(TargetId, OneshotSender<Option<Page>>),
    AddEventListener(EventListenerRequest),
    CloseBrowser(OneshotSender<Result<CloseReturns>>),
}
