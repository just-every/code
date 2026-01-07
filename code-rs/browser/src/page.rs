use crate::BrowserError;
use crate::Result;
use crate::config::BrowserConfig;
use crate::config::ImageFormat;
use crate::config::ViewportConfig;
use crate::config::WaitStrategy;
use chromiumoxide::cdp::browser_protocol::input::DispatchKeyEventParams;
use chromiumoxide::cdp::browser_protocol::input::DispatchKeyEventType;
use chromiumoxide::cdp::browser_protocol::input::DispatchMouseEventParams;
use chromiumoxide::cdp::browser_protocol::input::DispatchMouseEventType;
// Import MouseButton (New)
use chromiumoxide::cdp::browser_protocol::input::MouseButton;
// Import AddScriptToEvaluateOnNewDocumentParams (New)
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams;
use chromiumoxide::page::Page as CdpPage;
use chromiumoxide::cdp::js_protocol::runtime as cdp_runtime;
use chromiumoxide::cdp::browser_protocol::log as cdp_log;
use futures::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
// Use Mutex for cursor state (New)
use tokio::sync::Mutex;
use tracing::debug;
use tracing::info;
use tracing::warn;

// Externalized virtual cursor script (editable JS)
const VIRTUAL_CURSOR_JS: &str = include_str!("js/virtual_cursor.js");

// Define CursorState struct (New)
#[derive(Debug, Clone)]
pub struct CursorState {
    pub x: f64,
    pub y: f64,
    // Include button state, mirroring the TS implementation
    pub button: MouseButton,
    // Track whether mouse button is currently pressed
    pub is_mouse_down: bool,
}

pub struct Page {
    cdp_page: Arc<CdpPage>,
    config: BrowserConfig,
    current_url: Arc<RwLock<Option<String>>>,
    // Add cursor state tracking (New)
    cursor_state: Arc<Mutex<CursorState>>,
    // Buffer for CDP-captured console logs
    console_logs: Arc<Mutex<Vec<serde_json::Value>>>,
    // Screenshot path preflight cache:
    // - We strongly prefer compositor captures via from_surface(false) to avoid visible flashes in the
    //   user's real Chrome window. However, that path can be flaky or unavailable when the window is not
    //   visible/minimized. A tiny 8×8 probe (guarded by a ~350ms timeout) predicts viability and is cached
    //   for ~5 seconds to avoid repeated probes while navigating.
    // - IMPORTANT: This cache and probe logic protect both UX (no flash on visible windows) and reliability
    //   (preventing repeated long timeouts when minimized). If you change this, ensure visible windows never
    //   start with from_surface(true), and keep a short/cheap probe for hidden/minimized states.
    preflight_cache: Arc<Mutex<Option<(Instant, bool)>>>,
}

