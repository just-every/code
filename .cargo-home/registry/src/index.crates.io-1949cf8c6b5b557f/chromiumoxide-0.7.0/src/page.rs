use std::path::Path;
use std::sync::Arc;

use futures::channel::mpsc::unbounded;
use futures::channel::oneshot::channel as oneshot_channel;
use futures::{stream, SinkExt, StreamExt};

use chromiumoxide_cdp::cdp::browser_protocol::dom::*;
use chromiumoxide_cdp::cdp::browser_protocol::emulation::{
    MediaFeature, SetEmulatedMediaParams, SetGeolocationOverrideParams, SetLocaleOverrideParams,
    SetTimezoneOverrideParams,
};
use chromiumoxide_cdp::cdp::browser_protocol::network::{
    Cookie, CookieParam, DeleteCookiesParams, GetCookiesParams, SetCookiesParams,
    SetUserAgentOverrideParams,
};
use chromiumoxide_cdp::cdp::browser_protocol::page::*;
use chromiumoxide_cdp::cdp::browser_protocol::performance::{GetMetricsParams, Metric};
use chromiumoxide_cdp::cdp::browser_protocol::target::{SessionId, TargetId};
use chromiumoxide_cdp::cdp::js_protocol;
use chromiumoxide_cdp::cdp::js_protocol::debugger::GetScriptSourceParams;
use chromiumoxide_cdp::cdp::js_protocol::runtime::{
    AddBindingParams, CallArgument, CallFunctionOnParams, EvaluateParams, ExecutionContextId,
    RemoteObjectType, ScriptId,
};
use chromiumoxide_cdp::cdp::{browser_protocol, IntoEventKind};
use chromiumoxide_types::*;

use crate::auth::Credentials;
use crate::element::Element;
use crate::error::{CdpError, Result};
use crate::handler::commandfuture::CommandFuture;
use crate::handler::domworld::DOMWorldKind;
use crate::handler::httpfuture::HttpFuture;
use crate::handler::target::{GetName, GetParent, GetUrl, TargetMessage};
use crate::handler::PageInner;
use crate::js::{Evaluation, EvaluationResult};
use crate::layout::Point;
use crate::listeners::{EventListenerRequest, EventStream};
use crate::{utils, ArcHttpRequest};

#[derive(Debug, Clone)]
pub struct Page {
    inner: Arc<PageInner>,
}

impl Page {
    /// Removes the `navigator.webdriver` property
    /// changes permissions, pluggins rendering contexts and the `window.chrome`
    /// property to make it harder to detect the scraper as a bot
    async fn _enable_stealth_mode(&self) -> Result<()> {
        self.hide_webdriver().await?;
        self.hide_permissions().await?;
        self.hide_plugins().await?;
        self.hide_webgl_vendor().await?;
        self.hide_chrome().await?;

        Ok(())
    }

    /// Changes your user_agent, removes the `navigator.webdriver` property
    /// changes permissions, pluggins rendering contexts and the `window.chrome`
    /// property to make it harder to detect the scraper as a bot
    pub async fn enable_stealth_mode(&self) -> Result<()> {
        self._enable_stealth_mode().await?;
        self.set_user_agent("Mozilla/5.0 (Windows NT 11.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/107.0.5296.0 Safari/537.36").await?;

        Ok(())
    }

    /// Changes your user_agent with a custom agent, removes the `navigator.webdriver` property
    /// changes permissions, pluggins rendering contexts and the `window.chrome`
    /// property to make it harder to detect the scraper as a bot
    pub async fn enable_stealth_mode_with_agent(&self, ua: &str) -> Result<()> {
        self._enable_stealth_mode().await?;
        if !ua.is_empty() {
            self.set_user_agent(ua).await?;
        }
        Ok(())
    }

    /// Sets `window.chrome` on frame creation
    async fn hide_chrome(&self) -> Result<(), CdpError> {
        self.execute(AddScriptToEvaluateOnNewDocumentParams {
            source: "window.chrome = { runtime: {} };".to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        })
        .await?;
        Ok(())
    }

    /// Obfuscates WebGL vendor on frame creation
    async fn hide_webgl_vendor(&self) -> Result<(), CdpError> {
        self
            .execute(AddScriptToEvaluateOnNewDocumentParams {
                source: "
                    const getParameter = WebGLRenderingContext.getParameter;
                    WebGLRenderingContext.prototype.getParameter = function (parameter) {
                        if (parameter === 37445) {
                            return 'Google Inc. (NVIDIA)';
                        }
    
                        if (parameter === 37446) {
                            return 'ANGLE (NVIDIA, NVIDIA GeForce GTX 1050 Direct3D11 vs_5_0 ps_5_0, D3D11-27.21.14.5671)';
                        }
    
                        return getParameter(parameter);
                    };
                "
                .to_string(),
                world_name: None,
                include_command_line_api: None,
                run_immediately: None,
            })
            .await?;
        Ok(())
    }

    /// Obfuscates browser plugins on frame creation
    async fn hide_plugins(&self) -> Result<(), CdpError> {
        self.execute(AddScriptToEvaluateOnNewDocumentParams {
            source: "
                    Object.defineProperty(
                        navigator,
                        'plugins',
                        {
                            get: () => [
                                { filename: 'internal-pdf-viewer' },
                                { filename: 'adsfkjlkjhalkh' },
                                { filename: 'internal-nacl-plugin '}
                            ],
                        }
                    );
                "
            .to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        })
        .await?;
        Ok(())
    }

