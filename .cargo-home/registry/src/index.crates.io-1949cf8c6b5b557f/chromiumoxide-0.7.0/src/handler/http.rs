use chromiumoxide_cdp::cdp::browser_protocol::network::{InterceptionId, RequestId, Response};
use chromiumoxide_cdp::cdp::browser_protocol::page::FrameId;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    request_id: RequestId,
    pub from_memory_cache: bool,
    pub failure_text: Option<String>,
    pub interception_id: Option<InterceptionId>,
    pub response: Option<Response>,
    pub headers: HashMap<String, String>,
    pub frame: Option<FrameId>,
    pub is_navigation_request: bool,
    pub allow_interception: bool,
    pub interception_handled: bool,
    pub method: Option<String>,
    pub url: Option<String>,
    pub resource_type: Option<String>,
    pub post_data: Option<String>,
    pub redirect_chain: Vec<HttpRequest>,
}

impl HttpRequest {
    pub fn new(
        request_id: RequestId,
        frame: Option<FrameId>,
        interception_id: Option<InterceptionId>,
        allow_interception: bool,
        redirect_chain: Vec<HttpRequest>,
    ) -> Self {
        Self {
            request_id,
            from_memory_cache: false,
            failure_text: None,
            interception_id,
            response: None,
            headers: Default::default(),
            frame,
            is_navigation_request: false,
            allow_interception,
            interception_handled: false,
            method: None,
            url: None,
            resource_type: None,
            post_data: None,
            redirect_chain,
        }
    }

    pub fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    pub(crate) fn set_response(&mut self, response: Response) {
        self.response = Some(response)
    }
}