impl Page {
    pub fn new(cdp_page: CdpPage, config: BrowserConfig) -> Self {
        // Initialize cursor position (Updated)
        let initial_cursor = CursorState {
            x: (config.viewport.width as f64 / 2.0).floor(),
            y: (config.viewport.height as f64 / 4.0).floor(),
            button: MouseButton::None,
            is_mouse_down: false,
        };

        let page = Self {
            cdp_page: Arc::new(cdp_page),
            config,
            current_url: Arc::new(RwLock::new(None)),
            cursor_state: Arc::new(Mutex::new(initial_cursor)),
            preflight_cache: Arc::new(Mutex::new(None)),
            console_logs: Arc::new(Mutex::new(Vec::new())),
        };

        // Register a unified bootstrap (runs on every new document):
        //  - Blocks _blank/tab opens
        //  - Installs minimal virtual cursor early
        //  - Hooks SPA history to signal route changes
        let cdp_page_boot = page.cdp_page.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::inject_bootstrap_script(&cdp_page_boot).await {
                warn!("Failed to inject unified bootstrap script: {}", e);
            } else {
                debug!("Unified bootstrap script registered for new documents");
            }
        });

        // Enable CDP Runtime/Log and start capturing console events into an internal buffer.
        // This complements the JS hook and works even if the page overwrites console later.
        let cdp_page_events = page.cdp_page.clone();
        let logs_buf = page.console_logs.clone();
        tokio::spawn(async move {
            // Best-effort enable; ignore failures silently to avoid breaking page creation.
            let _ = cdp_page_events.execute(cdp_runtime::EnableParams::default()).await;
            let _ = cdp_page_events.execute(cdp_log::EnableParams::default()).await;

            // Listen for Runtime.consoleAPICalled
            if let Ok(mut stream) = cdp_page_events
                .event_listener::<cdp_runtime::EventConsoleApiCalled>()
                .await
            {
                while let Some(evt) = stream.next().await {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i128)
                        .unwrap_or(0);
                    // Join args into a readable string; also keep raw values
                    let text = match serde_json::to_string(&evt.args) {
                        Ok(s) => s,
                        Err(_) => String::new(),
                    };
                    let item = serde_json::json!({
                        "ts_unix_ms": ts,
                        "level": format!("{:?}", evt.r#type),
                        "message": text,
                        "source": "cdp:runtime"
                    });
                    let mut buf = logs_buf.lock().await;
                    buf.push(item);
                    if buf.len() > 2000 { buf.remove(0); }
                }
            }

            // Also listen for Log.entryAdded (browser-side logs)
            if let Ok(mut stream) = cdp_page_events
                .event_listener::<cdp_log::EventEntryAdded>()
                .await
            {
                while let Some(evt) = stream.next().await {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i128)
                        .unwrap_or(0);
                    let entry = &evt.entry;
                    let item = serde_json::json!({
                        "ts_unix_ms": ts,
                        "level": format!("{:?}", entry.level),
                        "message": entry.text,
                        "source": "cdp:log",
                        "url": entry.url,
                        "line": entry.line_number
                    });
                    let mut buf = logs_buf.lock().await;
                    buf.push(item);
                    if buf.len() > 2000 { buf.remove(0); }
                }
            }
        });

        page
    }

    /// Ensure the virtual cursor is present; inject if missing, then update to current position.
    async fn ensure_virtual_cursor(&self) -> Result<bool> {
        // Desired runtime version of the virtual cursor script
        let desired_version: i32 = 11;
        // Quick existence check
        // Check existence and version
        let status = self
            .cdp_page
            .evaluate(format!(
                r#"(function(v) {{
                      if (typeof window.__vc === 'undefined') return 'missing';
                      try {{
                        var cur = window.__vc.__version|0;
                        if (!cur || cur !== v) {{
                          if (window.__vc && typeof window.__vc.destroy === 'function') try {{ window.__vc.destroy(); }} catch (e) {{}}
                          return 'reinstall';
                        }}
                        return 'ok';
                      }} catch (e) {{ return 'reinstall'; }}
                }})({})"#,
                desired_version
            ))
            .await
            .ok()
            .and_then(|r| r.value().and_then(|v| v.as_str().map(|s| s.to_string())))
            .unwrap_or_else(|| "missing".to_string());

        if status != "ok" {
            // Inject if missing
            if let Err(e) = self.inject_virtual_cursor().await {
                warn!("Failed to inject virtual cursor: {}", e);
                return Err(e);
            }
            return Ok(true);
        }

        Ok(false)
    }

    /// (NEW) Injects the script to prevent new tabs from opening and redirect them to the current tab.
    #[allow(dead_code)]
    async fn inject_tab_interception_script(cdp_page: &Arc<CdpPage>) -> Result<()> {
        // The comprehensive script ported from browser_session.ts
        let script = r#"
            (() => {
                // Hardened window.open override with Proxy
                const originalOpen = window.open;
                const openProxy = new Proxy(originalOpen, {
                    apply(_t, _this, args) {
                        const url = args[0];
                        if (url) location.href = url;
                        return null;
                    }
                });
                // Lock down the property
                Object.defineProperty(window, 'open', {
                    value: openProxy,
                    writable: false,
                    configurable: false
                });

                // Extract URL helper (including specific attributes from TS version)
                const urlFrom = n => n?.href ??
                    n?.getAttribute?.('href') ??
                    n?.getAttribute?.('post-outbound-link') ??
                    n?.dataset?.url ??
                    n?.dataset?.href ?? null;

                // Intercept handler (handles shadow DOM)
                const intercept = e => {
                    const path = e.composedPath?.() ?? [];
                    for (const n of path) {
                        if (!n?.getAttribute) continue;
                        if (n.getAttribute('target') === '_blank') {
                            const url = urlFrom(n);
                            if (url) {
                                e.preventDefault();
                                e.stopImmediatePropagation();
                                location.href = url;
                            }
                            return;
                        }
                    }
                };

                // Attach listeners
                ['pointerdown', 'click', 'auxclick'].forEach(ev =>
                    document.addEventListener(ev, intercept, { capture: true })
                );

                // Handle keyboard navigation
                document.addEventListener('keydown', e => {
                    if ((e.key === 'Enter' || e.key === ' ') &&
                        document.activeElement?.getAttribute?.('target') === '_blank') {
                        e.preventDefault();
                        const url = urlFrom(document.activeElement);
                        if (url) location.href = url;
                    }
                }, { capture: true });

                // Handle form submissions
                document.addEventListener('submit', e => {
                    if (e.target?.target === '_blank') {
                        e.preventDefault();
                        e.target.target = '_self';
                        e.target.submit();
                    }
                }, { capture: true });

                // Helper to attach listeners to shadow roots
                const attach = root =>
                    ['pointerdown', 'click', 'auxclick'].forEach(ev =>
                        root.addEventListener(ev, intercept, { capture: true })
                    );

                // MutationObserver for shadow DOM
                try {
                    const observeTarget = document.documentElement || document;
                    if (observeTarget) {
                        new MutationObserver(muts => {
                            muts.forEach(m =>
                                m.addedNodes.forEach(n => n && n.shadowRoot && attach(n.shadowRoot))
                            );
                        }).observe(observeTarget, { subtree: true, childList: true });
                    }
                } catch (e) {
                    console.warn("BrowserAutomation: Failed to set up MutationObserver for tab blocking", e);
                }
            })();
        "#;

        let params = AddScriptToEvaluateOnNewDocumentParams::new(script);
        cdp_page.execute(params).await?;
        debug!("Tab interception script injected successfully.");
        Ok(())
    }

    /// Injects a unified bootstrap for each new document: tab blocking + cursor bootstrap + SPA hooks
    /// and early console capture so tools like `browser_console` can read logs reliably.
    async fn inject_bootstrap_script(cdp_page: &Arc<CdpPage>) -> Result<()> {
        // This script installs the full virtual cursor on DOM ready for each new document.
        // It also prevents _blank tabs, hooks SPA history changes, and installs
        // console/error capture early so logs accumulate from the start of the page.
        let script = r#"
(function(){
  // 1) Tab blocking: override window.open + intercept target="_blank"
  try {
    const originalOpen = window.open;
    const openProxy = new Proxy(originalOpen, {
      apply(_t, _this, args) {
        const url = args[0];
        if (url) location.href = url;
        return null;
      }
    });
    Object.defineProperty(window, 'open', { value: openProxy, writable: false, configurable: false });

    const urlFrom = n => n?.href ?? n?.getAttribute?.('href') ?? n?.getAttribute?.('post-outbound-link') ?? n?.dataset?.url ?? n?.dataset?.href ?? null;
    const intercept = e => {
      const path = e.composedPath?.() ?? [];
      for (const n of path) {
        if (!n?.getAttribute) continue;
        if (n.getAttribute('target') === '_blank') {
          const url = urlFrom(n);
          if (url) { e.preventDefault(); e.stopImmediatePropagation(); location.href = url; }
          return;
        }
      }
    };
    ['pointerdown','click','auxclick'].forEach(ev => document.addEventListener(ev, intercept, { capture: true }));
    document.addEventListener('keydown', e => {
      if ((e.key === 'Enter' || e.key === ' ') && document.activeElement?.getAttribute?.('target') === '_blank') {
        e.preventDefault(); const url = urlFrom(document.activeElement); if (url) location.href = url;
      }
    }, { capture: true });
    document.addEventListener('submit', e => {
      if (e.target?.target === '_blank') { e.preventDefault(); e.target.target = '_self'; e.target.submit(); }
    }, { capture: true });
    try {
      const observeTarget = document.documentElement || document;
      if (observeTarget) {
        new MutationObserver(muts => muts.forEach(m => m.addedNodes.forEach(n => n && n.shadowRoot && ['pointerdown','click','auxclick'].forEach(ev => n.shadowRoot.addEventListener(ev, intercept, { capture: true })) ))).observe(observeTarget, { subtree: true, childList: true });
      }
    } catch (e) { console.warn('Tab block MO failed', e); }
  } catch (e) { console.warn('Tab blocking failed', e); }

  // 2) SPA history hooks
  try {
    const dispatch = () => {
      try {
        const ev = new Event('codex:locationchange');
        window.dispatchEvent(ev);
        window.__code_last_url = location.href;
      } catch {}
    };
    const push = history.pushState.bind(history);
    const repl = history.replaceState.bind(history);
    history.pushState = function(...a){ const r = push(...a); dispatch(); return r; };
    history.replaceState = function(...a){ const r = repl(...a); dispatch(); return r; };
    window.addEventListener('popstate', dispatch, { passive: true });
    dispatch();
  } catch (e) { console.warn('SPA hook failed', e); }

  // 3) Console capture: install once and persist for the lifetime of the document
  try {
    if (!window.__code_console_logs) {
      window.__code_console_logs = [];
      const push = (level, message) => {
        try {
          window.__code_console_logs.push({ timestamp: new Date().toISOString(), level, message });
          if (window.__code_console_logs.length > 2000) window.__code_console_logs.shift();
        } catch (_) {}
      };

      // Override console methods once
      ['log','warn','error','info','debug'].forEach(function(method) {
        try {
          const orig = console[method];
          console[method] = function() {
            try {
              var args = Array.prototype.slice.call(arguments);
              var msg = args.map(function(a) {
                try {
                  if (a && typeof a === 'object') return JSON.stringify(a);
                  return String(a);
                } catch (_) { return String(a); }
              }).join(' ');
              push(method, msg);
            } catch(_) {}
            if (orig) return orig.apply(console, arguments);
          };
        } catch(_) {}
      });

      // Capture uncaught errors
      window.addEventListener('error', function(e) {
        try {
          var msg = e && e.message ? e.message : 'Script error';
          var stack = e && e.error && e.error.stack ? ('\n' + e.error.stack) : '';
          push('exception', msg + stack);
        } catch(_) {}
      });
      // Capture unhandled promise rejections
      window.addEventListener('unhandledrejection', function(e) {
        try {
          var reason = e && e.reason;
          if (reason && typeof reason === 'object') {
            try { reason = JSON.stringify(reason); } catch(_) {}
          }
          push('unhandledrejection', String(reason));
        } catch(_) {}
      });
    }
  } catch (e) { /* swallow */ }

  // 5) Stealth: reduce headless/automation signals for basic anti-bot checks
  try {
    // webdriver: undefined
    try { Object.defineProperty(Navigator.prototype, 'webdriver', { get: () => undefined }); } catch(_) {}

    // languages
    try {
      const langs = ['en-US','en'];
      Object.defineProperty(Navigator.prototype, 'languages', { get: () => langs.slice() });
      Object.defineProperty(Navigator.prototype, 'language', { get: () => 'en-US' });
    } catch(_) {}

    // plugins & mimeTypes
    try {
      const fakePlugin = { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer', description: 'Portable Document Format' };
      const arrLike = (len) => ({ length: len, item(i){ return this[i]; } });
      const plugins = arrLike(1); plugins[0] = fakePlugin;
      const mimes = arrLike(2); mimes[0] = { type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format' }; mimes[1] = { type: 'application/x-google-chrome-pdf', suffixes: 'pdf', description: 'Portable Document Format' };
      Object.defineProperty(Navigator.prototype, 'plugins', { get: () => plugins });
      Object.defineProperty(Navigator.prototype, 'mimeTypes', { get: () => mimes });
    } catch(_) {}

    // hardwareConcurrency & deviceMemory
    try { Object.defineProperty(Navigator.prototype, 'hardwareConcurrency', { get: () => 8 }); } catch(_) {}
    try { Object.defineProperty(Navigator.prototype, 'deviceMemory', { get: () => 8 }); } catch(_) {}

    // permissions.query
    try {
      const orig = navigator.permissions && navigator.permissions.query ? navigator.permissions.query.bind(navigator.permissions) : null;
      if (orig) {
        navigator.permissions.query = function(p){
          if (p && p.name === 'notifications') { return Promise.resolve({ state: 'granted' }); }
          return orig(p);
        }
      }
    } catch(_) {}

    // WebGL vendor/renderer
    try {
      const spoof = (proto) => {
        const orig = proto.getParameter;
        Object.defineProperty(proto, 'getParameter', { value: function(p){
          const UNMASKED_VENDOR_WEBGL = 0x9245; // WEBGL_debug_renderer_info
          const UNMASKED_RENDERER_WEBGL = 0x9246;
          if (p === UNMASKED_VENDOR_WEBGL) return 'Apple Inc.';
          if (p === UNMASKED_RENDERER_WEBGL) return 'Apple M2';
          return orig.apply(this, arguments);
        }});
      };
      if (window.WebGLRenderingContext) spoof(WebGLRenderingContext.prototype);
      if (window.WebGL2RenderingContext) spoof(WebGL2RenderingContext.prototype);
    } catch(_) {}

    // userAgentData (hints)
    try {
      if (!('userAgentData' in navigator)) {
        Object.defineProperty(Navigator.prototype, 'userAgentData', { get: () => ({
          brands: [ { brand: 'Chromium', version: '128' }, { brand: 'Google Chrome', version: '128' } ],
          mobile: false,
          platform: navigator.platform || 'macOS'
        })});
      }
    } catch(_) {}
  } catch(_) { /* ignore */ }

  // 4) No cursor bootstrap here; full cursor is injected by runtime ensure_virtual_cursor
})();
"#;

        let params = AddScriptToEvaluateOnNewDocumentParams::new(script);
        cdp_page.execute(params).await?;
        Ok(())
    }

    /// Helper function to capture screenshot with retry logic.
    /// Strategy summary (critical to UX and reliability):
    /// - Visible pages: Start with from_surface(false) (no-flash path). If it fails once, retry false quickly.
    ///   Only as a last resort use from_surface(true), because it can flash a visible window.
    /// - Non-visible pages: Use a fast 8×8 preflight (false) to decide. If compositor is unavailable, start
    ///   with from_surface(true) immediately (safe when not visible). Fallbacks stay conservative.
    /// - Final fallback: If two attempts with false fail even while visible, we try true once rather than
    ///   failing entirely. This prevents chronic timeouts; the flash trade-off is acceptable as a last resort.
    /// Do not loosen these guarantees casually; they were tuned to balance reliability and no-flash UX.
    async fn capture_screenshot_with_retry(
        &self,
        params_builder: chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParamsBuilder,
    ) -> Result<chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotReturns> {
        // Determine page visibility once to decide if from_surface(true) is safe/necessary.
        // If this check fails, assume visible to avoid accidentally picking the flashing path.
        let is_visible = {
            let eval = self
                .cdp_page
                .evaluate(
                    "(() => { try { return { hidden: !!document.hidden, vs: String(document.visibilityState||'unknown') }; } catch (e) { return { hidden: null, vs: 'error' }; } })()",
                )
                .await;
            match eval {
                Ok(v) => {
                    let obj = v.value().cloned().unwrap_or(serde_json::Value::Null);
                    let hidden = obj
                        .get("hidden")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false);
                    let vs = obj.get("vs").and_then(|x| x.as_str()).unwrap_or("visible");
                    !(hidden || vs != "visible")
                }
                Err(_) => true, // assume visible to avoid risky from_surface(true)
            }
        };

        // Preflight probe (non-visible only):
        // - A very fast 8×8 clip via from_surface(false) predicts if the compositor path is currently viable.
        // - Only run when page is not visible (no flash risk) and cache the result for ~5s.
        // This avoids long timeouts on minimized windows without reintroducing flash for visible ones.
        let mut prefer_false = true;
        if !is_visible {
            let now = Instant::now();
            {
                let mut cache = self.preflight_cache.lock().await;
                if let Some((ts, ok)) = *cache {
                    if now.duration_since(ts) < Duration::from_secs(5) {
                        prefer_false = ok;
                    } else {
                        *cache = None;
                    }
                }
            }

            if prefer_false {
                let cached = {
                    let cache = self.preflight_cache.lock().await;
                    cache.is_some()
                };
                if !cached {
                    let probe_params = params_builder
                        .clone()
                        .from_surface(false)
                        .capture_beyond_viewport(true)
                        .clip(chromiumoxide::cdp::browser_protocol::page::Viewport {
                            x: 0.0,
                            y: 0.0,
                            width: 8.0,
                            height: 8.0,
                            scale: 1.0,
                        })
                        .build();
                    let probe = tokio::time::timeout(Duration::from_millis(350), self.cdp_page.execute(probe_params)).await;
                    let ok = matches!(probe, Ok(Ok(_)));
                    let mut cache = self.preflight_cache.lock().await;
                    *cache = Some((Instant::now(), ok));
                    prefer_false = ok;
                    if !prefer_false {
                        debug!("Preflight suggests compositor path unavailable; non-visible context will use from_surface(true)");
                    }
                }
            }
        }

        // First attempt policy:
        // - Visible: Always start with from_surface(false) and a short timeout to minimize flash risk.
        // - Not visible: Use preflight outcome; allow from_surface(true) immediately when compositor is unavailable.
        let (first_params, first_timeout, first_is_false) = if is_visible {
            (params_builder.clone().from_surface(false).build(), Duration::from_secs(3), true)
        } else if prefer_false {
            (params_builder.clone().from_surface(false).build(), Duration::from_secs(6), true)
        } else {
            (params_builder.clone().from_surface(true).build(), Duration::from_secs(6), false)
        };
        let first_attempt = tokio::time::timeout(first_timeout, self.cdp_page.execute(first_params)).await;

        match first_attempt {
            Ok(Ok(resp)) => Ok(resp.result),
            Ok(Err(e)) => {
                debug!(
                    "Screenshot first attempt failed (used_false={}): {} (visible={})",
                    first_is_false, e, is_visible
                );
                if !is_visible || !first_is_false {
                    // Non-visible or already tried true path: retry with from_surface(true).
                    // Safe for minimized/hidden windows and avoids repeated long timeouts.
                    let retry_params = params_builder.from_surface(true).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(8),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;

                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(retry_err)) => Err(retry_err.into()),
                        Err(_) => Err(BrowserError::ScreenshotError(
                            "Screenshot retry (from_surface=true) timed out".to_string(),
                        )),
                    }
                } else {
                    // Visible: avoid from_surface(true) if at all possible. Brief wait and retry once with false.
                    tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
                    let retry_params = params_builder.clone().from_surface(false).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(4),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;
                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(_)) => {
                            // Last resort for visible pages: try from_surface(true).
                            // This can flash; we only do it after exhausting the safer path to prevent permanent failures.
                            debug!(
                                "Retry with from_surface(false) failed while visible; attempting from_surface(true) as fallback"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out (final from_surface=true fallback)".to_string(),
                                )),
                            }
                        }
                        Err(_) => {
                            // Timeout on second false attempt; try true once as last resort (may flash)
                            debug!(
                                "Retry with from_surface(false) timed out while visible; attempting from_surface(true) as fallback"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out after retries (from_surface=true fallback)".to_string(),
                                )),
                            }
                        }
                    }
                }
            }
            Err(_) => {
                debug!(
                    "Screenshot first attempt timed out (used_false={}, visible={})",
                    first_is_false, is_visible
                );
                if !is_visible || !first_is_false {
                    // Not visible (safe) or already tried false: try from_surface(true)
                    let retry_params = params_builder.from_surface(true).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(8),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;
                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(e)) => Err(e.into()),
                        Err(_) => Err(BrowserError::ScreenshotError(
                            "Screenshot timed out with from_surface(true)".to_string(),
                        )),
                    }
                } else {
                    // Visible: avoid from_surface(true) if possible; retry quickly with false
                    let retry_params = params_builder.clone().from_surface(false).build();
                    let retry_attempt = tokio::time::timeout(
                        tokio::time::Duration::from_secs(4),
                        self.cdp_page.execute(retry_params),
                    )
                    .await;
                    match retry_attempt {
                        Ok(Ok(resp)) => Ok(resp.result),
                        Ok(Err(_)) => {
                            // Final fallback with from_surface(true) even though visible (see doc rationale)
                            debug!(
                                "Second attempt with from_surface(false) failed while visible; attempting final from_surface(true)"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out after retries (final from_surface=true)".to_string(),
                                )),
                            }
                        }
                        Err(_) => {
                            // Timeout on second false attempt, try true once (final)
                            debug!(
                                "Second attempt with from_surface(false) timed out while visible; attempting final from_surface(true)"
                            );
                            let final_params = params_builder.from_surface(true).build();
                            let final_attempt = tokio::time::timeout(
                                tokio::time::Duration::from_secs(4),
                                self.cdp_page.execute(final_params),
                            )
                            .await;
                            match final_attempt {
                                Ok(Ok(resp)) => Ok(resp.result),
                                Ok(Err(e3)) => Err(e3.into()),
                                Err(_) => Err(BrowserError::ScreenshotError(
                                    "Screenshot timed out after retries (from_surface=true fallback)".to_string(),
                                )),
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns the current page title, if available.
    pub async fn get_title(&self) -> Option<String> {
        self.cdp_page.get_title().await.ok().flatten()
    }

    /// Check and fix viewport scaling issues before taking screenshots
    #[allow(dead_code)]
    async fn check_and_fix_scaling(&self) -> Result<()> {
        // Never touch viewport metrics for external Chrome connections.
        // Changing device metrics on a user's Chrome causes a visible flash
        // and slows down screenshots. We only verify/correct for internally
        // launched Chrome where we control the window.
        if self.config.connect_port.is_some() || self.config.connect_ws.is_some() {
            return Ok(());
        }
        // Check current viewport and scaling
        let check_script = r#"
            (() => {
                const vw = window.innerWidth;
                const vh = window.innerHeight;
                const dpr = window.devicePixelRatio || 1;
                const zoom = Math.round(window.outerWidth / window.innerWidth * 100) / 100;
                
                // Check if viewport matches expected dimensions
                const expectedWidth = %EXPECTED_WIDTH%;
                const expectedHeight = %EXPECTED_HEIGHT%;
                const expectedDpr = %EXPECTED_DPR%;
                
                return {
                    currentWidth: vw,
                    currentHeight: vh,
                    currentDpr: dpr,
                    currentZoom: zoom,
                    expectedWidth: expectedWidth,
                    expectedHeight: expectedHeight,
                    expectedDpr: expectedDpr,
                    // Only correct when there's a meaningful mismatch in size/DPR.
                    // Ignore zoom heuristics which can be noisy on some platforms.
                    needsCorrection: (
                        Math.abs(vw - expectedWidth) > 5 ||
                        Math.abs(vh - expectedHeight) > 5 ||
                        Math.abs(dpr - expectedDpr) > 0.05
                    )
                };
            })()
        "#;

        // Replace placeholders with actual expected values
        let script = check_script
            .replace("%EXPECTED_WIDTH%", &self.config.viewport.width.to_string())
            .replace(
                "%EXPECTED_HEIGHT%",
                &self.config.viewport.height.to_string(),
            )
            .replace(
                "%EXPECTED_DPR%",
                &self.config.viewport.device_scale_factor.to_string(),
            );

        let result = self.cdp_page.evaluate(script).await?;

        if let Some(obj) = result.value() {
            let needs_correction = obj
                .get("needsCorrection")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if needs_correction {
                let current_width = obj
                    .get("currentWidth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let current_height = obj
                    .get("currentHeight")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let current_dpr = obj
                    .get("currentDpr")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);
                let current_zoom = obj
                    .get("currentZoom")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);

                debug!(
                    "Viewport needs correction: {}x{} @ {}x DPR (zoom: {}) -> {}x{} @ {}x DPR",
                    current_width,
                    current_height,
                    current_dpr,
                    current_zoom,
                    self.config.viewport.width,
                    self.config.viewport.height,
                    self.config.viewport.device_scale_factor
                );

                // Use CDP to set the correct viewport metrics
                let params = SetDeviceMetricsOverrideParams::builder()
                    .width(self.config.viewport.width as i64)
                    .height(self.config.viewport.height as i64)
                    .device_scale_factor(self.config.viewport.device_scale_factor)
                    .mobile(self.config.viewport.mobile)
                    .build()
                    .map_err(|e| {
                        BrowserError::CdpError(format!("Failed to build viewport params: {}", e))
                    })?;

                self.cdp_page.execute(params).await?;

                // Avoid aggressive zoom resets to reduce reflow/flash.
                // If internal zoom is off, leave it unless size/DPR corrected above isn't sufficient.

                info!("Viewport scaling corrected");
            }
        }

        Ok(())
    }

    /// (NEW) Injects a virtual cursor element into the page at the current coordinates.
    pub async fn inject_virtual_cursor(&self) -> Result<()> {
        let cursor = self.cursor_state.lock().await.clone();
        let cursor_x = cursor.x;
        let cursor_y = cursor.y;

        // First try the externalized installer for easier iteration.
        // The JS must define `window.__vcInstall(x,y)` and create window.__vc with __version=11.
        let external = format!(
            "{}\n;(()=>{{ try {{ return (window.__vcInstall ? window.__vcInstall : function(x,y){{}})({:.0},{:.0}); }} catch (e) {{ return String(e && e.message || e); }} }})()",
            VIRTUAL_CURSOR_JS,
            cursor_x,
            cursor_y
        );
        if let Ok(res) = self.cdp_page.evaluate(external).await {
            let status = res
                .value()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if status == "ok" {
                return Ok(());
            } else {
                warn!("Virtual cursor injection reported: {}", status);
                return Err(BrowserError::CdpError(format!(
                    "Virtual cursor injection failed: {}",
                    status
                )));
            }
        }
        warn!("Virtual cursor injection failed: no response");
        Err(BrowserError::CdpError("Virtual cursor injection failed: no response".into()))
    }

    /// Ensures an editable element is focused before typing without stealing focus.
    /// Rules:
    /// - If the deeply focused element (piercing shadow DOM and same-origin iframes) is editable, do nothing.
    /// - Otherwise, try to focus the editable element directly under the virtual cursor location.
    /// - Never fall back to any other candidate (prevents unexpected focus steals).
    async fn ensure_editable_focused(&self) -> Result<bool> {
        let cursor = self.cursor_state.lock().await.clone();
        let cursor_x = cursor.x;
        let cursor_y = cursor.y;

        let script = format!(
            r#"
            (function(cursorX, cursorY) {{
                const isEditableInputType = (t) => !/^(checkbox|radio|button|submit|reset|file|image|color|hidden|range)$/i.test(t || '');
                const isEditable = (el) => !!el && (
                    (el.tagName === 'INPUT' && isEditableInputType(el.type)) ||
                    el.tagName === 'TEXTAREA' ||
                    el.isContentEditable === true
                );

                const deepActiveElement = () => {{
                    try {{
                        let ae = document.activeElement;
                        // Pierce shadow roots
                        while (ae && ae.shadowRoot && ae.shadowRoot.activeElement) {{
                            ae = ae.shadowRoot.activeElement;
                        }}
                        // Pierce same-origin iframes
                        while (ae && ae.tagName === 'IFRAME') {{
                            try {{
                                const doc = ae.contentWindow && ae.contentWindow.document;
                                if (!doc) break;
                                let inner = doc.activeElement;
                                if (!inner) break;
                                while (inner && inner.shadowRoot && inner.shadowRoot.activeElement) {{
                                    inner = inner.shadowRoot.activeElement;
                                }}
                                ae = inner;
                            }} catch (_) {{ break; }}
                        }}
                        return ae || null;
                    }} catch (_) {{ return null; }}
                }};

                const deepElementFromPoint = (x, y) => {{
                    // Walk composed tree using elementsFromPoint, then descend into open shadow roots and same-origin iframes
                    const walk = (root, gx, gy) => {{
                        let list = [];
                        try {{
                            list = (root.elementsFromPoint ? root.elementsFromPoint(gx, gy) : [root.elementFromPoint(gx, gy)].filter(Boolean)) || [];
                        }} catch (_) {{ list = []; }}
                        for (const el of list) {{
                            // Descend into shadow root if present
                            if (el && el.shadowRoot) {{
                                const deep = walk(el.shadowRoot, gx, gy);
                                if (deep) return deep;
                            }}
                            // Descend into same-origin iframe
                            if (el && el.tagName === 'IFRAME') {{
                                try {{
                                    const rect = el.getBoundingClientRect();
                                    const lx = gx - rect.left; // local X inside iframe viewport
                                    const ly = gy - rect.top;  // local Y inside iframe viewport
                                    const doc = el.contentWindow && el.contentWindow.document;
                                    if (doc) {{
                                        const deep = walk(doc, lx, ly);
                                        if (deep) return deep;
                                    }}
                                }} catch(_) {{ /* cross-origin: skip */ }}
                            }}
                            if (el) return el;
                        }}
                        return null;
                    }};
                    return walk(document, x, y);
                }};

                // 1) If something is already focused and is editable (deeply), keep it.
                const current = deepActiveElement();
                if (isEditable(current)) return true;

                // 2) Otherwise, only try to focus the editable element under the cursor.
                if (Number.isFinite(cursorX) && Number.isFinite(cursorY)) {{
                    let el = deepElementFromPoint(cursorX, cursorY);
                    // climb up to an editable ancestor if needed within same composed tree
                    const canFocus = (n) => n && typeof n.focus === 'function';
                    let walker = el;
                    while (walker && !isEditable(walker)) {{
                        walker = walker.parentElement || (walker.getRootNode && (walker.getRootNode().host || null)) || null;
                    }}
                    if (isEditable(walker) && canFocus(walker)) {{
                        walker.focus();
                        const after = deepActiveElement();
                        return after === walker;
                    }}
                }}
                return false; // Do not steal focus by picking arbitrary candidates.
            }})({cursor_x}, {cursor_y})
            "#,
            cursor_x = cursor_x,
            cursor_y = cursor_y
        );

        let result = self.cdp_page.evaluate(script).await?;
        let focused = result.value().and_then(|v| v.as_bool()).unwrap_or(false);
        Ok(focused)
    }

    pub async fn goto(&self, url: &str, wait: Option<WaitStrategy>) -> Result<GotoResult> {
        info!("Navigating to {}", url);

        let wait_strategy = wait.unwrap_or_else(|| self.config.wait.clone());

        // Navigate to the URL with retry on timeout. If Chrome reports timeouts
        // but the page URL actually updates to a real http(s) page, treat it as success.
        let max_retries = 3;
        let mut last_error = None;
        let mut fallback_navigated = false;

        for attempt in 1..=max_retries {
            // Wrap CDP navigation with a short timeout so we don't block ~30s
            // for sites that load but don't signal expected events.
            let nav_attempt =
                tokio::time::timeout(tokio::time::Duration::from_secs(5), self.cdp_page.goto(url))
                    .await;

            match nav_attempt {
                Ok(Ok(_)) => {
                    // Navigation reported success
                    last_error = None;
                    break;
                }
                Ok(Err(e)) => {
                    let error_str = e.to_string();
                    if error_str.contains("Request timed out") || error_str.contains("timeout") {
                        warn!(
                            "Navigation timeout on attempt {}/{}: {}",
                            attempt, max_retries, error_str
                        );
                        last_error = Some(e);

                        // Check if the page actually navigated despite the timeout
                        if let Ok(cur_opt) = self.cdp_page.url().await {
                            if let Some(cur) = cur_opt {
                                let looks_loaded =
                                    cur.starts_with("http://") || cur.starts_with("https://");
                                if looks_loaded && cur != "about:blank" {
                                    info!(
                                        "Navigation reported timeout, but page URL is now {} — treating as success",
                                        cur
                                    );
                                    fallback_navigated = true;
                                    last_error = None;
                                    break;
                                }
                            }
                        }

                        if attempt < max_retries {
                            // Wait before retry, increasing delay each time
                            let delay_ms = 1000 * attempt as u64;
                            info!("Retrying navigation after {}ms...", delay_ms);
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                            continue;
                        }
                    } else {
                        // Non-timeout error, fail immediately
                        return Err(e.into());
                    }
                }
                Err(_) => {
                    // Our outer timeout fired; fallback to URL check, same as above.
                    warn!(
                        "Navigation attempt {}/{} exceeded 5s timeout; checking current URL...",
                        attempt, max_retries
                    );
                    // Check if the page actually navigated despite the timeout
                    if let Ok(cur_opt) = self.cdp_page.url().await {
                        if let Some(cur) = cur_opt {
                            let looks_loaded =
                                cur.starts_with("http://") || cur.starts_with("https://");
                            if looks_loaded && cur != "about:blank" {
                                info!(
                                    "Navigation exceeded timeout, but page URL is now {} — treating as success",
                                    cur
                                );
                                fallback_navigated = true;
                                last_error = None;
                                break;
                            }
                        }
                    }

                    if attempt < max_retries {
                        let delay_ms = 1000 * attempt as u64;
                        info!("Retrying navigation after {}ms...", delay_ms);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                    // If this was the last attempt, return a synthetic timeout error
                    return Err(BrowserError::CdpError("Navigation timed out".to_string()));
                }
            }
        }

        // If we exhausted retries and still have an error, bail out
        if let Some(e) = last_error {
            return Err(BrowserError::CdpError(format!(
                "Navigation failed after {} retries: {}",
                max_retries, e
            )));
        }

        // Wait according to the strategy
        match wait_strategy {
            WaitStrategy::Event(event) => match event.as_str() {
                "domcontentloaded" => {
                    if fallback_navigated {
                        // Poll document.readyState instead of wait_for_navigation()
                        let script = "document.readyState";
                        let start = std::time::Instant::now();
                        loop {
                            let state = self.cdp_page.evaluate(script).await.ok().and_then(|r| {
                                r.value().and_then(|v| v.as_str().map(|s| s.to_string()))
                            });
                            if matches!(state.as_deref(), Some("interactive") | Some("complete")) {
                                break;
                            }
                            if start.elapsed() > std::time::Duration::from_secs(3) {
                                break;
                            }
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    } else {
                        // Wait for DOMContentLoaded event
                        self.cdp_page.wait_for_navigation().await?;
                    }
                }
                "networkidle" | "networkidle0" => {
                    // Wait for network to be idle
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                }
                "networkidle2" => {
                    // Wait for network to be mostly idle
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
                "load" => {
                    if fallback_navigated {
                        // Poll for complete state
                        let script = "document.readyState";
                        let start = std::time::Instant::now();
                        loop {
                            let state = self.cdp_page.evaluate(script).await.ok().and_then(|r| {
                                r.value().and_then(|v| v.as_str().map(|s| s.to_string()))
                            });
                            if matches!(state.as_deref(), Some("complete")) {
                                break;
                            }
                            if start.elapsed() > std::time::Duration::from_secs(4) {
                                break;
                            }
                            tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
                        }
                        // Small cushion after load
                        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                    } else {
                        // Wait for load event
                        self.cdp_page.wait_for_navigation().await?;
                        // Add extra delay to ensure page is fully loaded
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
                _ => {
                    return Err(BrowserError::ConfigError(format!(
                        "Unknown wait event: {}",
                        event
                    )));
                }
            },
            WaitStrategy::Delay { delay_ms } => {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }

        // Get the final URL and title after navigation completes
        let title = self.cdp_page.get_title().await.ok().flatten();

        // Try to get the URL multiple times in case it's not immediately available
        let mut final_url = None;
        for _ in 0..3 {
            if let Ok(Some(url)) = self.cdp_page.url().await {
                final_url = Some(url);
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        let final_url = final_url.unwrap_or_else(|| url.to_string());

        let mut current_url = self.current_url.write().await;
        *current_url = Some(final_url.clone());
        drop(current_url); // Release lock before injecting cursor

        // Ensure the virtual cursor after navigation
        debug!("Ensuring virtual cursor after navigation");
        if let Err(e) = self.ensure_virtual_cursor().await {
            warn!("Failed to inject virtual cursor after navigation: {}", e);
            // Continue even if cursor injection fails
        }

        Ok(GotoResult {
            url: final_url,
            title,
        })
    }

    // (UPDATED) Inject cursor before taking screenshot
    pub async fn screenshot(&self, mode: ScreenshotMode) -> Result<Vec<Screenshot>> {
        // Do not adjust device metrics before screenshots; this causes flashing on
        // external Chrome and adds latency. Rely on connect-time configuration.

        // Fast path: ensure the virtual cursor exists before capturing
        let injected = match self.ensure_virtual_cursor().await {
            Ok(injected) => injected,
            Err(e) => {
                warn!("Failed to inject virtual cursor: {}", e);
                // Continue with screenshot even if cursor injection fails
                false
            }
        };

        // Do not wait for animations to settle; capture current frame to preserve visible motion
        // Small render delay only on fresh injection to avoid empty frame
        if injected {
            tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
        }

        match mode {
            ScreenshotMode::Viewport => self.screenshot_viewport().await,
            ScreenshotMode::FullPage { segments_max } => {
                self.screenshot_fullpage(segments_max.unwrap_or(self.config.segments_max))
                    .await
            }
            ScreenshotMode::Region(region) => self.screenshot_region(region).await,
        }
    }

    pub async fn screenshot_viewport(&self) -> Result<Vec<Screenshot>> {
        // Safe viewport capture: do not change device metrics or viewport.
        // Measure CSS viewport size via JS and capture a clipped image
        // using the compositor without affecting focus.
        debug!("Taking viewport screenshot (safe clip, no resize)");

        let format = match self.config.format {
            ImageFormat::Png => CaptureScreenshotFormat::Png,
            ImageFormat::Webp => CaptureScreenshotFormat::Webp,
        };

        // Probe CSS viewport using Runtime.evaluate to avoid layout_metrics
        let probe = self
            .inject_js(
                "(() => ({ w: (document.documentElement.clientWidth|0), h: (document.documentElement.clientHeight|0) }))()",
            )
            .await
            .unwrap_or(serde_json::Value::Null);

        let doc_w = probe.get("w").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let doc_h = probe.get("h").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        // Fall back to configured viewport if probe failed
        let vw = if doc_w > 0 {
            doc_w
        } else {
            self.config.viewport.width
        };
        let vh = if doc_h > 0 {
            doc_h
        } else {
            self.config.viewport.height
        };

        // Clamp to configured maximums to keep images small for the LLM
        let target_w = vw.min(self.config.viewport.width);
        let target_h = vh.min(self.config.viewport.height);

        let params_builder = CaptureScreenshotParams::builder()
            .format(format)
            .capture_beyond_viewport(true)
            .clip(chromiumoxide::cdp::browser_protocol::page::Viewport {
                x: 0.0,
                y: 0.0,
                width: target_w as f64,
                height: target_h as f64,
                scale: 1.0,
            });

        // Use our retry logic to handle cases where window is not visible
        let resp = self.capture_screenshot_with_retry(params_builder).await?;
        let data_b64: &str = resp.data.as_ref();
        let data = base64::engine::general_purpose::STANDARD
            .decode(data_b64.as_bytes())
            .map_err(|e| BrowserError::ScreenshotError(format!("base64 decode failed: {}", e)))?;

        Ok(vec![Screenshot {
            data,
            width: target_w,
            height: target_h,
            format: self.config.format,
        }])
    }

    pub async fn screenshot_fullpage(&self, segments_max: usize) -> Result<Vec<Screenshot>> {
        let format = match self.config.format {
            ImageFormat::Png => CaptureScreenshotFormat::Png,
            ImageFormat::Webp => CaptureScreenshotFormat::Webp,
        };

        // 1) Get document dimensions (CSS px)
        let lm = self.cdp_page.layout_metrics().await?;
        let content = lm.css_content_size; // Rect (not Option)
        let doc_w = content.width.ceil() as u32;
        let doc_h = content.height.ceil() as u32;

        // Use your configured viewport width, but never exceed doc width
        let vw = self.config.viewport.width.min(doc_w);
        let vh = self.config.viewport.height;

        // 2) Slice the page by y-offsets WITHOUT scrolling the page
        let mut shots = Vec::new();
        let mut y: u32 = 0;
        let mut taken = 0usize;

        while y < doc_h && taken < segments_max {
            let h = vh.min(doc_h - y); // last slice may be shorter
            let params_builder = CaptureScreenshotParams::builder()
                .format(format.clone())
                .capture_beyond_viewport(true) // key to avoid scrolling/flash
                .clip(chromiumoxide::cdp::browser_protocol::page::Viewport {
                    x: 0.0,
                    y: y as f64,
                    width: vw as f64,
                    height: h as f64,
                    scale: 1.0,
                });

            let resp = self.capture_screenshot_with_retry(params_builder).await?;
            let data_b64: &str = resp.data.as_ref();
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_b64.as_bytes())
                .map_err(|e| {
                    BrowserError::ScreenshotError(format!("base64 decode failed: {}", e))
                })?;
            shots.push(Screenshot {
                data,
                width: vw,
                height: h,
                format: self.config.format,
            });

            y += h;
            taken += 1;
        }

        if taken == segments_max && y < doc_h {
            info!("[full page truncated at {} segments]", segments_max);
        }

        Ok(shots)
    }

    pub async fn screenshot_region(&self, region: ScreenshotRegion) -> Result<Vec<Screenshot>> {
        debug!(
            "Taking region screenshot: {}x{} at ({}, {})",
            region.width, region.height, region.x, region.y
        );

        let format = match self.config.format {
            ImageFormat::Png => CaptureScreenshotFormat::Png,
            ImageFormat::Webp => CaptureScreenshotFormat::Webp,
        };

        let params_builder = CaptureScreenshotParams::builder().format(format).clip(
            chromiumoxide::cdp::browser_protocol::page::Viewport {
                x: region.x as f64,
                y: region.y as f64,
                width: region.width as f64,
                height: region.height as f64,
                scale: 1.0,
            },
        );

        let resp = self.capture_screenshot_with_retry(params_builder).await?;
        let data_b64: &str = resp.data.as_ref();
        let data = base64::engine::general_purpose::STANDARD
            .decode(data_b64.as_bytes())
            .map_err(|e| BrowserError::ScreenshotError(format!("base64 decode failed: {}", e)))?;

        let final_width = if region.width > 1024 {
            1024
        } else {
            region.width
        };

        Ok(vec![Screenshot {
            data,
            width: final_width,
            height: region.height,
            format: self.config.format,
        }])
    }

    pub async fn set_viewport(&self, viewport: SetViewportParams) -> Result<ViewportResult> {
        // Apply CDP device metrics override once on demand
        let params = SetDeviceMetricsOverrideParams::builder()
            .width(viewport.width as i64)
            .height(viewport.height as i64)
            .device_scale_factor(viewport.device_scale_factor.unwrap_or(1.0))
            .mobile(viewport.mobile.unwrap_or(false))
            .build()
            .map_err(|e| BrowserError::CdpError(format!("Failed to build viewport params: {}", e)))?;
        self.cdp_page.execute(params).await?;

        Ok(ViewportResult {
            width: viewport.width,
            height: viewport.height,
            dpr: viewport.device_scale_factor.unwrap_or(1.0),
        })
    }

    pub async fn inject_js(&self, script: &str) -> Result<serde_json::Value> {
        let result = self.cdp_page.evaluate(script).await?;
        Ok(result.value().cloned().unwrap_or(serde_json::Value::Null))
    }

    pub async fn close(&self) -> Result<()> {
        // Note: chromiumoxide's close() takes ownership, so we can't call it on Arc<Page>
        // The page will be closed when the Arc is dropped
        Ok(())
    }

    /// Return a snapshot (tail) of the CDP-captured console buffer.
    pub async fn get_console_logs_tail(&self, lines: Option<usize>) -> serde_json::Value {
        let buf = self.console_logs.lock().await;
        if buf.is_empty() {
            return serde_json::Value::Array(vec![]);
        }
        let n = lines.unwrap_or(0);
        let slice: Vec<serde_json::Value> = if n > 0 && n < buf.len() {
            buf[buf.len() - n..].to_vec()
        } else {
            buf.clone()
        };
        serde_json::Value::Array(slice)
    }

    pub async fn get_url(&self) -> Result<String> {
        let url_guard = self.current_url.read().await;
        url_guard.clone().ok_or(BrowserError::PageNotLoaded)
    }

    /// Get the current URL directly from the browser (not cached)
    pub async fn get_current_url(&self) -> Result<String> {
        match self.cdp_page.url().await? {
            Some(url) => Ok(url),
            None => Err(BrowserError::PageNotLoaded),
        }
    }

    pub fn target_id_debug(&self) -> String {
        let target_id = self.cdp_page.target_id();
        format!("{target_id:?}")
    }

    pub fn session_id_debug(&self) -> String {
        let session_id = self.cdp_page.session_id();
        format!("{session_id:?}")
    }

    pub fn opener_id_debug(&self) -> Option<String> {
        self.cdp_page
            .opener_id()
            .as_ref()
            .map(|opener_id| format!("{opener_id:?}"))
    }

    pub async fn update_viewport(&self, _viewport: ViewportConfig) -> Result<()> {
        Ok(())
    }

    // Move the mouse by relative offset from current position
    pub async fn move_mouse_relative(&self, dx: f64, dy: f64) -> Result<(f64, f64)> {
        // Get current position
        let cursor = self.cursor_state.lock().await;
        let current_x = cursor.x;
        let current_y = cursor.y;
        drop(cursor);

        // Calculate new position
        let new_x = current_x + dx;
        let new_y = current_y + dy;

        debug!(
            "Moving mouse relatively by ({}, {}) from ({}, {}) to ({}, {})",
            dx, dy, current_x, current_y, new_x, new_y
        );

        // Use absolute move with the calculated position
        self.move_mouse(new_x, new_y).await?;
        Ok((new_x, new_y))
    }

    // (NEW) Move the mouse to the specified coordinates
    pub async fn move_mouse(&self, x: f64, y: f64) -> Result<()> {
        debug!("Moving mouse to ({}, {})", x, y);

        // Clamp and floor coordinates
        let move_x = x.floor().max(0.0);
        let move_y = y.floor().max(0.0);

        let mut cursor = self.cursor_state.lock().await;

        // If target is effectively the same as current, avoid dispatching/animating
        if (cursor.x - move_x).abs() < 0.5 && (cursor.y - move_y).abs() < 0.5 {
            drop(cursor);
            // Ensure cursor is present/updated even if no move
            let _ = self.ensure_virtual_cursor().await;
            return Ok(());
        }

        // Dispatch the mouse move event, including the current button state
        let move_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseMoved)
            .x(move_x)
            .y(move_y)
            .button(cursor.button.clone()) // Pass the button state
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(move_params).await?;

        // Update cursor position
        cursor.x = move_x;
        cursor.y = move_y;
        drop(cursor); // Release lock before JavaScript evaluation

        // First check if cursor exists, if not inject it
        let check_script = "typeof window.__vc !== 'undefined'";
        let cursor_exists = self
            .cdp_page
            .evaluate(check_script)
            .await
            .ok()
            .and_then(|result| result.value().and_then(|v| v.as_bool()))
            .unwrap_or(false);

        if !cursor_exists {
            debug!("Virtual cursor not found, injecting it now");
            if let Err(e) = self.inject_virtual_cursor().await {
                warn!("Failed to inject virtual cursor: {}", e);
            }
        }

        // For internal browser, snap instantly without animation. For external, animate and respect duration.
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let dur_ms = if is_external {
            self
                .cdp_page
                .evaluate(format!(
                    "(function(x,y){{ try {{ if(window.__vc && window.__vc.update) return window.__vc.update(x,y)|0; }} catch(_e){{}} return 0; }})({:.0},{:.0})",
                    move_x, move_y
                ))
                .await
                .ok()
                .and_then(|r| r.value().and_then(|v| v.as_u64()))
                .unwrap_or(0) as u64
        } else {
            // Internal browser: snap immediately and report zero duration
            let _ = self
                .cdp_page
                .evaluate(format!(
                    "(function(x,y){{ try {{ if(window.__vc && window.__vc.snapTo) {{ window.__vc.snapTo(x,y); return 0; }} }} catch(_e){{}} return 0; }})({:.0},{:.0})",
                    move_x, move_y
                ))
                .await;
            0
        };

        // Only wait when connected to an external browser and there is a non-zero duration
        if is_external && dur_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(dur_ms)).await;
        }

        Ok(())
    }

    /// (UPDATED) Click at the specified coordinates with visual animation
    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        debug!("Clicking at ({}, {})", x, y);

        // Use move_mouse to handle movement, clamping, and state update
        self.move_mouse(x, y).await?;

        // Get the final coordinates after potential clamping
        let cursor = self.cursor_state.lock().await;
        let click_x = cursor.x;
        let click_y = cursor.y;
        drop(cursor); // Release lock before async calls

        // Trigger click pulse animation via virtual cursor API and wait briefly
        let click_ms_val = self
            .cdp_page
            .evaluate(
                "(function(){ if(window.__vc && window.__vc.clickPulse){ return window.__vc.clickPulse(); } return 0; })()",
            )
            .await
            .ok()
            .and_then(|r| r.value().and_then(|v| v.as_u64()))
            .unwrap_or(0) as u64;

        // Mouse down
        let down_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Add a small delay between press and release
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;

        // Mouse up
        let up_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        // Wait briefly so the page processes the click; avoid long animation waits
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let settle_ms = if is_external { click_ms_val.min(240) } else { 40 };
        if settle_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(settle_ms)).await;
        }

        Ok(())
    }

    /// Perform mouse down at the current position
    pub async fn mouse_down_at_current(&self) -> Result<(f64, f64)> {
        let cursor = self.cursor_state.lock().await;
        let x = cursor.x;
        let y = cursor.y;
        let is_down = cursor.is_mouse_down;
        drop(cursor);

        if is_down {
            debug!("Mouse is already down at ({}, {})", x, y);
            return Ok((x, y));
        }

        debug!("Mouse down at current position ({}, {})", x, y);

        let down_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Update mouse state (track button for drag moves)
        let mut cursor = self.cursor_state.lock().await;
        cursor.is_mouse_down = true;
        cursor.button = MouseButton::Left;
        drop(cursor);

        Ok((x, y))
    }

    /// Perform mouse up at the current position
    pub async fn mouse_up_at_current(&self) -> Result<(f64, f64)> {
        let cursor = self.cursor_state.lock().await;
        let x = cursor.x;
        let y = cursor.y;
        let is_down = cursor.is_mouse_down;
        drop(cursor);

        if !is_down {
            debug!("Mouse is already up at ({}, {})", x, y);
            return Ok((x, y));
        }

        debug!("Mouse up at current position ({}, {})", x, y);

        let up_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(x)
            .y(y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        // Update mouse state
        let mut cursor = self.cursor_state.lock().await;
        cursor.is_mouse_down = false;
        cursor.button = MouseButton::None;
        drop(cursor);

        Ok((x, y))
    }

    /// Click at the current mouse position without moving the cursor
    pub async fn click_at_current(&self) -> Result<(f64, f64)> {
        // Get the current cursor position and check if mouse is down
        let cursor = self.cursor_state.lock().await;
        let click_x = cursor.x;
        let click_y = cursor.y;
        let was_down = cursor.is_mouse_down;
        debug!(
            "Clicking at current position ({}, {}), mouse was_down: {}",
            click_x, click_y, was_down
        );
        drop(cursor); // Release lock before async calls

        // If mouse is already down, release it first
        if was_down {
            debug!("Mouse was down, releasing first before click");
            self.mouse_up_at_current().await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
        }

        // Trigger click animation through virtual cursor API
        let click_ms_val = self
            .cdp_page
            .evaluate(
                "(function(){ if(window.__vc && window.__vc.clickPulse){ return window.__vc.clickPulse(); } return 0; })()",
            )
            .await
            .ok()
            .and_then(|r| r.value().and_then(|v| v.as_u64()))
            .unwrap_or(0) as u64;

        // Mouse down
        let down_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Add a small delay between press and release
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;

        // Mouse up
        let up_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(click_x)
            .y(click_y)
            .button(MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        // Wait briefly so the page processes the click; avoid long animation waits
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let settle_ms = if is_external { click_ms_val.min(240) } else { 40 };
        if settle_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(settle_ms)).await;
        }

        Ok((click_x, click_y))
    }

    /// Type text into the currently focused element with optimized typing strategies
    pub async fn type_text(&self, text: &str) -> Result<()> {
        // Replace em dashes with regular dashes
        let processed_text = text.replace('—', " - ");
        debug!("Typing text: {}", processed_text);

        // Ensure an editable element is focused first. If we cannot ensure focus
        // on an editable field, bail to avoid sending keystrokes to the wrong place.
        let ensured = self.ensure_editable_focused().await?;
        if !ensured {
            debug!("No editable focus ensured; skipping typing to avoid stealing focus");
            return Ok(());
        }

        // Install a temporary focus guard that keeps focus anchored on the
        // currently focused editable unless the user intentionally sends Tab/Enter.
        let _ = self.execute_javascript(
            r#"(() => {
                try {
                  const isEditableInputType = (t) => !/^(checkbox|radio|button|submit|reset|file|image|color|hidden|range)$/i.test(t || '');
                  const isEditable = (el) => !!el && (
                    (el.tagName === 'INPUT' && isEditableInputType(el.type)) ||
                    el.tagName === 'TEXTAREA' ||
                    el.isContentEditable === true
                  );
                  const deepActiveElement = (rootDoc) => {
                    let ae = (rootDoc || document).activeElement;
                    // Shadow roots
                    while (ae && ae.shadowRoot && ae.shadowRoot.activeElement) {
                      ae = ae.shadowRoot.activeElement;
                    }
                    // Same-origin iframes
                    while (ae && ae.tagName === 'IFRAME') {
                      try {
                        const doc = ae.contentWindow && ae.contentWindow.document;
                        if (!doc) break;
                        let inner = doc.activeElement;
                        if (!inner) break;
                        while (inner && inner.shadowRoot && inner.shadowRoot.activeElement) {
                          inner = inner.shadowRoot.activeElement;
                        }
                        ae = inner;
                      } catch (_) { break; }
                    }
                    return ae || null;
                  };

                  const w = window;
                  if (!w.__codeFG) {
                    w.__codeFG = {
                      active: false,
                      lastKey: null,
                      anchor: null,
                      onKeyDown: null,
                      onFocusIn: null,
                      onBlur: null,
                      install() {
                        const anchor = deepActiveElement();
                        if (!isEditable(anchor)) return false;
                        this.anchor = anchor;
                        this.active = true;
                        this.lastKey = null;
                        this.onKeyDown = (e) => { this.lastKey = e && e.key; };
                        this.onFocusIn = (e) => {
                          if (!this.active) return;
                          const a = this.anchor;
                          const curr = deepActiveElement();
                          if (!a || a === curr) return;
                          // Allow intentional navigations
                          if (this.lastKey === 'Tab' || this.lastKey === 'Enter') return;
                          // If anchor was detached or hidden, stop guarding
                          try {
                            const cs = a.ownerDocument && a.ownerDocument.defaultView && a.ownerDocument.defaultView.getComputedStyle(a);
                            const hidden = !a.isConnected || (cs && (cs.display === 'none' || cs.visibility === 'hidden'));
                            if (hidden) { this.active = false; return; }
                          } catch(_){}
                          // Restore focus asynchronously to override app-level auto-tabbing
                          setTimeout(() => { try { a.focus && a.focus(); } catch(_){} }, 0);
                        };
                        this.onBlur = () => { /* ignore */ };
                        document.addEventListener('keydown', this.onKeyDown, true);
                        document.addEventListener('focusin', this.onFocusIn, true);
                        document.addEventListener('blur', this.onBlur, true);
                        return true;
                      },
                      uninstall() {
                        try {
                          document.removeEventListener('keydown', this.onKeyDown, true);
                          document.removeEventListener('focusin', this.onFocusIn, true);
                          document.removeEventListener('blur', this.onBlur, true);
                        } catch(_){}
                        this.active = false;
                        this.anchor = null;
                        this.lastKey = null;
                        return true;
                      }
                    };
                  }
                  return window.__codeFG.install();
                } catch(_) { return false; }
            })()"#
        ).await;

        let text_len = processed_text.len();

        if text_len >= 100 {
            // Large text: paste-style insertion with no per-char delay
            // Try to insert at caret for input/textarea and contenteditable; fall back to raw key events without delay.
            let js = format!(
                r#"(() => {{
  try {{
    const T = {text_json};
    const isEditableInputType = (t) => !/^(checkbox|radio|button|submit|reset|file|image|color|hidden|range)$/i.test(t || '');
    const isEditable = (el) => !!el && ((el.tagName === 'INPUT' && isEditableInputType(el.type)) || el.tagName === 'TEXTAREA' || el.isContentEditable === true);
    const deepActiveElement = (rootDoc) => {{
      let ae = (rootDoc || document).activeElement;
      while (ae && ae.shadowRoot && ae.shadowRoot.activeElement) {{ ae = ae.shadowRoot.activeElement; }}
      while (ae && ae.tagName === 'IFRAME') {{
        try {{
          const doc = ae.contentWindow && ae.contentWindow.document; if (!doc) break;
          let inner = doc.activeElement; if (!inner) break;
          while (inner && inner.shadowRoot && inner.shadowRoot.activeElement) {{ inner = inner.shadowRoot.activeElement; }}
          ae = inner;
        }} catch (_) {{ break; }}
      }}
      return ae || null;
    }};
    const ae = deepActiveElement();
    if (!isEditable(ae)) return {{ success: false, reason: 'no-editable' }};

    if (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA') {{
      const start = ae.selectionStart|0, end = ae.selectionEnd|0;
      const val = String(ae.value || '');
      const before = val.slice(0, start), after = val.slice(end);
      ae.value = before + T + after;
      const pos = (before + T).length;
      ae.selectionStart = ae.selectionEnd = pos;
      try {{ ae.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: T }})); }} catch (_e) {{}}
      return {{ success: true, inserted: T.length, caret: pos }};
    }} else if (ae.isContentEditable === true) {{
      try {{ if (document.execCommand) {{ document.execCommand('insertText', false, T); return {{ success: true, inserted: T.length }}; }} }} catch (_e) {{}}
      try {{
        const sel = window.getSelection();
        if (sel && sel.rangeCount) {{
          const r = sel.getRangeAt(0);
          r.deleteContents();
          r.insertNode(document.createTextNode(T));
          r.collapse(false);
          return {{ success: true, inserted: T.length }};
        }}
      }} catch (_e) {{}}
      return {{ success: false, reason: 'contenteditable-failed' }};
    }}
    return {{ success: false, reason: 'unsupported' }};
  }} catch (e) {{ return {{ success: false, error: String(e) }}; }}
}})()"#,
                text_json = serde_json::to_string(&processed_text).unwrap_or_else(|_| "".to_string())
            );

            let _ = self.execute_javascript(&js).await;
        } else {
            // Short/medium text: per-character with reduced delay 30–60ms
            for ch in processed_text.chars() {
                if ch == '\n' {
                    self.press_key("Enter").await?;
                } else if ch == '\t' {
                    self.press_key("Tab").await?;
                } else {
                    let params = DispatchKeyEventParams::builder()
                        .r#type(DispatchKeyEventType::Char)
                        .text(ch.to_string())
                        .build()
                        .map_err(BrowserError::CdpError)?;
                    self.cdp_page.execute(params).await?;
                }

                // Reduced natural typing delay
                let delay = 30 + (rand::random::<u64>() % 31); // 30–60ms
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }

        // Remove the focus guard shortly after typing to cover post-typing side effects
        let _ = self.execute_javascript(
            r#"(() => { try { if (window.__codeFG && window.__codeFG.uninstall) { setTimeout(() => { try { window.__codeFG.uninstall(); } catch(_){} }, 500); return true; } return false; } catch(_) { return false; } })()"#
        ).await;

        Ok(())
    }

    /// Press a key (e.g., "Enter", "Tab", "Escape", "ArrowDown")
    pub async fn press_key(&self, key: &str) -> Result<()> {
        debug!("Pressing key: {}", key);

        // Map key names to their proper codes and virtual key codes
        let (code, text, windows_virtual_key_code, native_virtual_key_code) = match key {
            "Enter" => ("Enter", Some("\r"), Some(13), Some(13)),
            "Tab" => ("Tab", Some("\t"), Some(9), Some(9)),
            "Escape" => ("Escape", None, Some(27), Some(27)),
            "Backspace" => ("Backspace", None, Some(8), Some(8)),
            "Delete" => ("Delete", None, Some(46), Some(46)),
            "ArrowUp" => ("ArrowUp", None, Some(38), Some(38)),
            "ArrowDown" => ("ArrowDown", None, Some(40), Some(40)),
            "ArrowLeft" => ("ArrowLeft", None, Some(37), Some(37)),
            "ArrowRight" => ("ArrowRight", None, Some(39), Some(39)),
            "Home" => ("Home", None, Some(36), Some(36)),
            "End" => ("End", None, Some(35), Some(35)),
            "PageUp" => ("PageUp", None, Some(33), Some(33)),
            "PageDown" => ("PageDown", None, Some(34), Some(34)),
            "Space" => ("Space", Some(" "), Some(32), Some(32)),
            _ => (key, None, None, None), // Default fallback
        };

        // Key down
        let mut down_builder = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyDown)
            .key(key.to_string())
            .code(code.to_string());

        if let Some(vk) = windows_virtual_key_code {
            down_builder = down_builder.windows_virtual_key_code(vk);
        }
        if let Some(nvk) = native_virtual_key_code {
            down_builder = down_builder.native_virtual_key_code(nvk);
        }

        let down_params = down_builder.build().map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(down_params).await?;

        // Send char event for keys that produce text
        if let Some(text_str) = text {
            let char_params = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::Char)
                .key(key.to_string())
                .code(code.to_string())
                .text(text_str.to_string())
                .build()
                .map_err(BrowserError::CdpError)?;
            self.cdp_page.execute(char_params).await?;
        }

        // Key up
        let mut up_builder = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(key.to_string())
            .code(code.to_string());

        if let Some(vk) = windows_virtual_key_code {
            up_builder = up_builder.windows_virtual_key_code(vk);
        }
        if let Some(nvk) = native_virtual_key_code {
            up_builder = up_builder.native_virtual_key_code(nvk);
        }

        let up_params = up_builder.build().map_err(BrowserError::CdpError)?;
        self.cdp_page.execute(up_params).await?;

        Ok(())
    }

    /// Execute JavaScript code with enhanced return value handling
    pub async fn execute_javascript(&self, code: &str) -> Result<serde_json::Value> {
        debug!(
            "Executing JavaScript: {}...",
            &code.chars().take(100).collect::<String>()
        );

        // Create the user code with sourceURL for better debugging
        let user_code_with_source = format!("{code}\n//# sourceURL=browser_js_user_code.js");

        // Use the improved JavaScript harness
        let wrapped = format!(
            r#"(async () => {{
  const __meta = {{ startTs: Date.now(), urlBefore: location.href }};
  const __logs = [];
  const __errs = [];

  const __orig = {{
    log: console.log, warn: console.warn, error: console.error, debug: console.debug
  }};

  function __normalize(v, d = 0) {{
    const MAX_DEPTH = 3, MAX_STR = 4000;
    if (d > MAX_DEPTH) return {{ __type: 'truncated' }};
    if (v === undefined) return {{ __type: 'undefined' }};
    if (v === null || typeof v === 'number' || typeof v === 'boolean') return v;
    if (typeof v === 'string') return v.length > MAX_STR ? v.slice(0, MAX_STR) + '…' : v;
    if (typeof v === 'bigint') return {{ __type: 'bigint', value: v.toString() + 'n' }};
    if (typeof v === 'symbol') return {{ __type: 'symbol', value: String(v) }};
    if (typeof v === 'function') return {{ __type: 'function', name: v.name || '' }};

    if (typeof Element !== 'undefined' && v instanceof Element) {{
      return {{
        __type: 'element',
        tag: v.tagName, id: v.id || null, class: v.className || null,
        text: (v.textContent || '').trim().slice(0, 200)
      }};
    }}
    try {{ return JSON.parse(JSON.stringify(v)); }} catch {{}}

    if (Array.isArray(v)) return v.slice(0, 50).map(x => __normalize(x, d + 1));

    const out = Object.create(null);
    let n = 0;
    for (const k in v) {{
      if (!Object.prototype.hasOwnProperty.call(v, k)) continue;
      out[k] = __normalize(v[k], d + 1);
      if (++n >= 50) {{ out.__truncated = true; break; }}
    }}
    return out;
  }}

  const __push = (level, args) => {{
    __logs.push({{ level, args: args.map(a => __normalize(a)) }});
  }};
  console.log  = (...a) => {{ __push('log', a);  __orig.log(...a);  }};
  console.warn = (...a) => {{ __push('warn', a); __orig.warn(...a); }};
  console.error= (...a) => {{ __push('error',a); __orig.error(...a); }};
  console.debug= (...a) => {{ __push('debug',a); __orig.debug(...a); }};

  window.addEventListener('error', e => {{
    try {{ __errs.push(String(e.error || e.message || e)); }} catch {{ __errs.push('window.error'); }}
  }});
  window.addEventListener('unhandledrejection', e => {{
    try {{ __errs.push('unhandledrejection: ' + String(e.reason)); }} catch {{ __errs.push('unhandledrejection'); }}
  }});

  try {{
    const AsyncFunction = Object.getPrototypeOf(async function(){{}}).constructor;
    const __userCode = {0};
    const evaluator = new AsyncFunction('__code', '"use strict"; return eval(__code);');
    const raw = await evaluator(__userCode);
    const value = (raw === undefined ? null : __normalize(raw));

    return {{
      success: true,
      value,
      logs: __logs,
      errors: __errs,
      meta: {{
        urlBefore: __meta.urlBefore,
        urlAfter: location.href,
        durationMs: Date.now() - __meta.startTs
      }}
    }};
  }} catch (err) {{
    return {{
      success: false,
      value: null,
      error: String(err),
      stack: (err && err.stack) ? String(err.stack) : '',
      logs: __logs,
      errors: __errs
    }};
  }} finally {{
    console.log = __orig.log;
    console.warn = __orig.warn;
    console.error = __orig.error;
    console.debug = __orig.debug;
  }}
}})()"#,
            serde_json::to_string(&user_code_with_source).expect("Failed to serialize user code")
        );

        tracing::debug!("Executing JavaScript code: {}", code);
        tracing::debug!("Wrapped code: {}", wrapped);

        // Execute the wrapped code - chromiumoxide's evaluate method handles async functions
        let result = self.cdp_page.evaluate(wrapped).await?;
        let result_value = result.value().cloned().unwrap_or(serde_json::Value::Null);

        tracing::debug!("JavaScript execution result: {}", result_value);

        // Give a very brief moment for potential navigation or DOM updates triggered
        // by the script. Keep this low to avoid inflating tool latency.
        let is_external = self.config.connect_port.is_some() || self.config.connect_ws.is_some();
        let settle_ms = if is_external { 120 } else { 40 };
        tokio::time::sleep(tokio::time::Duration::from_millis(settle_ms)).await;

        Ok(result_value)
    }

    /// Scroll the page by the given delta in pixels
    pub async fn scroll_by(&self, dx: f64, dy: f64) -> Result<()> {
        debug!("Scrolling by ({}, {})", dx, dy);
        let js = format!(
            "(function() {{ window.scrollBy({dx}, {dy}); return {{ x: window.scrollX, y: window.scrollY }}; }})()"
        );
        let _ = self.execute_javascript(&js).await?;
        Ok(())
    }

    /// Navigate browser history backward one entry
    pub async fn go_back(&self) -> Result<()> {
        debug!("History back");
        let _ = self.execute_javascript("history.back();").await?;
        Ok(())
    }

    /// Navigate browser history forward one entry
    pub async fn go_forward(&self) -> Result<()> {
        debug!("History forward");
        let _ = self.execute_javascript("history.forward();").await?;
        Ok(())
    }

    /// Get the current cursor position
    pub async fn get_cursor_position(&self) -> Result<(f64, f64)> {
        let cursor = self.cursor_state.lock().await;
        Ok((cursor.x, cursor.y))
    }
}