    /// Obfuscates browser permissions on frame creation
    async fn hide_permissions(&self) -> Result<(), CdpError> {
        self.execute(AddScriptToEvaluateOnNewDocumentParams {
            source: "
                    const originalQuery = window.navigator.permissions.query;
                    window.navigator.permissions.__proto__.query = parameters => {
                        return parameters.name === 'notifications'
                            ? Promise.resolve({ state: Notification.permission })
                            : originalQuery(parameters);
                    }
                "
            .to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        })
        .await?;
        Ok(())
    }

    /// Removes the `navigator.webdriver` property on frame creation
    async fn hide_webdriver(&self) -> Result<(), CdpError> {
        self.execute(AddScriptToEvaluateOnNewDocumentParams {
            source: "
                    Object.defineProperty(
                        navigator,
                        'webdriver',
                        { get: () => undefined }
                    );
                "
            .to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        })
        .await?;
        Ok(())
    }

    /// Execute a command and return the `Command::Response`
    pub async fn execute<T: Command>(&self, cmd: T) -> Result<CommandResponse<T::Response>> {
        self.command_future(cmd)?.await
    }

    /// Execute a command and return the `Command::Response`
    pub fn command_future<T: Command>(&self, cmd: T) -> Result<CommandFuture<T>> {
        self.inner.command_future(cmd)
    }

    /// Execute a command and return the `Command::Response`
    pub fn http_future<T: Command>(&self, cmd: T) -> Result<HttpFuture<T>> {
        self.inner.http_future(cmd)
    }

    /// Adds an event listener to the `Target` and returns the receiver part as
    /// `EventStream`
    ///
    /// An `EventStream` receives every `Event` the `Target` receives.
    /// All event listener get notified with the same event, so registering
    /// multiple listeners for the same event is possible.
    ///
    /// Custom events rely on being deserializable from the received json params
    /// in the `EventMessage`. Custom Events are caught by the `CdpEvent::Other`
    /// variant. If there are mulitple custom event listener is registered
    /// for the same event, identified by the `MethodType::method_id` function,
    /// the `Target` tries to deserialize the json using the type of the event
    /// listener. Upon success the `Target` then notifies all listeners with the
    /// deserialized event. This means, while it is possible to register
    /// different types for the same custom event, only the type of first
    /// registered event listener will be used. The subsequent listeners, that
    /// registered for the same event but with another type won't be able to
    /// receive anything and therefor will come up empty until all their
    /// preceding event listeners are dropped and they become the first (or
    /// longest) registered event listener for an event.
    ///
    /// # Example Listen for canceled animations
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide_cdp::cdp::browser_protocol::animation::EventAnimationCanceled;
    /// # use futures::StreamExt;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let mut events = page.event_listener::<EventAnimationCanceled>().await?;
    ///     while let Some(event) = events.next().await {
    ///         //..
    ///     }
    ///     # Ok(())
    /// # }
    /// ```
    ///
    /// # Example Liste for a custom event
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use futures::StreamExt;
    /// # use serde::Deserialize;
    /// # use chromiumoxide::types::{MethodId, MethodType};
    /// # use chromiumoxide::cdp::CustomEvent;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     #[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
    ///     struct MyCustomEvent {
    ///         name: String,
    ///     }
    ///    impl MethodType for MyCustomEvent {
    ///        fn method_id() -> MethodId {
    ///            "Custom.Event".into()
    ///        }
    ///    }
    ///    impl CustomEvent for MyCustomEvent {}
    ///    let mut events = page.event_listener::<MyCustomEvent>().await?;
    ///    while let Some(event) = events.next().await {
    ///        //..
    ///    }
    ///
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn event_listener<T: IntoEventKind>(&self) -> Result<EventStream<T>> {
        let (tx, rx) = unbounded();
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::AddEventListener(
                EventListenerRequest::new::<T>(tx),
            ))
            .await?;

