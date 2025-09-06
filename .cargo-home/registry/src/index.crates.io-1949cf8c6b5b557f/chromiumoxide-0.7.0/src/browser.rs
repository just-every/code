use std::future::Future;
use std::time::Duration;
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

use futures::channel::mpsc::{channel, unbounded, Sender};
use futures::channel::oneshot::channel as oneshot_channel;
use futures::select;
use futures::SinkExt;

use chromiumoxide_cdp::cdp::browser_protocol::network::{Cookie, CookieParam};
use chromiumoxide_cdp::cdp::browser_protocol::storage::{
    ClearCookiesParams, GetCookiesParams, SetCookiesParams,
};
use chromiumoxide_cdp::cdp::browser_protocol::target::{
    CreateBrowserContextParams, CreateTargetParams, DisposeBrowserContextParams, TargetId,
    TargetInfo,
};
use chromiumoxide_cdp::cdp::{CdpEventMessage, IntoEventKind};
use chromiumoxide_types::*;

use crate::async_process::{self, Child, ExitStatus, Stdio};
use crate::cmd::{to_command_response, CommandMessage};
use crate::conn::Connection;
use crate::detection::{self, DetectionOptions};
use crate::error::{BrowserStderr, CdpError, Result};
use crate::handler::browser::BrowserContext;
use crate::handler::viewport::Viewport;
use crate::handler::{Handler, HandlerConfig, HandlerMessage, REQUEST_TIMEOUT};
use crate::listeners::{EventListenerRequest, EventStream};
use crate::page::Page;
use crate::utils;
use chromiumoxide_cdp::cdp::browser_protocol::browser::{
    BrowserContextId, CloseReturns, GetVersionParams, GetVersionReturns,
};

/// Default `Browser::launch` timeout in MS
pub const LAUNCH_TIMEOUT: u64 = 20_000;

/// A [`Browser`] is created when chromiumoxide connects to a Chromium instance.
#[derive(Debug)]
pub struct Browser {
    /// The `Sender` to send messages to the connection handler that drives the
    /// websocket
    sender: Sender<HandlerMessage>,
    /// How the spawned chromium instance was configured, if any
    config: Option<BrowserConfig>,
    /// The spawned chromium instance
    child: Option<Child>,
    /// The debug web socket url of the chromium instance
    debug_ws_url: String,
    /// The context of the browser
    browser_context: BrowserContext,
}

/// Browser connection information.
#[derive(serde::Deserialize, Debug, Default)]
pub struct BrowserConnection {
    #[serde(rename = "Browser")]
    /// The browser name
    pub browser: String,
    #[serde(rename = "Protocol-Version")]
    /// Browser version
    pub protocol_version: String,
    #[serde(rename = "User-Agent")]
    /// User Agent used by default.
    pub user_agent: String,
    #[serde(rename = "V8-Version")]
    /// The v8 engine version
    pub v8_version: String,
    #[serde(rename = "WebKit-Version")]
    /// Webkit version
    pub webkit_version: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    /// Remote debugging address
    pub web_socket_debugger_url: String,
}

impl Browser {
    /// Connect to an already running chromium instance via the given URL.
    ///
    /// If the URL is a http(s) URL, it will first attempt to retrieve the Websocket URL from the `json/version` endpoint.
    pub async fn connect(url: impl Into<String>) -> Result<(Self, Handler)> {
        Self::connect_with_config(url, HandlerConfig::default()).await
    }

