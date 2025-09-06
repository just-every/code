use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use chromiumoxide_cdp::cdp::browser_protocol::target::DetachFromTargetParams;
use futures::channel::oneshot::Sender;
use futures::stream::Stream;
use futures::task::{Context, Poll};

use chromiumoxide_cdp::cdp::browser_protocol::page::{FrameId, GetFrameTreeParams};
use chromiumoxide_cdp::cdp::browser_protocol::{
    browser::BrowserContextId,
    log as cdplog, performance,
    target::{AttachToTargetParams, SessionId, SetAutoAttachParams, TargetId, TargetInfo},
};
use chromiumoxide_cdp::cdp::events::CdpEvent;
use chromiumoxide_cdp::cdp::CdpEventMessage;
use chromiumoxide_types::{Command, Method, Request, Response};

use crate::auth::Credentials;
use crate::cdp::browser_protocol::target::CloseTargetParams;
use crate::cmd::CommandChain;
use crate::cmd::CommandMessage;
use crate::error::{CdpError, Result};
use crate::handler::browser::BrowserContext;
use crate::handler::domworld::DOMWorldKind;
use crate::handler::emulation::EmulationManager;
use crate::handler::frame::{
    FrameEvent, FrameManager, NavigationError, NavigationId, NavigationOk,
};
use crate::handler::frame::{FrameNavigationRequest, UTILITY_WORLD_NAME};
use crate::handler::network::{NetworkEvent, NetworkManager};
use crate::handler::page::PageHandle;
use crate::handler::viewport::Viewport;
use crate::handler::{PageInner, REQUEST_TIMEOUT};
use crate::listeners::{EventListenerRequest, EventListeners};
use crate::{page::Page, ArcHttpRequest};
use chromiumoxide_cdp::cdp::js_protocol::runtime::{
    ExecutionContextId, RunIfWaitingForDebuggerParams,
};
use std::time::Duration;

macro_rules! advance_state {
    ($s:ident, $cx:ident, $now:ident, $cmds: ident, $next_state:expr ) => {{
        if let Poll::Ready(poll) = $cmds.poll($now) {
            return match poll {
                None => {
                    $s.init_state = $next_state;
                    $s.poll($cx, $now)
                }
                Some(Ok((method, params))) => Some(TargetEvent::Request(Request {
                    method,
                    session_id: $s.session_id.clone().map(Into::into),
                    params,
                })),
                Some(Err(_)) => Some($s.on_initialization_failed()),
            };
        } else {
            return None;
        }
    }};
}

#[derive(Debug)]
pub struct Target {
    /// Info about this target as returned from the chromium instance
    info: TargetInfo,
    /// The type of this target
    r#type: TargetType,
    /// Configs for this target
    config: TargetConfig,
    /// The context this target is running in
    browser_context: BrowserContext,
    /// The frame manager that maintains the state of all frames and handles
    /// navigations of frames
    frame_manager: FrameManager,
    /// Handles all the https
    network_manager: NetworkManager,
    emulation_manager: EmulationManager,
    /// The identifier of the session this target is attached to
    session_id: Option<SessionId>,
    /// The handle of the browser page of this target
    page: Option<PageHandle>,
    /// Drives this target towards initialization
    init_state: TargetInit,
    /// Currently queued events to report to the `Handler`
    queued_events: VecDeque<TargetEvent>,
    /// All registered event subscriptions
    event_listeners: EventListeners,
    /// Senders that need to be notified once the main frame has loaded
    wait_for_frame_navigation: Vec<Sender<ArcHttpRequest>>,
    /// The sender who requested the page.
    initiator: Option<Sender<Result<Page>>>,
}

