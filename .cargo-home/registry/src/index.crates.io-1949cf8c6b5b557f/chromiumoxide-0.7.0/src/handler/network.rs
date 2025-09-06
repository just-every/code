use chromiumoxide_cdp::cdp::browser_protocol::fetch::{
    self, AuthChallengeResponse, AuthChallengeResponseResponse, ContinueRequestParams,
    ContinueWithAuthParams, DisableParams, EventAuthRequired, EventRequestPaused, RequestPattern,
};
use chromiumoxide_cdp::cdp::browser_protocol::network::{
    EmulateNetworkConditionsParams, EventLoadingFailed, EventLoadingFinished,
    EventRequestServedFromCache, EventRequestWillBeSent, EventResponseReceived, Headers,
    InterceptionId, RequestId, Response, SetCacheDisabledParams, SetExtraHttpHeadersParams,
};
use chromiumoxide_cdp::cdp::browser_protocol::{
    network::EnableParams, security::SetIgnoreCertificateErrorsParams,
};
use chromiumoxide_types::{Command, Method, MethodId};

use crate::auth::Credentials;
use crate::cmd::CommandChain;
use crate::handler::http::HttpRequest;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

#[derive(Debug)]
pub struct NetworkManager {
    queued_events: VecDeque<NetworkEvent>,
    ignore_httpserrors: bool,
    requests: HashMap<RequestId, HttpRequest>,
    // TODO put event in an Arc?
    requests_will_be_sent: HashMap<RequestId, EventRequestWillBeSent>,
    extra_headers: HashMap<String, String>,
    request_id_to_interception_id: HashMap<RequestId, InterceptionId>,
    user_cache_disabled: bool,
    attempted_authentications: HashSet<RequestId>,
    credentials: Option<Credentials>,
    user_request_interception_enabled: bool,
    protocol_request_interception_enabled: bool,
    offline: bool,
    request_timeout: Duration,
}

impl NetworkManager {
    pub fn new(ignore_httpserrors: bool, request_timeout: Duration) -> Self {
        Self {
            queued_events: Default::default(),
            ignore_httpserrors,
            requests: Default::default(),
            requests_will_be_sent: Default::default(),
            extra_headers: Default::default(),
            request_id_to_interception_id: Default::default(),
            user_cache_disabled: false,
            attempted_authentications: Default::default(),
            credentials: None,
            user_request_interception_enabled: false,
            protocol_request_interception_enabled: false,
            offline: false,
            request_timeout,
        }
    }

    pub fn init_commands(&self) -> CommandChain {
        let enable = EnableParams::default();
        let cmds = if self.ignore_httpserrors {
            let ignore = SetIgnoreCertificateErrorsParams::new(true);
            vec![
                (enable.identifier(), serde_json::to_value(enable).unwrap()),
                (ignore.identifier(), serde_json::to_value(ignore).unwrap()),
            ]
        } else {
            vec![(enable.identifier(), serde_json::to_value(enable).unwrap())]
        };
        CommandChain::new(cmds, self.request_timeout)
    }

    fn push_cdp_request<T: Command>(&mut self, cmd: T) {
        let method = cmd.identifier();
        let params = serde_json::to_value(cmd).expect("Command should not panic");
        self.queued_events
            .push_back(NetworkEvent::SendCdpRequest((method, params)));
    }

    /// The next event to handle
    pub fn poll(&mut self) -> Option<NetworkEvent> {
        self.queued_events.pop_front()
    }

    pub fn extra_headers(&self) -> &HashMap<String, String> {
        &self.extra_headers
    }

    pub fn set_extra_headers(&mut self, headers: HashMap<String, String>) {
        self.extra_headers = headers;
        let headers = serde_json::to_value(self.extra_headers.clone()).unwrap();
        self.push_cdp_request(SetExtraHttpHeadersParams::new(Headers::new(headers)));
    }

    pub fn set_request_interception(&mut self, enabled: bool) {
        self.user_request_interception_enabled = enabled;
        self.update_protocol_request_interception();
    }

    pub fn set_cache_enabled(&mut self, enabled: bool) {
        self.user_cache_disabled = !enabled;
        self.update_protocol_cache_disabled();
    }

    pub fn update_protocol_cache_disabled(&mut self) {
        self.push_cdp_request(SetCacheDisabledParams::new(
            self.user_cache_disabled || self.protocol_request_interception_enabled,
        ));
    }

    pub fn authenticate(&mut self, credentials: Credentials) {
        self.credentials = Some(credentials);
        self.update_protocol_request_interception()
    }

    fn update_protocol_request_interception(&mut self) {
        let enabled = self.user_request_interception_enabled || self.credentials.is_some();
        if enabled == self.protocol_request_interception_enabled {
            return;
        }
        self.update_protocol_cache_disabled();
        if enabled {
            self.push_cdp_request(
                fetch::EnableParams::builder()
                    .handle_auth_requests(true)
                    .pattern(RequestPattern::builder().url_pattern("*").build())
                    .build(),
            )
        } else {
            self.push_cdp_request(DisableParams::default())
        }
    }