        Ok(EventStream::new(rx))
    }

    pub async fn expose_function(
        &self,
        name: impl Into<String>,
        function: impl AsRef<str>,
    ) -> Result<()> {
        let name = name.into();
        let expression = utils::evaluation_string(function, &["exposedFun", name.as_str()]);

        self.execute(AddBindingParams::new(name)).await?;
        self.execute(AddScriptToEvaluateOnNewDocumentParams::new(
            expression.clone(),
        ))
        .await?;

        // TODO add execution context tracking for frames
        //let frames = self.frames().await?;

        Ok(())
    }

    /// This resolves once the navigation finished and the page is loaded.
    ///
    /// This is necessary after an interaction with the page that may trigger a
    /// navigation (`click`, `press_key`) in order to wait until the new browser
    /// page is loaded
    pub async fn wait_for_navigation_response(&self) -> Result<ArcHttpRequest> {
        self.inner.wait_for_navigation().await
    }

    /// Same as `wait_for_navigation_response` but returns `Self` instead
    pub async fn wait_for_navigation(&self) -> Result<&Self> {
        self.inner.wait_for_navigation().await?;
        Ok(self)
    }

    /// Navigate directly to the given URL.
    ///
    /// This resolves directly after the requested URL is fully loaded.
    pub async fn goto(&self, params: impl Into<NavigateParams>) -> Result<&Self> {
        let res = self.execute(params.into()).await?;
        if let Some(err) = res.result.error_text {
            return Err(CdpError::ChromeMessage(err));
        }

        Ok(self)
    }

    /// The identifier of the `Target` this page belongs to
    pub fn target_id(&self) -> &TargetId {
        self.inner.target_id()
    }

    /// The identifier of the `Session` target of this page is attached to
    pub fn session_id(&self) -> &SessionId {
        self.inner.session_id()
    }

    /// The identifier of the `Session` target of this page is attached to
    pub fn opener_id(&self) -> &Option<TargetId> {
        self.inner.opener_id()
    }

    /// Returns the name of the frame
    pub async fn frame_name(&self, frame_id: FrameId) -> Result<Option<String>> {
        let (tx, rx) = oneshot_channel();
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::Name(GetName {
                frame_id: Some(frame_id),
                tx,
            }))
            .await?;
        Ok(rx.await?)
    }

    pub async fn authenticate(&self, credentials: Credentials) -> Result<()> {
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::Authenticate(credentials))
            .await?;

        Ok(())
    }

    /// Returns the current url of the page
    pub async fn url(&self) -> Result<Option<String>> {
        let (tx, rx) = oneshot_channel();
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::Url(GetUrl::new(tx)))
            .await?;
        Ok(rx.await?)
    }

    /// Returns the current url of the frame
    pub async fn frame_url(&self, frame_id: FrameId) -> Result<Option<String>> {
        let (tx, rx) = oneshot_channel();
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::Url(GetUrl {
                frame_id: Some(frame_id),
                tx,
            }))
            .await?;
        Ok(rx.await?)
    }

    /// Returns the parent id of the frame
    pub async fn frame_parent(&self, frame_id: FrameId) -> Result<Option<FrameId>> {
        let (tx, rx) = oneshot_channel();
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::Parent(GetParent { frame_id, tx }))
            .await?;
        Ok(rx.await?)
    }

    /// Return the main frame of the page
    pub async fn mainframe(&self) -> Result<Option<FrameId>> {
        let (tx, rx) = oneshot_channel();
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::MainFrame(tx))
            .await?;
        Ok(rx.await?)
    }

    /// Return the frames of the page
    pub async fn frames(&self) -> Result<Vec<FrameId>> {
        let (tx, rx) = oneshot_channel();
        self.inner
            .sender()
            .clone()
            .send(TargetMessage::AllFrames(tx))
            .await?;
        Ok(rx.await?)
    }

    /// Allows overriding user agent with the given string.
    pub async fn set_user_agent(
        &self,
        params: impl Into<SetUserAgentOverrideParams>,
    ) -> Result<&Self> {
        self.execute(params.into()).await?;
        Ok(self)
    }

    /// Returns the user agent of the browser
    pub async fn user_agent(&self) -> Result<String> {
        Ok(self.inner.version().await?.user_agent)
    }

    /// Returns the root DOM node (and optionally the subtree) of the page.
    ///
    /// # Note: This does not return the actual HTML document of the page. To
    /// retrieve the HTML content of the page see `Page::content`.
    pub async fn get_document(&self) -> Result<Node> {
        let resp = self.execute(GetDocumentParams::default()).await?;
        Ok(resp.result.root)
    }

    /// Returns the first element in the document which matches the given CSS
    /// selector.
    ///
    /// Execute a query selector on the document's node.
    pub async fn find_element(&self, selector: impl Into<String>) -> Result<Element> {
        let root = self.get_document().await?.node_id;
        let node_id = self.inner.find_element(selector, root).await?;
        Element::new(Arc::clone(&self.inner), node_id).await
    }

    /// Return all `Element`s in the document that match the given selector
    pub async fn find_elements(&self, selector: impl Into<String>) -> Result<Vec<Element>> {
        let root = self.get_document().await?.node_id;
        let node_ids = self.inner.find_elements(selector, root).await?;
        Element::from_nodes(&self.inner, &node_ids).await
    }

    /// Returns the first element in the document which matches the given xpath
    /// selector.
    ///
    /// Execute a xpath selector on the document's node.
    pub async fn find_xpath(&self, selector: impl Into<String>) -> Result<Element> {
        self.get_document().await?;
        let node_id = self.inner.find_xpaths(selector).await?[0];
        Element::new(Arc::clone(&self.inner), node_id).await
    }

    /// Return all `Element`s in the document that match the given xpath selector
    pub async fn find_xpaths(&self, selector: impl Into<String>) -> Result<Vec<Element>> {
        self.get_document().await?;
        let node_ids = self.inner.find_xpaths(selector).await?;
        Element::from_nodes(&self.inner, &node_ids).await
    }

    /// Describes node given its id
    pub async fn describe_node(&self, node_id: NodeId) -> Result<Node> {
        let resp = self
            .execute(
                DescribeNodeParams::builder()
                    .node_id(node_id)
                    .depth(100)
                    .build(),
            )
            .await?;
        Ok(resp.result.node)
    }

    /// Tries to close page, running its beforeunload hooks, if any.
    /// Calls Page.close with [`CloseParams`]
    pub async fn close(self) -> Result<()> {
        self.execute(CloseParams::default()).await?;
        Ok(())
    }

    /// Performs a single mouse click event at the point's location.
    ///
    /// This scrolls the point into view first, then executes a
    /// `DispatchMouseEventParams` command of type `MouseLeft` with
    /// `MousePressed` as single click and then releases the mouse with an
    /// additional `DispatchMouseEventParams` of type `MouseLeft` with
    /// `MouseReleased`
    ///
    /// Bear in mind that if `click()` triggers a navigation the new page is not
    /// immediately loaded when `click()` resolves. To wait until navigation is
    /// finished an additional `wait_for_navigation()` is required:
    ///
    /// # Example
    ///
    /// Trigger a navigation and wait until the triggered navigation is finished
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide::layout::Point;
    /// # async fn demo(page: Page, point: Point) -> Result<()> {
    ///     let html = page.click(point).await?.wait_for_navigation().await?.content();
    ///     # Ok(())
    /// # }
    /// ```
    ///
    /// # Example
    ///
    /// Perform custom click
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide::layout::Point;
    /// # use chromiumoxide_cdp::cdp::browser_protocol::input::{DispatchMouseEventParams, MouseButton, DispatchMouseEventType};
    /// # async fn demo(page: Page, point: Point) -> Result<()> {
    ///      // double click
    ///      let cmd = DispatchMouseEventParams::builder()
    ///             .x(point.x)
    ///             .y(point.y)
    ///             .button(MouseButton::Left)
    ///             .click_count(2);
    ///
    ///         page.move_mouse(point).await?.execute(
    ///             cmd.clone()
    ///                 .r#type(DispatchMouseEventType::MousePressed)
    ///                 .build()
    ///                 .unwrap(),
    ///         )
    ///         .await?;
    ///
    ///         page.execute(
    ///             cmd.r#type(DispatchMouseEventType::MouseReleased)
    ///                 .build()
    ///                 .unwrap(),
    ///         )
    ///         .await?;
    ///
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn click(&self, point: Point) -> Result<&Self> {
        self.inner.click(point).await?;
        Ok(self)
    }

    /// Dispatches a `mousemove` event and moves the mouse to the position of
    /// the `point` where `Point.x` is the horizontal position of the mouse and
    /// `Point.y` the vertical position of the mouse.
    pub async fn move_mouse(&self, point: Point) -> Result<&Self> {
        self.inner.move_mouse(point).await?;
        Ok(self)
    }

    /// Take a screenshot of the current page
    pub async fn screenshot(&self, params: impl Into<ScreenshotParams>) -> Result<Vec<u8>> {
        self.inner.screenshot(params).await
    }

    /// Save a screenshot of the page
    ///
    /// # Example save a png file of a website
    ///
    /// ```no_run
    /// # use chromiumoxide::page::{Page, ScreenshotParams};
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide_cdp::cdp::browser_protocol::page::CaptureScreenshotFormat;
    /// # async fn demo(page: Page) -> Result<()> {
    ///         page.goto("http://example.com")
    ///             .await?
    ///             .save_screenshot(
    ///             ScreenshotParams::builder()
    ///                 .format(CaptureScreenshotFormat::Png)
    ///                 .full_page(true)
    ///                 .omit_background(true)
    ///                 .build(),
    ///             "example.png",
    ///             )
    ///             .await?;
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn save_screenshot(
        &self,
        params: impl Into<ScreenshotParams>,
        output: impl AsRef<Path>,
    ) -> Result<Vec<u8>> {
        let img = self.screenshot(params).await?;
        utils::write(output.as_ref(), &img).await?;
        Ok(img)
    }

    /// Print the current page as pdf.
    ///
    /// See [`PrintToPdfParams`]
    ///
    /// # Note Generating a pdf is currently only supported in Chrome headless.
    pub async fn pdf(&self, params: PrintToPdfParams) -> Result<Vec<u8>> {
        let res = self.execute(params).await?;
        Ok(utils::base64::decode(&res.data)?)
    }

    /// Save the current page as pdf as file to the `output` path and return the
    /// pdf contents.
    ///
    /// # Note Generating a pdf is currently only supported in Chrome headless.
    pub async fn save_pdf(
        &self,
        opts: PrintToPdfParams,
        output: impl AsRef<Path>,
    ) -> Result<Vec<u8>> {
        let pdf = self.pdf(opts).await?;
        utils::write(output.as_ref(), &pdf).await?;
        Ok(pdf)
    }

    /// Brings page to front (activates tab)
    pub async fn bring_to_front(&self) -> Result<&Self> {
        self.execute(BringToFrontParams::default()).await?;
        Ok(self)
    }

    /// Emulates the given media type or media feature for CSS media queries
    pub async fn emulate_media_features(&self, features: Vec<MediaFeature>) -> Result<&Self> {
        self.execute(SetEmulatedMediaParams::builder().features(features).build())
            .await?;
        Ok(self)
    }

    /// Changes the CSS media type of the page
    // Based on https://pptr.dev/api/puppeteer.page.emulatemediatype
    pub async fn emulate_media_type(
        &self,
        media_type: impl Into<MediaTypeParams>,
    ) -> Result<&Self> {
        self.execute(
            SetEmulatedMediaParams::builder()
                .media(media_type.into())
                .build(),
        )
        .await?;
        Ok(self)
    }

    /// Overrides default host system timezone
    pub async fn emulate_timezone(
        &self,
        timezoune_id: impl Into<SetTimezoneOverrideParams>,
    ) -> Result<&Self> {
        self.execute(timezoune_id.into()).await?;
        Ok(self)
    }

    /// Overrides default host system locale with the specified one
    pub async fn emulate_locale(
        &self,
        locale: impl Into<SetLocaleOverrideParams>,
    ) -> Result<&Self> {
        self.execute(locale.into()).await?;
        Ok(self)
    }

    /// Overrides the Geolocation Position or Error. Omitting any of the parameters emulates position unavailable.
    pub async fn emulate_geolocation(
        &self,
        geolocation: impl Into<SetGeolocationOverrideParams>,
    ) -> Result<&Self> {
        self.execute(geolocation.into()).await?;
        Ok(self)
    }

    /// Reloads given page
    ///
    /// To reload ignoring cache run:
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide_cdp::cdp::browser_protocol::page::ReloadParams;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     page.execute(ReloadParams::builder().ignore_cache(true).build()).await?;
    ///     page.wait_for_navigation().await?;
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn reload(&self) -> Result<&Self> {
        self.execute(ReloadParams::default()).await?;
        self.wait_for_navigation().await
    }

    /// Enables log domain. Enabled by default.
    ///
    /// Sends the entries collected so far to the client by means of the
    /// entryAdded notification.
    ///
    /// See https://chromedevtools.github.io/devtools-protocol/tot/Log#method-enable
    pub async fn enable_log(&self) -> Result<&Self> {
        self.execute(browser_protocol::log::EnableParams::default())
            .await?;
        Ok(self)
    }

    /// Disables log domain
    ///
    /// Prevents further log entries from being reported to the client
    ///
    /// See https://chromedevtools.github.io/devtools-protocol/tot/Log#method-disable
    pub async fn disable_log(&self) -> Result<&Self> {
        self.execute(browser_protocol::log::DisableParams::default())
            .await?;
        Ok(self)
    }

    /// Enables runtime domain. Activated by default.
    pub async fn enable_runtime(&self) -> Result<&Self> {
        self.execute(js_protocol::runtime::EnableParams::default())
            .await?;
        Ok(self)
    }

    /// Disables runtime domain
    pub async fn disable_runtime(&self) -> Result<&Self> {
        self.execute(js_protocol::runtime::DisableParams::default())
            .await?;
        Ok(self)
    }

    /// Enables Debugger. Enabled by default.
    pub async fn enable_debugger(&self) -> Result<&Self> {
        self.execute(js_protocol::debugger::EnableParams::default())
            .await?;
        Ok(self)
    }

    /// Disables Debugger.
    pub async fn disable_debugger(&self) -> Result<&Self> {
        self.execute(js_protocol::debugger::DisableParams::default())
            .await?;
        Ok(self)
    }

    // Enables DOM agent
    pub async fn enable_dom(&self) -> Result<&Self> {
        self.execute(browser_protocol::dom::EnableParams::default())
            .await?;
        Ok(self)
    }

    // Disables DOM agent
    pub async fn disable_dom(&self) -> Result<&Self> {
        self.execute(browser_protocol::dom::DisableParams::default())
            .await?;
        Ok(self)
    }

    // Enables the CSS agent
    pub async fn enable_css(&self) -> Result<&Self> {
        self.execute(browser_protocol::css::EnableParams::default())
            .await?;
        Ok(self)
    }

    // Disables the CSS agent
    pub async fn disable_css(&self) -> Result<&Self> {
        self.execute(browser_protocol::css::DisableParams::default())
            .await?;
        Ok(self)
    }

    /// Activates (focuses) the target.
    pub async fn activate(&self) -> Result<&Self> {
        self.inner.activate().await?;
        Ok(self)
    }

    /// Returns all cookies that match the tab's current URL.
    pub async fn get_cookies(&self) -> Result<Vec<Cookie>> {
        Ok(self
            .execute(GetCookiesParams::default())
            .await?
            .result
            .cookies)
    }

    /// Set a single cookie
    ///
    /// This fails if the cookie's url or if not provided, the page's url is
    /// `about:blank` or a `data:` url.
    ///
    /// # Example
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide_cdp::cdp::browser_protocol::network::CookieParam;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     page.set_cookie(CookieParam::new("Cookie-name", "Cookie-value")).await?;
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn set_cookie(&self, cookie: impl Into<CookieParam>) -> Result<&Self> {
        let mut cookie = cookie.into();
        if let Some(url) = cookie.url.as_ref() {
            validate_cookie_url(url)?;
        } else {
            let url = self
                .url()
                .await?
                .ok_or_else(|| CdpError::msg("Page url not found"))?;
            validate_cookie_url(&url)?;
            if url.starts_with("http") {
                cookie.url = Some(url);
            }
        }
        self.execute(DeleteCookiesParams::from_cookie(&cookie))
            .await?;
        self.execute(SetCookiesParams::new(vec![cookie])).await?;
        Ok(self)
    }

    /// Set all the cookies
    pub async fn set_cookies(&self, mut cookies: Vec<CookieParam>) -> Result<&Self> {
        let url = self
            .url()
            .await?
            .ok_or_else(|| CdpError::msg("Page url not found"))?;
        let is_http = url.starts_with("http");
        if !is_http {
            validate_cookie_url(&url)?;
        }

        for cookie in &mut cookies {
            if let Some(url) = cookie.url.as_ref() {
                validate_cookie_url(url)?;
            } else if is_http {
                cookie.url = Some(url.clone());
            }
        }
        self.delete_cookies_unchecked(cookies.iter().map(DeleteCookiesParams::from_cookie))
            .await?;

        self.execute(SetCookiesParams::new(cookies)).await?;
        Ok(self)
    }

    /// Delete a single cookie
    pub async fn delete_cookie(&self, cookie: impl Into<DeleteCookiesParams>) -> Result<&Self> {
        let mut cookie = cookie.into();
        if cookie.url.is_none() {
            let url = self
                .url()
                .await?
                .ok_or_else(|| CdpError::msg("Page url not found"))?;
            if url.starts_with("http") {
                cookie.url = Some(url);
            }
        }
        self.execute(cookie).await?;
        Ok(self)
    }

    /// Delete all the cookies
    pub async fn delete_cookies(&self, mut cookies: Vec<DeleteCookiesParams>) -> Result<&Self> {
        let mut url: Option<(String, bool)> = None;
        for cookie in &mut cookies {
            if cookie.url.is_none() {
                if let Some((url, is_http)) = url.as_ref() {
                    if *is_http {
                        cookie.url = Some(url.clone())
                    }
                } else {
                    let page_url = self
                        .url()
                        .await?
                        .ok_or_else(|| CdpError::msg("Page url not found"))?;
                    let is_http = page_url.starts_with("http");
                    if is_http {
                        cookie.url = Some(page_url.clone())
                    }
                    url = Some((page_url, is_http));
                }
            }
        }
        self.delete_cookies_unchecked(cookies.into_iter()).await?;
        Ok(self)
    }

    /// Convenience method that prevents another channel roundtrip to get the
    /// url and validate it
    async fn delete_cookies_unchecked(
        &self,
        cookies: impl Iterator<Item = DeleteCookiesParams>,
    ) -> Result<&Self> {
        // NOTE: the buffer size is arbitrary
        let mut cmds = stream::iter(cookies.into_iter().map(|cookie| self.execute(cookie)))
            .buffer_unordered(5);
        while let Some(resp) = cmds.next().await {
            resp?;
        }
        Ok(self)
    }

    /// Returns the title of the document.
    pub async fn get_title(&self) -> Result<Option<String>> {
        let result = self.evaluate("document.title").await?;

        let title: String = result.into_value()?;

        if title.is_empty() {
            Ok(None)
        } else {
            Ok(Some(title))
        }
    }

    /// Retrieve current values of run-time metrics.
    pub async fn metrics(&self) -> Result<Vec<Metric>> {
        Ok(self
            .execute(GetMetricsParams::default())
            .await?
            .result
            .metrics)
    }

    /// Returns metrics relating to the layout of the page
    pub async fn layout_metrics(&self) -> Result<GetLayoutMetricsReturns> {
        self.inner.layout_metrics().await
    }

    /// This evaluates strictly as expression.
    ///
    /// Same as `Page::evaluate` but no fallback or any attempts to detect
    /// whether the expression is actually a function. However you can
    /// submit a function evaluation string:
    ///
    /// # Example Evaluate function call as expression
    ///
    /// This will take the arguments `(1,2)` and will call the function
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let sum: usize = page
    ///         .evaluate_expression("((a,b) => {return a + b;})(1,2)")
    ///         .await?
    ///         .into_value()?;
    ///     assert_eq!(sum, 3);
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn evaluate_expression(
        &self,
        evaluate: impl Into<EvaluateParams>,
    ) -> Result<EvaluationResult> {
        self.inner.evaluate_expression(evaluate).await
    }

    /// Evaluates an expression or function in the page's context and returns
    /// the result.
    ///
    /// In contrast to `Page::evaluate_expression` this is capable of handling
    /// function calls and expressions alike. This takes anything that is
    /// `Into<Evaluation>`. When passing a `String` or `str`, this will try to
    /// detect whether it is a function or an expression. JS function detection
    /// is not very sophisticated but works for general cases (`(async)
    /// functions` and arrow functions). If you want a string statement
    /// specifically evaluated as expression or function either use the
    /// designated functions `Page::evaluate_function` or
    /// `Page::evaluate_expression` or use the proper parameter type for
    /// `Page::execute`:  `EvaluateParams` for strict expression evaluation or
    /// `CallFunctionOnParams` for strict function evaluation.
    ///
    /// If you don't trust the js function detection and are not sure whether
    /// the statement is an expression or of type function (arrow functions: `()
    /// => {..}`), you should pass it as `EvaluateParams` and set the
    /// `EvaluateParams::eval_as_function_fallback` option. This will first
    /// try to evaluate it as expression and if the result comes back
    /// evaluated as `RemoteObjectType::Function` it will submit the
    /// statement again but as function:
    ///
    ///  # Example Evaluate function statement as expression with fallback
    /// option
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide_cdp::cdp::js_protocol::runtime::{EvaluateParams, RemoteObjectType};
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let eval = EvaluateParams::builder().expression("() => {return 42;}");
    ///     // this will fail because the `EvaluationResult` returned by the browser will be
    ///     // of type `Function`
    ///     let result = page
    ///                 .evaluate(eval.clone().build().unwrap())
    ///                 .await?;
    ///     assert_eq!(result.object().r#type, RemoteObjectType::Function);
    ///     assert!(result.into_value::<usize>().is_err());
    ///
    ///     // This will also fail on the first try but it detects that the browser evaluated the
    ///     // statement as function and then evaluate it again but as function
    ///     let sum: usize = page
    ///         .evaluate(eval.eval_as_function_fallback(true).build().unwrap())
    ///         .await?
    ///         .into_value()?;
    ///     # Ok(())
    /// # }
    /// ```
    ///
    /// # Example Evaluate basic expression
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let sum:usize = page.evaluate("1 + 2").await?.into_value()?;
    ///     assert_eq!(sum, 3);
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn evaluate(&self, evaluate: impl Into<Evaluation>) -> Result<EvaluationResult> {
        match evaluate.into() {
            Evaluation::Expression(mut expr) => {
                if expr.context_id.is_none() {
                    expr.context_id = self.execution_context().await?;
                }
                let fallback = expr.eval_as_function_fallback.and_then(|p| {
                    if p {
                        Some(expr.clone())
                    } else {
                        None
                    }
                });
                let res = self.evaluate_expression(expr).await?;

                if res.object().r#type == RemoteObjectType::Function {
                    // expression was actually a function
                    if let Some(fallback) = fallback {
                        return self.evaluate_function(fallback).await;
                    }
                }
                Ok(res)
            }
            Evaluation::Function(fun) => Ok(self.evaluate_function(fun).await?),
        }
    }

    /// Eexecutes a function withinthe page's context and returns the result.
    ///
    /// # Example Evaluate a promise
    /// This will wait until the promise resolves and then returns the result.
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let sum:usize = page.evaluate_function("() => Promise.resolve(1 + 2)").await?.into_value()?;
    ///     assert_eq!(sum, 3);
    ///     # Ok(())
    /// # }
    /// ```
    ///
    /// # Example Evaluate an async function
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let val:usize = page.evaluate_function("async function() {return 42;}").await?.into_value()?;
    ///     assert_eq!(val, 42);
    ///     # Ok(())
    /// # }
    /// ```
    /// # Example Construct a function call
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # use chromiumoxide_cdp::cdp::js_protocol::runtime::{CallFunctionOnParams, CallArgument};
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let call = CallFunctionOnParams::builder()
    ///            .function_declaration(
    ///                "(a,b) => { return a + b;}"
    ///            )
    ///            .argument(
    ///                CallArgument::builder()
    ///                    .value(serde_json::json!(1))
    ///                    .build(),
    ///            )
    ///            .argument(
    ///                CallArgument::builder()
    ///                    .value(serde_json::json!(2))
    ///                    .build(),
    ///            )
    ///            .build()
    ///            .unwrap();
    ///     let sum:usize = page.evaluate_function(call).await?.into_value()?;
    ///     assert_eq!(sum, 3);
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn evaluate_function(
        &self,
        evaluate: impl Into<CallFunctionOnParams>,
    ) -> Result<EvaluationResult> {
        self.inner.evaluate_function(evaluate).await
    }

    /// Returns the default execution context identifier of this page that
    /// represents the context for JavaScript execution.
    pub async fn execution_context(&self) -> Result<Option<ExecutionContextId>> {
        self.inner.execution_context().await
    }

    /// Returns the secondary execution context identifier of this page that
    /// represents the context for JavaScript execution for manipulating the
    /// DOM.
    ///
    /// See `Page::set_contents`
    pub async fn secondary_execution_context(&self) -> Result<Option<ExecutionContextId>> {
        self.inner.secondary_execution_context().await
    }

    pub async fn frame_execution_context(
        &self,
        frame_id: FrameId,
    ) -> Result<Option<ExecutionContextId>> {
        self.inner.frame_execution_context(frame_id).await
    }

    pub async fn frame_secondary_execution_context(
        &self,
        frame_id: FrameId,
    ) -> Result<Option<ExecutionContextId>> {
        self.inner.frame_secondary_execution_context(frame_id).await
    }

    /// Evaluates given script in every frame upon creation (before loading
    /// frame's scripts)
    pub async fn evaluate_on_new_document(
        &self,
        script: impl Into<AddScriptToEvaluateOnNewDocumentParams>,
    ) -> Result<ScriptIdentifier> {
        Ok(self.execute(script.into()).await?.result.identifier)
    }

    /// Set the content of the frame.
    ///
    /// # Example
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     page.set_content("<body>
    ///  <h1>This was set via chromiumoxide</h1>
    ///  </body>").await?;
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn set_content(&self, html: impl AsRef<str>) -> Result<&Self> {
        let mut call = CallFunctionOnParams::builder()
            .function_declaration(
                "(html) => {
            document.open();
            document.write(html);
            document.close();
        }",
            )
            .argument(
                CallArgument::builder()
                    .value(serde_json::json!(html.as_ref()))
                    .build(),
            )
            .build()
            .unwrap();

        call.execution_context_id = self
            .inner
            .execution_context_for_world(None, DOMWorldKind::Secondary)
            .await?;

        self.evaluate_function(call).await?;
        // relying that document.open() will reset frame lifecycle with "init"
        // lifecycle event. @see https://crrev.com/608658
        self.wait_for_navigation().await
    }

    /// Returns the HTML content of the page
    pub async fn content(&self) -> Result<String> {
        Ok(self
            .evaluate(
                "{
          let retVal = '';
          if (document.doctype) {
            retVal = new XMLSerializer().serializeToString(document.doctype);
          }
          if (document.documentElement) {
            retVal += document.documentElement.outerHTML;
          }
          retVal
      }
      ",
            )
            .await?
            .into_value()?)
    }

    #[cfg(feature = "bytes")]
    /// Returns the HTML content of the page
    pub async fn content_bytes(&self) -> Result<bytes::Bytes> {
        Ok(self
            .evaluate(
                "{
            let retVal = '';
            if (document.doctype) {
            retVal = new XMLSerializer().serializeToString(document.doctype);
            }
            if (document.documentElement) {
            retVal += document.documentElement.outerHTML;
            }
            retVal
        }
        ",
            )
            .await?
            .into_value()?)
    }

    /// Returns source for the script with given id.
    ///
    /// Debugger must be enabled.
    pub async fn get_script_source(&self, script_id: impl Into<String>) -> Result<String> {
        Ok(self
            .execute(GetScriptSourceParams::new(ScriptId::from(script_id.into())))
            .await?
            .result
            .script_source)
    }
}