    // Connect to an already running chromium instance with a given `HandlerConfig`.
    ///
    /// If the URL is a http URL, it will first attempt to retrieve the Websocket URL from the `json/version` endpoint.
    pub async fn connect_with_config(
        url: impl Into<String>,
        config: HandlerConfig,
    ) -> Result<(Self, Handler)> {
        let mut debug_ws_url = url.into();

        if debug_ws_url.starts_with("http") {
            match reqwest::Client::new()
                .get(
                    if debug_ws_url.ends_with("/json/version")
                        || debug_ws_url.ends_with("/json/version/")
                    {
                        debug_ws_url.clone()
                    } else {
                        format!(
                            "{}{}json/version",
                            &debug_ws_url,
                            if debug_ws_url.ends_with('/') { "" } else { "/" }
                        )
                    },
                )
                .header("content-type", "application/json")
                .send()
                .await
            {
                Ok(req) => {
                    let socketaddr = req.remote_addr().unwrap();
                    let connection: BrowserConnection =
                        serde_json::from_slice(&req.bytes().await.unwrap_or_default())
                            .unwrap_or_default();

                    if !connection.web_socket_debugger_url.is_empty() {
                        // prevent proxy interfaces from returning local ips to connect to the exact machine
                        debug_ws_url = connection
                            .web_socket_debugger_url
                            .replace("127.0.0.1", &socketaddr.ip().to_string());
                    }
                }
                Err(_) => return Err(CdpError::NoResponse),
            }
        }

        let conn = Connection::<CdpEventMessage>::connect(&debug_ws_url).await?;

        let (tx, rx) = channel(1);

        let fut = Handler::new(conn, rx, config);
        let browser_context = fut.default_browser_context().clone();

        let browser = Self {
            sender: tx,
            config: None,
            child: None,
            debug_ws_url,
            browser_context,
        };
        Ok((browser, fut))
    }

