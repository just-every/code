use crate::{
    assets::{AssetManager, ImageRef},
    manager::BrowserManager,
    page::ScreenshotMode,
    Result,
};
use std::sync::Arc;
use tracing::{debug, info};

pub struct BrowserInjector {
    manager: Arc<BrowserManager>,
    asset_manager: Arc<AssetManager>,
}

impl BrowserInjector {
    pub fn new(manager: Arc<BrowserManager>, asset_manager: Arc<AssetManager>) -> Self {
        Self {
            manager,
            asset_manager,
        }
    }

    pub async fn inject_pre_llm_call(&self) -> Result<Option<InjectionResult>> {
        if !self.manager.is_enabled().await {
            return Ok(None);
        }

        debug!("Browser enabled, capturing pre-LLM screenshot");
        
        let page = match self.manager.get_or_create_page().await {
            Ok(p) => p,
            Err(_) => {
                info!("No active page, using about:blank");
                let page = self.manager.get_or_create_page().await?;
                page.goto("about:blank", None).await?;
                page
            }
        };

        let config = self.manager.get_config().await;
        let mode = if config.fullpage {
            ScreenshotMode::FullPage {
                segments_max: Some(config.segments_max),
            }
        } else {
            ScreenshotMode::Viewport
        };

        let screenshots = page.screenshot(mode).await?;
        let ttl_ms = 300000;
        let images = self.asset_manager.store_screenshots(screenshots, ttl_ms).await?;

        let current_url = page.get_current_url().await.unwrap_or_else(|_| "about:blank".to_string());
        
        let system_hint = format!(
            "A fresh screenshot of the active page ({}) is attached; use browser.* tools to navigate or capture more.",
            current_url
        );

        let segments_captured = images.len();
        
        Ok(Some(InjectionResult {
            images,
            system_hint,
            metadata: InjectionMetadata {
                url: current_url,
                fullpage: config.fullpage,
                segments_captured,
            },
        }))
    }

    pub async fn cleanup_expired(&self) -> Result<()> {
        self.asset_manager.cleanup_expired().await
    }
}

#[derive(Debug)]
pub struct InjectionResult {
    pub images: Vec<ImageRef>,
    pub system_hint: String,
    pub metadata: InjectionMetadata,
}

#[derive(Debug)]
pub struct InjectionMetadata {
    pub url: String,
    pub fullpage: bool,
    pub segments_captured: usize,
}