impl Target {
    /// Create a new target instance with `TargetInfo` after a
    /// `CreateTargetParams` request.
    pub fn new(info: TargetInfo, config: TargetConfig, browser_context: BrowserContext) -> Self {
        let ty = TargetType::new(&info.r#type);
        let request_timeout = config.request_timeout;
        let mut network_manager = NetworkManager::new(config.ignore_https_errors, request_timeout);

        network_manager.set_cache_enabled(config.cache_enabled);
        network_manager.set_request_interception(config.request_intercept);

        Self {
            info,
            r#type: ty,
            config,
            frame_manager: FrameManager::new(request_timeout),
            network_manager,
            emulation_manager: EmulationManager::new(request_timeout),
            session_id: None,
            page: None,
            init_state: TargetInit::AttachToTarget,
            wait_for_frame_navigation: Default::default(),
            queued_events: Default::default(),
            event_listeners: Default::default(),
            initiator: None,
            browser_context,
        }
    }

    pub fn set_session_id(&mut self, id: SessionId) {
        self.session_id = Some(id)
    }

    pub fn session_id(&self) -> Option<&SessionId> {
        self.session_id.as_ref()
    }

    pub fn browser_context(&self) -> &BrowserContext {
        &self.browser_context
    }

    pub fn session_id_mut(&mut self) -> &mut Option<SessionId> {
        &mut self.session_id
    }

    /// The identifier for this target
    pub fn target_id(&self) -> &TargetId {
        &self.info.target_id
    }

    /// The type of this target
    pub fn r#type(&self) -> &TargetType {
        &self.r#type
    }

    /// Whether this target is already initialized
    pub fn is_initialized(&self) -> bool {
        matches!(self.init_state, TargetInit::Initialized)
    }

    /// Navigate a frame
    pub fn goto(&mut self, req: FrameNavigationRequest) {
        self.frame_manager.goto(req)
    }

    fn create_page(&mut self) {
        if self.page.is_none() {
            if let Some(session) = self.session_id.clone() {
                let handle =
                    PageHandle::new(self.target_id().clone(), session, self.opener_id().cloned());
                self.page = Some(handle);
            }
        }
    }

    /// Tries to create the `PageInner` if this target is already initialized
    pub(crate) fn get_or_create_page(&mut self) -> Option<&Arc<PageInner>> {
        self.create_page();
        self.page.as_ref().map(|p| p.inner())
    }

    pub fn is_page(&self) -> bool {
        self.r#type().is_page()
    }

    pub fn browser_context_id(&self) -> Option<&BrowserContextId> {
        self.info.browser_context_id.as_ref()
    }

    pub fn info(&self) -> &TargetInfo {
        &self.info
    }

    /// Get the target that opened this target. Top-level targets return `None`.
    pub fn opener_id(&self) -> Option<&TargetId> {
        self.info.opener_id.as_ref()
    }

    pub fn frame_manager(&self) -> &FrameManager {
        &self.frame_manager
    }

    pub fn frame_manager_mut(&mut self) -> &mut FrameManager {
        &mut self.frame_manager
    }

    pub fn event_listeners_mut(&mut self) -> &mut EventListeners {
        &mut self.event_listeners
    }

    /// Received a response to a command issued by this target
    pub fn on_response(&mut self, resp: Response, method: &str) {
        if let Some(cmds) = self.init_state.commands_mut() {
            cmds.received_response(method);
        }
        #[allow(clippy::single_match)] // allow for now
        match method {
            GetFrameTreeParams::IDENTIFIER => {
                if let Some(resp) = resp
                    .result
                    .and_then(|val| GetFrameTreeParams::response_from_value(val).ok())
                {
                    self.frame_manager.on_frame_tree(resp.frame_tree);
                }
            }
            // requests originated from the network manager all return an empty response, hence they
            // can be ignored here
            _ => {}
        }
    }