    pub fn on_fetch_request_paused(&mut self, event: &EventRequestPaused) {
        if !self.user_request_interception_enabled && self.protocol_request_interception_enabled {
            self.push_cdp_request(ContinueRequestParams::new(event.request_id.clone()))
        }
        if let Some(network_id) = event.network_id.as_ref() {
            if let Some(request_will_be_sent) =
                self.requests_will_be_sent.remove(network_id.as_ref())
            {
                self.on_request(&request_will_be_sent, Some(event.request_id.clone().into()));
            } else {
                self.request_id_to_interception_id
                    .insert(network_id.clone(), event.request_id.clone().into());
            }
        }
    }

    pub fn on_fetch_auth_required(&mut self, event: &EventAuthRequired) {
        let response = if self
            .attempted_authentications
            .contains(event.request_id.as_ref())
        {
            AuthChallengeResponseResponse::CancelAuth
        } else if self.credentials.is_some() {
            self.attempted_authentications
                .insert(event.request_id.clone().into());
            AuthChallengeResponseResponse::ProvideCredentials
        } else {
            AuthChallengeResponseResponse::Default
        };

        let mut auth = AuthChallengeResponse::new(response);
        if let Some(creds) = self.credentials.clone() {
            auth.username = Some(creds.username);
            auth.password = Some(creds.password);
        }
        self.push_cdp_request(ContinueWithAuthParams::new(event.request_id.clone(), auth));
    }

    pub fn set_offline_mode(&mut self, value: bool) {
        if self.offline == value {
            return;
        }
        self.offline = value;
        self.push_cdp_request(
            EmulateNetworkConditionsParams::builder()
                .offline(self.offline)
                .latency(0)
                .download_throughput(-1.)
                .upload_throughput(-1.)
                .build()
                .unwrap(),
        );
    }

    /// Request interception doesn't happen for data URLs with Network Service.
    pub fn on_request_will_be_sent(&mut self, event: &EventRequestWillBeSent) {
        if self.protocol_request_interception_enabled && !event.request.url.starts_with("data:") {
            if let Some(interception_id) = self
                .request_id_to_interception_id
                .remove(event.request_id.as_ref())
            {
                self.on_request(event, Some(interception_id));
            } else {
                // TODO remove the clone for event
                self.requests_will_be_sent
                    .insert(event.request_id.clone(), event.clone());
            }
        } else {
            self.on_request(event, None);
        }
    }

    pub fn on_request_served_from_cache(&mut self, event: &EventRequestServedFromCache) {
        if let Some(request) = self.requests.get_mut(event.request_id.as_ref()) {
            request.from_memory_cache = true;
        }
    }

    pub fn on_response_received(&mut self, event: &EventResponseReceived) {
        if let Some(mut request) = self.requests.remove(event.request_id.as_ref()) {
            request.set_response(event.response.clone());
            self.queued_events
                .push_back(NetworkEvent::RequestFinished(request))
        }
    }

    pub fn on_network_loading_finished(&mut self, event: &EventLoadingFinished) {
        if let Some(request) = self.requests.remove(event.request_id.as_ref()) {
            if let Some(interception_id) = request.interception_id.as_ref() {
                self.attempted_authentications
                    .remove(interception_id.as_ref());
            }
            self.queued_events
                .push_back(NetworkEvent::RequestFinished(request));
        }
    }

    pub fn on_network_loading_failed(&mut self, event: &EventLoadingFailed) {
        if let Some(mut request) = self.requests.remove(event.request_id.as_ref()) {
            request.failure_text = Some(event.error_text.clone());
            if let Some(interception_id) = request.interception_id.as_ref() {
                self.attempted_authentications
                    .remove(interception_id.as_ref());
            }
            self.queued_events
                .push_back(NetworkEvent::RequestFailed(request));
        }
    }

    fn on_request(
        &mut self,
        event: &EventRequestWillBeSent,
        interception_id: Option<InterceptionId>,
    ) {
        let mut redirect_chain = Vec::new();
        if let Some(redirect_resp) = event.redirect_response.as_ref() {
            if let Some(mut request) = self.requests.remove(event.request_id.as_ref()) {
                self.handle_request_redirect(&mut request, redirect_resp.clone());
                redirect_chain = std::mem::take(&mut request.redirect_chain);
                redirect_chain.push(request);
            }
        }
        let request = HttpRequest::new(
            event.request_id.clone(),
            event.frame_id.clone(),
            interception_id,
            self.user_request_interception_enabled,
            redirect_chain,
        );

        self.requests.insert(event.request_id.clone(), request);
        self.queued_events
            .push_back(NetworkEvent::Request(event.request_id.clone()));
    }

    fn handle_request_redirect(&mut self, request: &mut HttpRequest, response: Response) {
        request.set_response(response);
        if let Some(interception_id) = request.interception_id.as_ref() {
            self.attempted_authentications
                .remove(interception_id.as_ref());
        }
    }
}

#[derive(Debug)]
pub enum NetworkEvent {
    SendCdpRequest((MethodId, serde_json::Value)),
    Request(RequestId),
    Response(RequestId),
    RequestFailed(HttpRequest),
    RequestFinished(HttpRequest),
}