// Raw CDP command wrapper to allow executing arbitrary methods with JSON params
#[derive(Debug, Clone)]
struct RawCdpCommand {
    method: String,
    params: serde_json::Value,
}

impl RawCdpCommand {
    fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            method: method.into(),
            params,
        }
    }
}

impl serde::Serialize for RawCdpCommand {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize only the params as the Command payload
        self.params.serialize(serializer)
    }
}

impl chromiumoxide_types::Method for RawCdpCommand {
    fn identifier(&self) -> chromiumoxide_types::MethodId {
        self.method.clone().into()
    }
}

impl chromiumoxide_types::Command for RawCdpCommand {
    type Response = serde_json::Value;
}

impl Page {
    /// Execute an arbitrary CDP method with the provided JSON params against this page's session
    pub async fn execute_cdp_raw(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let cmd = RawCdpCommand::new(method, params);
        let resp = self.cdp_page.execute(cmd).await?;
        Ok(resp.result)
    }
}

#[derive(Debug, Clone)]
pub enum ScreenshotMode {
    Viewport,
    FullPage { segments_max: Option<usize> },
    Region(ScreenshotRegion),
}

#[derive(Debug, Clone)]
pub struct ScreenshotRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct Screenshot {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}

#[derive(Debug, serde::Serialize)]
pub struct GotoResult {
    pub url: String,
    pub title: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SetViewportParams {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: Option<f64>,
    pub mobile: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct ViewportResult {
    pub width: u32,
    pub height: u32,
    pub dpr: f64,
}