impl From<Arc<PageInner>> for Page {
    fn from(inner: Arc<PageInner>) -> Self {
        Self { inner }
    }
}

pub(crate) fn validate_cookie_url(url: &str) -> Result<()> {
    if url.starts_with("data:") {
        Err(CdpError::msg("Data URL page can not have cookie"))
    } else if url == "about:blank" {
        Err(CdpError::msg("Blank page can not have cookie"))
    } else {
        Ok(())
    }
}

/// Page screenshot parameters with extra options.
#[derive(Debug, Default)]
pub struct ScreenshotParams {
    /// Chrome DevTools Protocol screenshot options.
    pub cdp_params: CaptureScreenshotParams,
    /// Take full page screenshot.
    pub full_page: Option<bool>,
    /// Make the background transparent (png only).
    pub omit_background: Option<bool>,
}

impl ScreenshotParams {
    pub fn builder() -> ScreenshotParamsBuilder {
        Default::default()
    }

    pub(crate) fn full_page(&self) -> bool {
        self.full_page.unwrap_or(false)
    }

    pub(crate) fn omit_background(&self) -> bool {
        self.omit_background.unwrap_or(false)
            && self
                .cdp_params
                .format
                .as_ref()
                .map_or(true, |f| f == &CaptureScreenshotFormat::Png)
    }
}

