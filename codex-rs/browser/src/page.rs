use crate::{
    config::{BrowserConfig, ImageFormat, ViewportConfig, WaitStrategy},
    BrowserError, Result,
};
use chromiumoxide::page::Page as CdpPage;
use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, CaptureScreenshotParams};
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::input::{
    DispatchMouseEventParams, DispatchMouseEventType,
    DispatchKeyEventParams, DispatchKeyEventType,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

pub struct Page {
    cdp_page: Arc<CdpPage>,
    config: BrowserConfig,
    current_url: Arc<RwLock<Option<String>>>,
}

impl Page {
    pub fn new(cdp_page: CdpPage, config: BrowserConfig) -> Self {
        Self {
            cdp_page: Arc::new(cdp_page),
            config,
            current_url: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn goto(&self, url: &str, wait: Option<WaitStrategy>) -> Result<GotoResult> {
        info!("Navigating to {}", url);
        
        let wait_strategy = wait.unwrap_or_else(|| self.config.wait.clone());
        
        self.cdp_page.goto(url).await?;
        
        match wait_strategy {
            WaitStrategy::Event(event) => {
                match event.as_str() {
                    "domcontentloaded" => {
                        self.cdp_page.wait_for_navigation().await?;
                    }
                    "networkidle" | "networkidle0" => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                    "networkidle2" => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                    "load" => {
                        self.cdp_page.wait_for_navigation().await?;
                    }
                    _ => {
                        return Err(BrowserError::ConfigError(format!(
                            "Unknown wait event: {}",
                            event
                        )));
                    }
                }
            }
            WaitStrategy::Delay { delay_ms } => {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }

        let title = self.cdp_page.get_title().await.ok().flatten();
        let final_url = self.cdp_page.url().await?.unwrap_or_else(|| url.to_string());
        
        let mut current_url = self.current_url.write().await;
        *current_url = Some(final_url.clone());

        Ok(GotoResult {
            url: final_url,
            title,
        })
    }

    pub async fn screenshot(&self, mode: ScreenshotMode) -> Result<Vec<Screenshot>> {
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
        debug!("Taking viewport screenshot");
        
        let format = match self.config.format {
            ImageFormat::Png => CaptureScreenshotFormat::Png,
            ImageFormat::Webp => CaptureScreenshotFormat::Webp,
        };

        let params = CaptureScreenshotParams::builder()
            .format(format)
            .build();

        let data = self.cdp_page.screenshot(params).await?;
        
        Ok(vec![Screenshot {
            data,
            width: self.config.viewport.width,
            height: self.config.viewport.height,
            format: self.config.format,
        }])
    }

    pub async fn screenshot_fullpage(&self, segments_max: usize) -> Result<Vec<Screenshot>> {
        debug!("Taking full page screenshot with max {} segments", segments_max);
        
        let viewport = &self.config.viewport;
        let overlap = 24;
        let scroll_height = viewport.height - overlap;
        
        let mut screenshots = Vec::new();
        let mut current_y = 0;

        for segment_idx in 0..segments_max {
            self.cdp_page
                .evaluate(format!("window.scrollTo(0, {})", current_y))
                .await?;
            
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let format = match self.config.format {
                ImageFormat::Png => CaptureScreenshotFormat::Png,
                ImageFormat::Webp => CaptureScreenshotFormat::Webp,
            };

            let params = CaptureScreenshotParams::builder()
                .format(format)
                .build();

            let data = self.cdp_page.screenshot(params).await?;
            
            screenshots.push(Screenshot {
                data,
                width: viewport.width,
                height: viewport.height,
                format: self.config.format,
            });

            let eval_result = self
                .cdp_page
                .evaluate("document.body.scrollHeight")
                .await?;
            
            let page_height: i64 = eval_result
                .value()
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            current_y += scroll_height as i64;
            
            if current_y >= page_height {
                debug!("Reached end of page after {} segments", segment_idx + 1);
                break;
            }
        }

        if screenshots.len() == segments_max {
            info!("[full page truncated at {} segments]", segments_max);
        }

        Ok(screenshots)
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

        let params = CaptureScreenshotParams::builder()
            .format(format)
            .clip(chromiumoxide::cdp::browser_protocol::page::Viewport {
                x: region.x as f64,
                y: region.y as f64,
                width: region.width as f64,
                height: region.height as f64,
                scale: 1.0,
            })
            .build();

        let data = self.cdp_page.screenshot(params).await?;
        
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
        let params = SetDeviceMetricsOverrideParams::builder()
            .width(viewport.width as i64)
            .height(viewport.height as i64)
            .device_scale_factor(viewport.device_scale_factor.unwrap_or(1.0))
            .mobile(viewport.mobile.unwrap_or(false))
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        
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

    pub async fn get_url(&self) -> Result<String> {
        let url_guard = self.current_url.read().await;
        url_guard
            .clone()
            .ok_or(BrowserError::PageNotLoaded)
    }

    /// Get the current URL directly from the browser (not cached)
    pub async fn get_current_url(&self) -> Result<String> {
        match self.cdp_page.url().await? {
            Some(url) => Ok(url),
            None => Err(BrowserError::PageNotLoaded)
        }
    }

    pub async fn update_viewport(&self, viewport: ViewportConfig) -> Result<()> {
        let params = SetDeviceMetricsOverrideParams::builder()
            .width(viewport.width as i64)
            .height(viewport.height as i64)
            .device_scale_factor(viewport.device_scale_factor)
            .mobile(viewport.mobile)
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        
        self.cdp_page.execute(params).await?;
        Ok(())
    }

    /// Click at the specified coordinates
    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        debug!("Clicking at ({}, {})", x, y);
        
        // Move to position
        let move_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseMoved)
            .x(x)
            .y(y)
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        self.cdp_page.execute(move_params).await?;
        
        // Mouse down
        let down_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(x)
            .y(y)
            .button(chromiumoxide::cdp::browser_protocol::input::MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        self.cdp_page.execute(down_params).await?;
        
        // Mouse up
        let up_params = DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(x)
            .y(y)
            .button(chromiumoxide::cdp::browser_protocol::input::MouseButton::Left)
            .click_count(1)
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        self.cdp_page.execute(up_params).await?;
        
        Ok(())
    }

    /// Type text into the currently focused element
    pub async fn type_text(&self, text: &str) -> Result<()> {
        debug!("Typing text: {}", text);
        
        for ch in text.chars() {
            let params = DispatchKeyEventParams::builder()
                .r#type(DispatchKeyEventType::Char)
                .text(ch.to_string())
                .build()
                .map_err(|e| BrowserError::CdpError(e))?;
            self.cdp_page.execute(params).await?;
        }
        
        Ok(())
    }

    /// Press a key (e.g., "Enter", "Tab", "Escape", "ArrowDown")
    pub async fn press_key(&self, key: &str) -> Result<()> {
        debug!("Pressing key: {}", key);
        
        // Key down
        let down_params = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyDown)
            .key(key.to_string())
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        self.cdp_page.execute(down_params).await?;
        
        // Key up
        let up_params = DispatchKeyEventParams::builder()
            .r#type(DispatchKeyEventType::KeyUp)
            .key(key.to_string())
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        self.cdp_page.execute(up_params).await?;
        
        Ok(())
    }

    /// Execute JavaScript code with enhanced return value handling
    pub async fn execute_javascript(&self, code: &str) -> Result<serde_json::Value> {
        debug!("Executing JavaScript: {}...", &code.chars().take(100).collect::<String>());
        
        // Helper to check if a line looks like an expression that should be returned
        let is_expression_line = |line: &str| -> bool {
            let line = line.trim();
            if line.is_empty() { return false; }
            
            // Skip lines that are already returns or start with control keywords
            let control_keywords = [
                "return", "const", "let", "var", "class", "function", "async",
                "if", "for", "while", "switch", "try", "catch", "finally", 
                "throw", "import", "export", "yield", "await", "break", 
                "continue", "debugger"
            ];
            
            for keyword in &control_keywords {
                if line.starts_with(keyword) && 
                   (line.len() == keyword.len() || 
                    line.chars().nth(keyword.len()).map_or(false, |c| !c.is_alphanumeric())) {
                    return false;
                }
            }
            
            // Skip lines that end with block closures
            if line.starts_with('}') { return false; }
            
            true
        };
        
        // Analyze the code
        let lines: Vec<&str> = code.split('\n').collect();
        let mut last_expr_line = None;
        let mut has_explicit_return = false;
        
        // Scan for significant lines from the end
        for (i, line) in lines.iter().enumerate().rev() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") { continue; }
            
            if line.starts_with("return") {
                has_explicit_return = true;
                break;
            }
            
            if last_expr_line.is_none() && is_expression_line(line) {
                last_expr_line = Some(i);
                if !line.ends_with(';') && !line.ends_with('}') {
                    break;
                }
            }
        }
        
        // Build the function body
        let body = if has_explicit_return {
            code.to_string()
        } else if let Some(expr_line_idx) = last_expr_line {
            let prefix = lines[..expr_line_idx].join("\n");
            let expr_line = lines[expr_line_idx].trim();
            let suffix = lines[expr_line_idx + 1..].join("\n");
            
            // Handle object literals and array literals - wrap in parentheses
            let wrapped_expr = if expr_line.starts_with('{') || expr_line.starts_with('[') {
                format!("({})", expr_line)
            } else {
                expr_line.to_string()
            };
            
            format!("{}\nreturn {};\n{}", prefix, wrapped_expr, suffix)
        } else {
            // Try to capture the last variable defined
            let var_regex = regex::Regex::new(r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=").unwrap();
            if let Some(captures) = var_regex.captures(code) {
                if let Some(var_name) = captures.get(1) {
                    format!("{}; return typeof {} !== 'undefined' ? {} : undefined;", 
                            code, var_name.as_str(), var_name.as_str())
                } else {
                    code.to_string()
                }
            } else {
                code.to_string()
            }
        };
        
        // Wrap in async IIFE with error handling and console.log capture
        let wrapped = format!(r#"
(async () => {{
    const __logs = [];
    const __origLog = console.log;
    
    function safe(arg) {{
        if (arg === null || typeof arg !== 'object') return String(arg);
        try {{ return JSON.stringify(arg); }} catch {{ return '[Circular]'; }}
    }}
    
    console.log = function(...args) {{
        __logs.push(args.map(safe).join(' '));
        __origLog.apply(console, args);
    }};
    
    try {{
        const AsyncFunction = Object.getPrototypeOf(async function(){{}}).constructor;
        const __userCode = {};
        const fn = new AsyncFunction(__userCode);
        const value = await fn();
        return {{
            success: true,
            value: (value === undefined && __logs.length > 0) ? __logs[__logs.length-1] : value,
            logs: __logs
        }};
    }} catch (err) {{
        return {{
            success: false,
            error: err.toString() + (err.stack ? '\n' + err.stack : ''),
            logs: __logs
        }};
    }} finally {{
        console.log = __origLog;
    }}
}})()
"#, serde_json::to_string(&body).unwrap());
        
        tracing::debug!("Executing JavaScript code: {}", code);
        tracing::debug!("Wrapped code: {}", wrapped);
        
        let result = self.cdp_page.evaluate(wrapped).await?;
        let result_value = result.value().cloned().unwrap_or(serde_json::Value::Null);
        
        tracing::info!("JavaScript execution result: {}", result_value);
        
        Ok(result_value)
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