    /// Launches a new instance of `chromium` in the background and attaches to
    /// its debug web socket.
    ///
    /// This fails when no chromium executable could be detected.
    ///
    /// This fails if no web socket url could be detected from the child
    /// processes stderr for more than the configured `launch_timeout`
    /// (20 seconds by default).
    pub async fn launch(mut config: BrowserConfig) -> Result<(Self, Handler)> {
        // Canonalize paths to reduce issues with sandboxing
        config.executable = utils::canonicalize(&config.executable).await?;

        // Launch a new chromium instance
        let mut child = config.launch()?;

        /// Faillible initialization to run once the child process is created.
        ///
        /// All faillible calls must be executed inside this function. This ensures that all
        /// errors are caught and that the child process is properly cleaned-up.
        async fn with_child(
            config: &BrowserConfig,
            child: &mut Child,
        ) -> Result<(String, Connection<CdpEventMessage>)> {
            let dur = config.launch_timeout;
            cfg_if::cfg_if! {
                if #[cfg(feature = "async-std-runtime")] {
                    let timeout_fut = Box::pin(async_std::task::sleep(dur));
                } else if #[cfg(feature = "tokio-runtime")] {
                    let timeout_fut = Box::pin(tokio::time::sleep(dur));
                } else {
                    panic!("missing chromiumoxide runtime: enable `async-std-runtime` or `tokio-runtime`")
                }
            };
            // extract the ws:
            let debug_ws_url = ws_url_from_output(child, timeout_fut).await?;
            let conn = Connection::<CdpEventMessage>::connect(&debug_ws_url).await?;
            Ok((debug_ws_url, conn))
        }

        let (debug_ws_url, conn) = match with_child(&config, &mut child).await {
            Ok(conn) => conn,
            Err(e) => {
                // An initialization error occurred, clean up the process
                if let Ok(Some(_)) = child.try_wait() {
                    // already exited, do nothing, may happen if the browser crashed
                } else {
                    // the process is still alive, kill it and wait for exit (avoid zombie processes)
                    child.kill().await.expect("`Browser::launch` failed but could not clean-up the child process (`kill`)");
                    child.wait().await.expect("`Browser::launch` failed but could not clean-up the child process (`wait`)");
                }
                return Err(e);
            }
        };

        // Only infaillible calls are allowed after this point to avoid clean-up issues with the
        // child process.

        let (tx, rx) = channel(1);

        let handler_config = HandlerConfig {
            ignore_https_errors: config.ignore_https_errors,
            viewport: config.viewport.clone(),
            context_ids: Vec::new(),
            request_timeout: config.request_timeout,
            request_intercept: config.request_intercept,
            cache_enabled: config.cache_enabled,
        };

        let fut = Handler::new(conn, rx, handler_config);
        let browser_context = fut.default_browser_context().clone();

        let browser = Self {
            sender: tx,
            config: Some(config),
            child: Some(child),
            debug_ws_url,
            browser_context,
        };

        Ok((browser, fut))
    }

    /// Request to fetch all existing browser targets.
    ///
    /// By default, only targets launched after the browser connection are tracked
    /// when connecting to a existing browser instance with the devtools websocket url
    /// This function fetches existing targets on the browser and adds them as pages internally
    ///
    /// The pages are not guaranteed to be ready as soon as the function returns
    /// You should wait a few millis if you need to use a page
    /// Returns [TargetInfo]
    pub async fn fetch_targets(&mut self) -> Result<Vec<TargetInfo>> {
        let (tx, rx) = oneshot_channel();

        self.sender
            .clone()
            .send(HandlerMessage::FetchTargets(tx))
            .await?;

        rx.await?
    }

    /// Request for the browser to close completely.
    ///
    /// If the browser was spawned by [`Browser::launch`], it is recommended to wait for the
    /// spawned instance exit, to avoid "zombie" processes ([`Browser::wait`],
    /// [`Browser::wait_sync`], [`Browser::try_wait`]).
    /// [`Browser::drop`] waits automatically if needed.
    pub async fn close(&mut self) -> Result<CloseReturns> {
        let (tx, rx) = oneshot_channel();

        self.sender
            .clone()
            .send(HandlerMessage::CloseBrowser(tx))
            .await?;

        rx.await?
    }

    /// Asynchronously wait for the spawned chromium instance to exit completely.
    ///
    /// The instance is spawned by [`Browser::launch`]. `wait` is usually called after
    /// [`Browser::close`]. You can call this explicitly to collect the process and avoid
    /// "zombie" processes.
    ///
    /// This call has no effect if this [`Browser`] did not spawn any chromium instance (e.g.
    /// connected to an existing browser through [`Browser::connect`])
    pub async fn wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(child) = self.child.as_mut() {
            Ok(Some(child.wait().await?))
        } else {
            Ok(None)
        }
    }

    /// If the spawned chromium instance has completely exited, wait for it.
    ///
    /// The instance is spawned by [`Browser::launch`]. `try_wait` is usually called after
    /// [`Browser::close`]. You can call this explicitly to collect the process and avoid
    /// "zombie" processes.
    ///
    /// This call has no effect if this [`Browser`] did not spawn any chromium instance (e.g.
    /// connected to an existing browser through [`Browser::connect`])
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(child) = self.child.as_mut() {
            child.try_wait()
        } else {
            Ok(None)
        }
    }

    /// Get the spawned chromium instance
    ///
    /// The instance is spawned by [`Browser::launch`]. The result is a [`async_process::Child`]
    /// value. It acts as a compat wrapper for an `async-std` or `tokio` child process.
    ///
    /// You may use [`async_process::Child::as_mut_inner`] to retrieve the concrete implementation
    /// for the selected runtime.
    ///
    /// This call has no effect if this [`Browser`] did not spawn any chromium instance (e.g.
    /// connected to an existing browser through [`Browser::connect`])
    pub fn get_mut_child(&mut self) -> Option<&mut Child> {
        self.child.as_mut()
    }

    /// Forcibly kill the spawned chromium instance
    ///
    /// The instance is spawned by [`Browser::launch`]. `kill` will automatically wait for the child
    /// process to exit to avoid "zombie" processes.
    ///
    /// This method is provided to help if the browser does not close by itself. You should prefer
    /// to use [`Browser::close`].
    ///
    /// This call has no effect if this [`Browser`] did not spawn any chromium instance (e.g.
    /// connected to an existing browser through [`Browser::connect`])
    pub async fn kill(&mut self) -> Option<io::Result<()>> {
        match self.child.as_mut() {
            Some(child) => Some(child.kill().await),
            None => None,
        }
    }

    /// If not launched as incognito this creates a new incognito browser
    /// context. After that this browser exists within the incognito session.
    /// New pages created while being in incognito mode will also run in the
    /// incognito context. Incognito contexts won't share cookies/cache with
    /// other browser contexts.
    pub async fn start_incognito_context(&mut self) -> Result<&mut Self> {
        if !self.is_incognito_configured() {
            let browser_context_id = self
                .create_browser_context(CreateBrowserContextParams::default())
                .await?;
            self.browser_context = BrowserContext::from(browser_context_id);
            self.sender
                .clone()
                .send(HandlerMessage::InsertContext(self.browser_context.clone()))
                .await?;
        }

        Ok(self)
    }

    /// If a incognito session was created with
    /// `Browser::start_incognito_context` this disposes this context.
    ///
    /// # Note This will also dispose all pages that were running within the
    /// incognito context.
    pub async fn quit_incognito_context(&mut self) -> Result<&mut Self> {
        if let Some(id) = self.browser_context.take() {
            self.dispose_browser_context(id.clone()).await?;
            self.sender
                .clone()
                .send(HandlerMessage::DisposeContext(BrowserContext::from(id)))
                .await?;
        }
        Ok(self)
    }

    /// Whether incognito mode was configured from the start
    fn is_incognito_configured(&self) -> bool {
        self.config
            .as_ref()
            .map(|c| c.incognito)
            .unwrap_or_default()
    }

    /// Returns the address of the websocket this browser is attached to
    pub fn websocket_address(&self) -> &String {
        &self.debug_ws_url
    }

    /// Whether the BrowserContext is incognito.
    pub fn is_incognito(&self) -> bool {
        self.is_incognito_configured() || self.browser_context.is_incognito()
    }

    /// The config of the spawned chromium instance if any.
    pub fn config(&self) -> Option<&BrowserConfig> {
        self.config.as_ref()
    }

    /// Create a new browser page
    pub async fn new_page(&self, params: impl Into<CreateTargetParams>) -> Result<Page> {
        let (tx, rx) = oneshot_channel();
        let mut params = params.into();
        if let Some(id) = self.browser_context.id() {
            if params.browser_context_id.is_none() {
                params.browser_context_id = Some(id.clone());
            }
        }

        self.sender
            .clone()
            .send(HandlerMessage::CreatePage(params, tx))
            .await?;

        rx.await?
    }

    /// Version information about the browser
    pub async fn version(&self) -> Result<GetVersionReturns> {
        Ok(self.execute(GetVersionParams::default()).await?.result)
    }

    /// Returns the user agent of the browser
    pub async fn user_agent(&self) -> Result<String> {
        Ok(self.version().await?.user_agent)
    }

    /// Call a browser method.
    pub async fn execute<T: Command>(&self, cmd: T) -> Result<CommandResponse<T::Response>> {
        let (tx, rx) = oneshot_channel();
        let method = cmd.identifier();
        let msg = CommandMessage::new(cmd, tx)?;

        self.sender
            .clone()
            .send(HandlerMessage::Command(msg))
            .await?;
        let resp = rx.await??;
        to_command_response::<T>(resp, method)
    }

    /// Return all of the pages of the browser
    pub async fn pages(&self) -> Result<Vec<Page>> {
        let (tx, rx) = oneshot_channel();
        self.sender
            .clone()
            .send(HandlerMessage::GetPages(tx))
            .await?;
        Ok(rx.await?)
    }

    /// Return page of given target_id
    pub async fn get_page(&self, target_id: TargetId) -> Result<Page> {
        let (tx, rx) = oneshot_channel();
        self.sender
            .clone()
            .send(HandlerMessage::GetPage(target_id, tx))
            .await?;
        rx.await?.ok_or(CdpError::NotFound)
    }

    /// Set listener for browser event
    pub async fn event_listener<T: IntoEventKind>(&self) -> Result<EventStream<T>> {
        let (tx, rx) = unbounded();
        self.sender
            .clone()
            .send(HandlerMessage::AddEventListener(
                EventListenerRequest::new::<T>(tx),
            ))
            .await?;

        Ok(EventStream::new(rx))
    }

    /// Creates a new empty browser context.
    pub async fn create_browser_context(
        &self,
        params: CreateBrowserContextParams,
    ) -> Result<BrowserContextId> {
        let response = self.execute(params).await?;
        Ok(response.result.browser_context_id)
    }

    /// Deletes a browser context.
    pub async fn dispose_browser_context(
        &self,
        browser_context_id: impl Into<BrowserContextId>,
    ) -> Result<()> {
        self.execute(DisposeBrowserContextParams::new(browser_context_id))
            .await?;

        Ok(())
    }

    /// Clears cookies.
    pub async fn clear_cookies(&self) -> Result<()> {
        self.execute(ClearCookiesParams::default()).await?;
        Ok(())
    }

    /// Returns all browser cookies.
    pub async fn get_cookies(&self) -> Result<Vec<Cookie>> {
        Ok(self
            .execute(GetCookiesParams::default())
            .await?
            .result
            .cookies)
    }

    /// Sets given cookies.
    pub async fn set_cookies(&self, mut cookies: Vec<CookieParam>) -> Result<&Self> {
        for cookie in &mut cookies {
            if let Some(url) = cookie.url.as_ref() {
                crate::page::validate_cookie_url(url)?;
            }
        }

        self.execute(SetCookiesParams::new(cookies)).await?;
        Ok(self)
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(_)) = child.try_wait() {
                // Already exited, do nothing. Usually occurs after using the method close or kill.
            } else {
                // We set the `kill_on_drop` property for the child process, so no need to explicitely
                // kill it here. It can't really be done anyway since the method is async.
                //
                // On Unix, the process will be reaped in the background by the runtime automatically
                // so it won't leave any resources locked. It is, however, a better practice for the user to
                // do it himself since the runtime doesn't provide garantees as to when the reap occurs, so we
                // warn him here.
                tracing::warn!("Browser was not closed manually, it will be killed automatically in the background");
            }
        }
    }
}