/// Page screenshot parameters builder with extra options.
#[derive(Debug, Default)]
pub struct ScreenshotParamsBuilder {
    cdp_params: CaptureScreenshotParams,
    full_page: Option<bool>,
    omit_background: Option<bool>,
}

impl ScreenshotParamsBuilder {
    /// Image compression format (defaults to png).
    pub fn format(mut self, format: impl Into<CaptureScreenshotFormat>) -> Self {
        self.cdp_params.format = Some(format.into());
        self
    }

    /// Compression quality from range [0..100] (jpeg only).
    pub fn quality(mut self, quality: impl Into<i64>) -> Self {
        self.cdp_params.quality = Some(quality.into());
        self
    }

    /// Capture the screenshot of a given region only.
    pub fn clip(mut self, clip: impl Into<Viewport>) -> Self {
        self.cdp_params.clip = Some(clip.into());
        self
    }

    /// Capture the screenshot from the surface, rather than the view (defaults to true).
    pub fn from_surface(mut self, from_surface: impl Into<bool>) -> Self {
        self.cdp_params.from_surface = Some(from_surface.into());
        self
    }

    /// Capture the screenshot beyond the viewport (defaults to false).
    pub fn capture_beyond_viewport(mut self, capture_beyond_viewport: impl Into<bool>) -> Self {
        self.cdp_params.capture_beyond_viewport = Some(capture_beyond_viewport.into());
        self
    }

    /// Full page screen capture.
    pub fn full_page(mut self, full_page: impl Into<bool>) -> Self {
        self.full_page = Some(full_page.into());
        self
    }

    /// Make the background transparent (png only)
    pub fn omit_background(mut self, omit_background: impl Into<bool>) -> Self {
        self.omit_background = Some(omit_background.into());
        self
    }

    pub fn build(self) -> ScreenshotParams {
        ScreenshotParams {
            cdp_params: self.cdp_params,
            full_page: self.full_page,
            omit_background: self.omit_background,
        }
    }
}

impl From<CaptureScreenshotParams> for ScreenshotParams {
    fn from(cdp_params: CaptureScreenshotParams) -> Self {
        Self {
            cdp_params,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum MediaTypeParams {
    /// Default CSS media type behavior for page and print
    #[default]
    Null,
    /// Force screen CSS media type for page and print
    Screen,
    /// Force print CSS media type for page and print
    Print,
}
impl From<MediaTypeParams> for String {
    fn from(media_type: MediaTypeParams) -> Self {
        match media_type {
            MediaTypeParams::Null => "null".to_string(),
            MediaTypeParams::Screen => "screen".to_string(),
            MediaTypeParams::Print => "print".to_string(),
        }
    }
}