    pub fn on_event(&mut self, event: CdpEventMessage) {
        let CdpEventMessage { params, method, .. } = event;
        match &params {
            // `FrameManager` events
            CdpEvent::PageFrameAttached(ev) => self
                .frame_manager
                .on_frame_attached(ev.frame_id.clone(), Some(ev.parent_frame_id.clone())),
            CdpEvent::PageFrameDetached(ev) => self.frame_manager.on_frame_detached(ev),
            CdpEvent::PageFrameNavigated(ev) => self.frame_manager.on_frame_navigated(&ev.frame),
            CdpEvent::PageNavigatedWithinDocument(ev) => {
                self.frame_manager.on_frame_navigated_within_document(ev)
            }
            CdpEvent::RuntimeExecutionContextCreated(ev) => {
                self.frame_manager.on_frame_execution_context_created(ev)
            }
            CdpEvent::RuntimeExecutionContextDestroyed(ev) => {
                self.frame_manager.on_frame_execution_context_destroyed(ev)
            }
            CdpEvent::RuntimeExecutionContextsCleared(_) => {
                self.frame_manager.on_execution_contexts_cleared()
            }
            CdpEvent::RuntimeBindingCalled(ev) => {
                // TODO check if binding registered and payload is json
                self.frame_manager.on_runtime_binding_called(ev)
            }
            CdpEvent::PageLifecycleEvent(ev) => self.frame_manager.on_page_lifecycle_event(ev),
            CdpEvent::PageFrameStartedLoading(ev) => {
                self.frame_manager.on_frame_started_loading(ev);
            }

            // `Target` events
            CdpEvent::TargetAttachedToTarget(ev) => {
                if ev.waiting_for_debugger {
                    let runtime_cmd = RunIfWaitingForDebuggerParams::default();

                    self.queued_events.push_back(TargetEvent::Request(Request {
                        method: runtime_cmd.identifier(),
                        session_id: Some(ev.session_id.clone().into()),
                        params: serde_json::to_value(runtime_cmd).unwrap(),
                    }));
                }

                if "service_worker" == &ev.target_info.r#type {
                    let detach_command = DetachFromTargetParams::builder()
                        .session_id(ev.session_id.clone())
                        .build();

                    self.queued_events.push_back(TargetEvent::Request(Request {
                        method: detach_command.identifier(),
                        session_id: self.session_id.clone().map(Into::into),
                        params: serde_json::to_value(detach_command).unwrap(),
                    }));
                }
            }

            // `NetworkManager` events
            CdpEvent::FetchRequestPaused(ev) => self.network_manager.on_fetch_request_paused(ev),
            CdpEvent::FetchAuthRequired(ev) => self.network_manager.on_fetch_auth_required(ev),
            CdpEvent::NetworkRequestWillBeSent(ev) => {
                self.network_manager.on_request_will_be_sent(ev)
            }
            CdpEvent::NetworkRequestServedFromCache(ev) => {
                self.network_manager.on_request_served_from_cache(ev)
            }
            CdpEvent::NetworkResponseReceived(ev) => self.network_manager.on_response_received(ev),
            CdpEvent::NetworkLoadingFinished(ev) => {
                self.network_manager.on_network_loading_finished(ev)
            }
            CdpEvent::NetworkLoadingFailed(ev) => {
                self.network_manager.on_network_loading_failed(ev)
            }
            _ => {}
        }
        chromiumoxide_cdp::consume_event!(match params {
           |ev| self.event_listeners.start_send(ev),
           |json| { let _ = self.event_listeners.try_send_custom(&method, json);}
        });
    }

    /// Called when a init command timed out
    fn on_initialization_failed(&mut self) -> TargetEvent {
        if let Some(initiator) = self.initiator.take() {
            let _ = initiator.send(Err(CdpError::Timeout));
        }
        self.init_state = TargetInit::Closing;
        let close_target = CloseTargetParams::new(self.info.target_id.clone());
        TargetEvent::Request(Request {
            method: close_target.identifier(),
            session_id: self.session_id.clone().map(Into::into),
            params: serde_json::to_value(close_target).unwrap(),
        })
    }

    /// Advance that target's state
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>, now: Instant) -> Option<TargetEvent> {
        if !self.is_page() {
            // can only poll pages
            return None;
        }
        match &mut self.init_state {
            TargetInit::AttachToTarget => {
                self.init_state = TargetInit::InitializingFrame(FrameManager::init_commands(
                    self.config.request_timeout,
                ));
                let params = AttachToTargetParams::builder()
                    .target_id(self.target_id().clone())
                    .flatten(true)
                    .build()
                    .unwrap();

                return Some(TargetEvent::Request(Request::new(
                    params.identifier(),
                    serde_json::to_value(params).unwrap(),
                )));
            }
            TargetInit::InitializingFrame(cmds) => {
                self.session_id.as_ref()?;
                if let Poll::Ready(poll) = cmds.poll(now) {
                    return match poll {
                        None => {
                            if let Some(isolated_world_cmds) =
                                self.frame_manager.ensure_isolated_world(UTILITY_WORLD_NAME)
                            {
                                *cmds = isolated_world_cmds;
                            } else {
                                self.init_state = TargetInit::InitializingNetwork(
                                    self.network_manager.init_commands(),
                                );
                            }
                            self.poll(cx, now)
                        }
                        Some(Ok((method, params))) => Some(TargetEvent::Request(Request {
                            method,
                            session_id: self.session_id.clone().map(Into::into),
                            params,
                        })),
                        Some(Err(_)) => Some(self.on_initialization_failed()),
                    };
                } else {
                    return None;
                }
            }
            TargetInit::InitializingNetwork(cmds) => {
                advance_state!(
                    self,
                    cx,
                    now,
                    cmds,
                    TargetInit::InitializingPage(Self::page_init_commands(
                        self.config.request_timeout
                    ))
                );
            }
            TargetInit::InitializingPage(cmds) => {
                advance_state!(
                    self,
                    cx,
                    now,
                    cmds,
                    match self.config.viewport.as_ref() {
                        Some(viewport) => TargetInit::InitializingEmulation(
                            self.emulation_manager.init_commands(viewport)
                        ),
                        None => TargetInit::Initialized,
                    }
                );
            }
            TargetInit::InitializingEmulation(cmds) => {
                advance_state!(self, cx, now, cmds, TargetInit::Initialized);
            }
            TargetInit::Initialized => {
                if let Some(initiator) = self.initiator.take() {
                    // make sure that the main frame of the page has finished loading
                    if self
                        .frame_manager
                        .main_frame()
                        .map(|frame| frame.is_loaded())
                        .unwrap_or_default()
                    {
                        if let Some(page) = self.get_or_create_page() {
                            let _ = initiator.send(Ok(page.clone().into()));
                        } else {
                            self.initiator = Some(initiator);
                        }
                    } else {
                        self.initiator = Some(initiator);
                    }
                }
            }
            TargetInit::Closing => return None,
        };
        loop {
            if let Some(frame) = self.frame_manager.main_frame() {
                if frame.is_loaded() {
                    while let Some(tx) = self.wait_for_frame_navigation.pop() {
                        let _ = tx.send(frame.http_request().cloned());
                    }
                }
            }

            // Drain queued messages first.
            if let Some(ev) = self.queued_events.pop_front() {
                return Some(ev);
            }

            if let Some(handle) = self.page.as_mut() {
                while let Poll::Ready(Some(msg)) = Pin::new(&mut handle.rx).poll_next(cx) {
                    match msg {
                        TargetMessage::Command(cmd) => {
                            self.queued_events.push_back(TargetEvent::Command(cmd));
                        }
                        TargetMessage::MainFrame(tx) => {
                            let _ =
                                tx.send(self.frame_manager.main_frame().map(|f| f.id().clone()));
                        }
                        TargetMessage::AllFrames(tx) => {
                            let _ = tx.send(
                                self.frame_manager
                                    .frames()
                                    .map(|f| f.id().clone())
                                    .collect(),
                            );
                        }
                        TargetMessage::Url(req) => {
                            let GetUrl { frame_id, tx } = req;
                            let frame = if let Some(frame_id) = frame_id {
                                self.frame_manager.frame(&frame_id)
                            } else {
                                self.frame_manager.main_frame()
                            };
                            let _ = tx.send(frame.and_then(|f| f.url().map(str::to_string)));
                        }
                        TargetMessage::Name(req) => {
                            let GetName { frame_id, tx } = req;
                            let frame = if let Some(frame_id) = frame_id {
                                self.frame_manager.frame(&frame_id)
                            } else {
                                self.frame_manager.main_frame()
                            };
                            let _ = tx.send(frame.and_then(|f| f.name().map(str::to_string)));
                        }
                        TargetMessage::Parent(req) => {
                            let GetParent { frame_id, tx } = req;
                            let frame = self.frame_manager.frame(&frame_id);
                            let _ = tx.send(frame.and_then(|f| f.parent_id().cloned()));
                        }
                        TargetMessage::WaitForNavigation(tx) => {
                            if let Some(frame) = self.frame_manager.main_frame() {
                                // TODO submit a navigation watcher: waitForFrameNavigation

                                // TODO return the watchers navigationResponse
                                if frame.is_loaded() {
                                    let _ = tx.send(frame.http_request().cloned());
                                } else {
                                    self.wait_for_frame_navigation.push(tx);
                                }
                            } else {
                                self.wait_for_frame_navigation.push(tx);
                            }
                        }
                        TargetMessage::AddEventListener(req) => {
                            // register a new listener
                            self.event_listeners.add_listener(req);
                        }
                        TargetMessage::GetExecutionContext(ctx) => {
                            let GetExecutionContext {
                                dom_world,
                                frame_id,
                                tx,
                            } = ctx;
                            let frame = if let Some(frame_id) = frame_id {
                                self.frame_manager.frame(&frame_id)
                            } else {
                                self.frame_manager.main_frame()
                            };

                            if let Some(frame) = frame {
                                match dom_world {
                                    DOMWorldKind::Main => {
                                        let _ = tx.send(frame.main_world().execution_context());
                                    }
                                    DOMWorldKind::Secondary => {
                                        let _ =
                                            tx.send(frame.secondary_world().execution_context());
                                    }
                                }
                            } else {
                                let _ = tx.send(None);
                            }
                        }
                        TargetMessage::Authenticate(credentials) => {
                            self.network_manager.authenticate(credentials);
                        }
                    }
                }
            }

            while let Some(event) = self.network_manager.poll() {
                match event {
                    NetworkEvent::SendCdpRequest((method, params)) => {
                        // send a message to the browser
                        self.queued_events.push_back(TargetEvent::Request(Request {
                            method,
                            session_id: self.session_id.clone().map(Into::into),
                            params,
                        }))
                    }
                    NetworkEvent::Request(_) => {}
                    NetworkEvent::Response(_) => {}
                    NetworkEvent::RequestFailed(request) => {
                        self.frame_manager.on_http_request_finished(request);
                    }
                    NetworkEvent::RequestFinished(request) => {
                        self.frame_manager.on_http_request_finished(request);
                    }
                }
            }

            while let Some(event) = self.frame_manager.poll(now) {
                match event {
                    FrameEvent::NavigationResult(res) => {
                        self.queued_events
                            .push_back(TargetEvent::NavigationResult(res));
                    }
                    FrameEvent::NavigationRequest(id, req) => {
                        self.queued_events
                            .push_back(TargetEvent::NavigationRequest(id, req));
                    }
                }
            }

            if self.queued_events.is_empty() {
                return None;
            }
        }
    }

    /// Set the sender half of the channel who requested the creation of this
    /// target
    pub fn set_initiator(&mut self, tx: Sender<Result<Page>>) {
        self.initiator = Some(tx);
    }

    pub(crate) fn page_init_commands(timeout: Duration) -> CommandChain {
        let attach = SetAutoAttachParams::builder()
            .flatten(true)
            .auto_attach(true)
            .wait_for_debugger_on_start(true)
            .build()
            .unwrap();
        let enable_performance = performance::EnableParams::default();
        let enable_log = cdplog::EnableParams::default();
        CommandChain::new(
            vec![
                (attach.identifier(), serde_json::to_value(attach).unwrap()),
                (
                    enable_performance.identifier(),
                    serde_json::to_value(enable_performance).unwrap(),
                ),
                (
                    enable_log.identifier(),
                    serde_json::to_value(enable_log).unwrap(),
                ),
            ],
            timeout,
        )
    }
}