/// Resolve devtools WebSocket URL from the provided browser process
///
/// If an error occurs, it returns the browser's stderr output.
///
/// The URL resolution fails if:
/// - [`CdpError::LaunchTimeout`]: `timeout_fut` completes, this corresponds to a timeout
/// - [`CdpError::LaunchExit`]: the browser process exits (or is killed)
/// - [`CdpError::LaunchIo`]: an input/output error occurs when await the process exit or reading
///   the browser's stderr: end of stream, invalid UTF-8, other
async fn ws_url_from_output(
    child_process: &mut Child,
    timeout_fut: impl Future<Output = ()> + Unpin,
) -> Result<String> {
    use futures::{AsyncBufReadExt, FutureExt};
    let mut timeout_fut = timeout_fut.fuse();
    let stderr = child_process.stderr.take().expect("no stderror");
    let mut stderr_bytes = Vec::<u8>::new();
    let mut exit_status_fut = Box::pin(child_process.wait()).fuse();
    let mut buf = futures::io::BufReader::new(stderr);
    loop {
        select! {
            _ = timeout_fut => return Err(CdpError::LaunchTimeout(BrowserStderr::new(stderr_bytes))),
            exit_status = exit_status_fut => {
                return Err(match exit_status {
                    Err(e) => CdpError::LaunchIo(e, BrowserStderr::new(stderr_bytes)),
                    Ok(exit_status) => CdpError::LaunchExit(exit_status, BrowserStderr::new(stderr_bytes)),
                })
            },
            read_res = buf.read_until(b'\n', &mut stderr_bytes).fuse() => {
                match read_res {
                    Err(e) => return Err(CdpError::LaunchIo(e, BrowserStderr::new(stderr_bytes))),
                    Ok(byte_count) => {
                        if byte_count == 0 {
                            let e = io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected end of stream");
                            return Err(CdpError::LaunchIo(e, BrowserStderr::new(stderr_bytes)));
                        }
                        let start_offset = stderr_bytes.len() - byte_count;
                        let new_bytes = &stderr_bytes[start_offset..];
                        match std::str::from_utf8(new_bytes) {
                            Err(_) => {
                                let e = io::Error::new(io::ErrorKind::InvalidData, "stream did not contain valid UTF-8");
                                return Err(CdpError::LaunchIo(e, BrowserStderr::new(stderr_bytes)));
                            }
                            Ok(line) => {
                                if let Some((_, ws)) = line.rsplit_once("listening on ") {
                                    if ws.starts_with("ws") && ws.contains("devtools/browser") {
                                        return Ok(ws.trim().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HeadlessMode {
    /// The "headful" mode.
    False,
    /// The old headless mode.
    #[default]
    True,
    /// The new headless mode. See also: https://developer.chrome.com/docs/chromium/new-headless
    New,
}

#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Determines whether to run headless version of the browser. Defaults to
    /// true.
    headless: HeadlessMode,
    /// Determines whether to run the browser with a sandbox.
    sandbox: bool,
    /// Launch the browser with a specific window width and height.
    window_size: Option<(u32, u32)>,
    /// Launch the browser with a specific debugging port.
    port: u16,
    /// Path for Chrome or Chromium.
    ///
    /// If unspecified, the create will try to automatically detect a suitable
    /// binary.
    executable: std::path::PathBuf,

    /// A list of Chrome extensions to load.
    ///
    /// An extension should be a path to a folder containing the extension code.
    /// CRX files cannot be used directly and must be first extracted.
    ///
    /// Note that Chrome does not support loading extensions in headless-mode.
    /// See https://bugs.chromium.org/p/chromium/issues/detail?id=706008#c5
    extensions: Vec<String>,

    /// Environment variables to set for the Chromium process.
    /// Passes value through to std::process::Command::envs.
    pub process_envs: Option<HashMap<String, String>>,

    /// Data dir for user data
    pub user_data_dir: Option<PathBuf>,

    /// Whether to launch the `Browser` in incognito mode
    incognito: bool,

    /// Timeout duration for `Browser::launch`.
    launch_timeout: Duration,

    /// Ignore https errors, default is true
    ignore_https_errors: bool,
    viewport: Option<Viewport>,
    /// The duration after a request with no response should time out
    request_timeout: Duration,

    /// Additional command line arguments to pass to the browser instance.
    args: Vec<String>,

    /// Whether to disable DEFAULT_ARGS or not, default is false
    disable_default_args: bool,

    /// Whether to enable request interception
    pub request_intercept: bool,

    /// Whether to enable cache
    pub cache_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct BrowserConfigBuilder {
    headless: HeadlessMode,
    sandbox: bool,
    window_size: Option<(u32, u32)>,
    port: u16,
    executable: Option<PathBuf>,
    executation_detection: DetectionOptions,
    extensions: Vec<String>,
    process_envs: Option<HashMap<String, String>>,
    user_data_dir: Option<PathBuf>,
    incognito: bool,
    launch_timeout: Duration,
    ignore_https_errors: bool,
    viewport: Option<Viewport>,
    request_timeout: Duration,
    args: Vec<String>,
    disable_default_args: bool,
    request_intercept: bool,
    cache_enabled: bool,
}

impl BrowserConfig {
    pub fn builder() -> BrowserConfigBuilder {
        BrowserConfigBuilder::default()
    }

    pub fn with_executable(path: impl AsRef<Path>) -> Self {
        Self::builder().chrome_executable(path).build().unwrap()
    }
}

impl Default for BrowserConfigBuilder {
    fn default() -> Self {
        Self {
            headless: HeadlessMode::True,
            sandbox: true,
            window_size: None,
            port: 0,
            executable: None,
            executation_detection: DetectionOptions::default(),
            extensions: Vec::new(),
            process_envs: None,
            user_data_dir: None,
            incognito: false,
            launch_timeout: Duration::from_millis(LAUNCH_TIMEOUT),
            ignore_https_errors: true,
            viewport: Some(Default::default()),
            request_timeout: Duration::from_millis(REQUEST_TIMEOUT),
            args: Vec::new(),
            disable_default_args: false,
            request_intercept: false,
            cache_enabled: true,
        }
    }
}

impl BrowserConfigBuilder {
    pub fn window_size(mut self, width: u32, height: u32) -> Self {
        self.window_size = Some((width, height));
        self
    }

    pub fn no_sandbox(mut self) -> Self {
        self.sandbox = false;
        self
    }

    pub fn with_head(mut self) -> Self {
        self.headless = HeadlessMode::False;
        self
    }

    pub fn new_headless_mode(mut self) -> Self {
        self.headless = HeadlessMode::New;
        self
    }

    pub fn headless_mode(mut self, mode: HeadlessMode) -> Self {
        self.headless = mode;
        self
    }

    pub fn incognito(mut self) -> Self {
        self.incognito = true;
        self
    }

    pub fn respect_https_errors(mut self) -> Self {
        self.ignore_https_errors = false;
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn launch_timeout(mut self, timeout: Duration) -> Self {
        self.launch_timeout = timeout;
        self
    }

    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Configures the viewport of the browser, which defaults to `800x600`.
    /// `None` disables viewport emulation (i.e., it uses the browsers default
    /// configuration, which fills the available space. This is similar to what
    /// Playwright does when you provide `null` as the value of its `viewport`
    /// option).
    pub fn viewport(mut self, viewport: impl Into<Option<Viewport>>) -> Self {
        self.viewport = viewport.into();
        self
    }

    pub fn user_data_dir(mut self, data_dir: impl AsRef<Path>) -> Self {
        self.user_data_dir = Some(data_dir.as_ref().to_path_buf());
        self
    }

    pub fn chrome_executable(mut self, path: impl AsRef<Path>) -> Self {
        self.executable = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn chrome_detection(mut self, options: DetectionOptions) -> Self {
        self.executation_detection = options;
        self
    }

    pub fn extension(mut self, extension: impl Into<String>) -> Self {
        self.extensions.push(extension.into());
        self
    }

    pub fn extensions<I, S>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for ext in extensions {
            self.extensions.push(ext.into());
        }
        self
    }

    pub fn env(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.process_envs
            .get_or_insert(HashMap::new())
            .insert(key.into(), val.into());
        self
    }

    pub fn envs<I, K, V>(mut self, envs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.process_envs
            .get_or_insert(HashMap::new())
            .extend(envs.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for arg in args {
            self.args.push(arg.into());
        }
        self
    }

    pub fn disable_default_args(mut self) -> Self {
        self.disable_default_args = true;
        self
    }

    pub fn enable_request_intercept(mut self) -> Self {
        self.request_intercept = true;
        self
    }

    pub fn disable_request_intercept(mut self) -> Self {
        self.request_intercept = false;
        self
    }

    pub fn enable_cache(mut self) -> Self {
        self.cache_enabled = true;
        self
    }

    pub fn disable_cache(mut self) -> Self {
        self.cache_enabled = false;
        self
    }

    pub fn build(self) -> std::result::Result<BrowserConfig, String> {
        let executable = if let Some(e) = self.executable {
            e
        } else {
            detection::default_executable(self.executation_detection)?
        };

        Ok(BrowserConfig {
            headless: self.headless,
            sandbox: self.sandbox,
            window_size: self.window_size,
            port: self.port,
            executable,
            extensions: self.extensions,
            process_envs: self.process_envs,
            user_data_dir: self.user_data_dir,
            incognito: self.incognito,
            launch_timeout: self.launch_timeout,
            ignore_https_errors: self.ignore_https_errors,
            viewport: self.viewport,
            request_timeout: self.request_timeout,
            args: self.args,
            disable_default_args: self.disable_default_args,
            request_intercept: self.request_intercept,
            cache_enabled: self.cache_enabled,
        })
    }
}

impl BrowserConfig {
    pub fn launch(&self) -> io::Result<Child> {
        let mut cmd = async_process::Command::new(&self.executable);

        if self.disable_default_args {
            cmd.args(&self.args);
        } else {
            cmd.args(DEFAULT_ARGS).args(&self.args);
        }

        if !self
            .args
            .iter()
            .any(|arg| arg.contains("--remote-debugging-port="))
        {
            cmd.arg(format!("--remote-debugging-port={}", self.port));
        }

        cmd.args(
            self.extensions
                .iter()
                .map(|e| format!("--load-extension={e}")),
        );

        if let Some(ref user_data) = self.user_data_dir {
            cmd.arg(format!("--user-data-dir={}", user_data.display()));
        } else {
            // If the user did not specify a data directory, this would default to the systems default
            // data directory. In most cases, we would rather have a fresh instance of Chromium. Specify
            // a temp dir just for chromiumoxide instead.
            cmd.arg(format!(
                "--user-data-dir={}",
                std::env::temp_dir().join("chromiumoxide-runner").display()
            ));
        }

        if let Some((width, height)) = self.window_size {
            cmd.arg(format!("--window-size={width},{height}"));
        }

        if !self.sandbox {
            cmd.args(["--no-sandbox", "--disable-setuid-sandbox"]);
        }

        match self.headless {
            HeadlessMode::False => (),
            HeadlessMode::True => {
                cmd.args(["--headless", "--hide-scrollbars", "--mute-audio"]);
            }
            HeadlessMode::New => {
                cmd.args(["--headless=new", "--hide-scrollbars", "--mute-audio"]);
            }
        }

        if self.incognito {
            cmd.arg("--incognito");
        }

        if let Some(ref envs) = self.process_envs {
            cmd.envs(envs);
        }
        cmd.stderr(Stdio::piped()).spawn()
    }
}

/// Returns the path to Chrome's executable.
///
/// If the `CHROME` environment variable is set, `default_executable` will
/// use it as the default path. Otherwise, the filenames `google-chrome-stable`
/// `chromium`, `chromium-browser`, `chrome` and `chrome-browser` are
/// searched for in standard places. If that fails,
/// `/Applications/Google Chrome.app/...` (on MacOS) or the registry (on
/// Windows) is consulted. If all of the above fail, an error is returned.
#[deprecated(note = "Use detection::default_executable instead")]
pub fn default_executable() -> Result<std::path::PathBuf, String> {
    let options = DetectionOptions {
        msedge: false,
        unstable: false,
    };
    detection::default_executable(options)
}

/// These are passed to the Chrome binary by default.
/// Via https://github.com/puppeteer/puppeteer/blob/4846b8723cf20d3551c0d755df394cc5e0c82a94/src/node/Launcher.ts#L157
static DEFAULT_ARGS: [&str; 25] = [
    "--disable-background-networking",
    "--enable-features=NetworkService,NetworkServiceInProcess",
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
    "--disable-breakpad",
    "--disable-client-side-phishing-detection",
    "--disable-component-extensions-with-background-pages",
    "--disable-default-apps",
    "--disable-dev-shm-usage",
    "--disable-extensions",
    "--disable-features=TranslateUI",
    "--disable-hang-monitor",
    "--disable-ipc-flooding-protection",
    "--disable-popup-blocking",
    "--disable-prompt-on-repost",
    "--disable-renderer-backgrounding",
    "--disable-sync",
    "--force-color-profile=srgb",
    "--metrics-recording-only",
    "--no-first-run",
    "--enable-automation",
    "--password-store=basic",
    "--use-mock-keychain",
    "--enable-blink-features=IdleDetection",
    "--lang=en_US",
];