#[derive(Debug, Clone)]
pub struct TargetConfig {
    pub ignore_https_errors: bool,
    ///  Request timeout to use
    pub request_timeout: Duration,
    pub viewport: Option<Viewport>,
    pub request_intercept: bool,
    pub cache_enabled: bool,
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self {
            ignore_https_errors: true,
            request_timeout: Duration::from_secs(REQUEST_TIMEOUT),
            viewport: Default::default(),
            request_intercept: false,
            cache_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TargetType {
    Page,
    BackgroundPage,
    ServiceWorker,
    SharedWorker,
    Other,
    Browser,
    Webview,
    Unknown(String),
}

impl TargetType {
    pub fn new(ty: &str) -> Self {
        match ty {
            "page" => TargetType::Page,
            "background_page" => TargetType::BackgroundPage,
            "service_worker" => TargetType::ServiceWorker,
            "shared_worker" => TargetType::SharedWorker,
            "other" => TargetType::Other,
            "browser" => TargetType::Browser,
            "webview" => TargetType::Webview,
            s => TargetType::Unknown(s.to_string()),
        }
    }

    pub fn is_page(&self) -> bool {
        matches!(self, TargetType::Page)
    }

    pub fn is_background_page(&self) -> bool {
        matches!(self, TargetType::BackgroundPage)
    }

    pub fn is_service_worker(&self) -> bool {
        matches!(self, TargetType::ServiceWorker)
    }

    pub fn is_shared_worker(&self) -> bool {
        matches!(self, TargetType::SharedWorker)
    }

    pub fn is_other(&self) -> bool {
        matches!(self, TargetType::Other)
    }

    pub fn is_browser(&self) -> bool {
        matches!(self, TargetType::Browser)
    }

    pub fn is_webview(&self) -> bool {
        matches!(self, TargetType::Webview)
    }
}

#[derive(Debug)]
pub(crate) enum TargetEvent {
    /// An internal request
    Request(Request),
    /// An internal navigation request
    NavigationRequest(NavigationId, Request),
    /// Indicates that a previous requested navigation has finished
    NavigationResult(Result<NavigationOk, NavigationError>),
    /// A new command arrived via a channel
    Command(CommandMessage),
}

// TODO this can be moved into the classes?
#[derive(Debug)]
pub enum TargetInit {
    InitializingFrame(CommandChain),
    InitializingNetwork(CommandChain),
    InitializingPage(CommandChain),
    InitializingEmulation(CommandChain),
    AttachToTarget,
    Initialized,
    Closing,
}

impl TargetInit {
    fn commands_mut(&mut self) -> Option<&mut CommandChain> {
        match self {
            TargetInit::InitializingFrame(cmd) => Some(cmd),
            TargetInit::InitializingNetwork(cmd) => Some(cmd),
            TargetInit::InitializingPage(cmd) => Some(cmd),
            TargetInit::InitializingEmulation(cmd) => Some(cmd),
            TargetInit::AttachToTarget => None,
            TargetInit::Initialized => None,
            TargetInit::Closing => None,
        }
    }
}

#[derive(Debug)]
pub struct GetExecutionContext {
    /// For which world the execution context was requested
    pub dom_world: DOMWorldKind,
    /// The if of the frame to get the `ExecutionContext` for
    pub frame_id: Option<FrameId>,
    /// Sender half of the channel to send the response back
    pub tx: Sender<Option<ExecutionContextId>>,
}

impl GetExecutionContext {
    pub fn new(tx: Sender<Option<ExecutionContextId>>) -> Self {
        Self {
            dom_world: DOMWorldKind::Main,
            frame_id: None,
            tx,
        }
    }
}

#[derive(Debug)]
pub struct GetUrl {
    /// The id of the frame to get the url for (None = main frame)
    pub frame_id: Option<FrameId>,
    /// Sender half of the channel to send the response back
    pub tx: Sender<Option<String>>,
}

impl GetUrl {
    pub fn new(tx: Sender<Option<String>>) -> Self {
        Self { frame_id: None, tx }
    }
}

#[derive(Debug)]
pub struct GetName {
    /// The id of the frame to get the name for (None = main frame)
    pub frame_id: Option<FrameId>,
    /// Sender half of the channel to send the response back
    pub tx: Sender<Option<String>>,
}

#[derive(Debug)]
pub struct GetParent {
    /// The id of the frame to get the parent for (None = main frame)
    pub frame_id: FrameId,
    /// Sender half of the channel to send the response back
    pub tx: Sender<Option<FrameId>>,
}

#[derive(Debug)]
pub enum TargetMessage {
    /// Execute a command within the session of this target
    Command(CommandMessage),
    /// Return the main frame of this target's page
    MainFrame(Sender<Option<FrameId>>),
    /// Return all the frames of this target's page
    AllFrames(Sender<Vec<FrameId>>),
    /// Return the url if available
    Url(GetUrl),
    /// Return the name if available
    Name(GetName),
    /// Return the parent id of a frame
    Parent(GetParent),
    /// A Message that resolves when the frame finished loading a new url
    WaitForNavigation(Sender<ArcHttpRequest>),
    /// A request to submit a new listener that gets notified with every
    /// received event
    AddEventListener(EventListenerRequest),
    /// Get the `ExecutionContext` if available
    GetExecutionContext(GetExecutionContext),
    Authenticate(Credentials),
}
