//! Widget that separates resize+encode from rendering.
//! This allows for rendering to be non-blocking, offloading resize+encode into another thread.
//! See examples/async.rs for how to setup the threads and channels.
//! At least one worker thread for resize+encode is required, the example shows how to combine
//! the needs-resize-polling with other terminal events into one event loop.

use std::sync::mpsc::Sender;

use image::Rgba;
use ratatui::prelude::{Buffer, Rect};

use crate::{
    errors::Errors,
    protocol::{StatefulProtocol, StatefulProtocolType},
    Resize, ResizeEncodeRender,
};

/// The only usage of this struct is to call `perform()` on it and pass the completed resize to `ThreadProtocols` `update_protocol()`
pub struct ResizeRequest {
    protocol: StatefulProtocol,
    resize: Resize,
    area: Rect,
    id: u64,
}

impl ResizeRequest {
    pub fn resize_encode(mut self) -> Result<ResizeResponse, Errors> {
        self.protocol.resize_encode(&self.resize, self.area);
        self.protocol
            .last_encoding_result()
            .expect("The resize has just been performed")?;
        Ok(ResizeResponse {
            protocol: self.protocol,
            id: self.id,
        })
    }
}

/// The only usage of this struct is to pass it to `ThreadProtocols` `update_resize_protocol()`
pub struct ResizeResponse {
    protocol: StatefulProtocol,
    id: u64,
}

/// The state of a ThreadImage.
///
/// Has `inner` [StatefulProtocol] and sents requests through the mspc channel to do the
/// `resize_encode()` work.
pub struct ThreadProtocol {
    inner: Option<StatefulProtocol>,
    tx: Sender<ResizeRequest>,
    id: u64,
}

impl ThreadProtocol {
    pub fn new(tx: Sender<ResizeRequest>, inner: Option<StatefulProtocol>) -> ThreadProtocol {
        Self { inner, tx, id: 0 }
    }

    pub fn replace_protocol(&mut self, proto: StatefulProtocol) {
        self.inner = Some(proto);
        self.increment_id();
    }

    pub fn protocol_type(&self) -> Option<&StatefulProtocolType> {
        self.inner.as_ref().map(|inner| inner.protocol_type())
    }

    pub fn protocol_type_owned(self) -> Option<StatefulProtocolType> {
        self.inner.map(|inner| inner.protocol_type_owned())
    }

    // Get the background color that fills in when resizing.
    pub fn background_color(&self) -> Option<Rgba<u8>> {
        self.inner.as_ref().map(|inner| inner.background_color())
    }

    /// This function should be used when an image should be updated but the updated image is not yet available
    pub fn empty_protocol(&mut self) {
        self.inner = None;
        self.increment_id();
    }

    pub fn update_resized_protocol(&mut self, completed: ResizeResponse) -> bool {
        let equal = self.id == completed.id;
        if equal {
            self.inner = Some(completed.protocol)
        }
        equal
    }

    pub fn size_for(&self, resize: Resize, area: Rect) -> Option<Rect> {
        self.inner
            .as_ref()
            .map(|protocol| protocol.size_for(resize, area))
    }

    fn increment_id(&mut self) {
        self.id = self.id.wrapping_add(1);
    }
}

impl ResizeEncodeRender for ThreadProtocol {
    fn needs_resize(&self, resize: &Resize, area: Rect) -> Option<Rect> {
        self.inner
            .as_ref()
            .and_then(|protocol| protocol.needs_resize(resize, area))
    }

    /// Senda a `ResizeRequest` through the channel if there already isn't a pending `ResizeRequest`
    fn resize_encode(&mut self, resize: &Resize, area: Rect) {
        let _ = self.inner.take().map(|protocol| {
            self.increment_id();
            let _ = self.tx.send(ResizeRequest {
                protocol,
                resize: resize.clone(),
                area,
                id: self.id,
            });
        });
    }

    /// Render the currently resized and encoded data to the buffer, if there isn't a pending `ResizeRequest`
    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let _ = self
            .inner
            .as_mut()
            .map(|protocol| protocol.render(area, buf));
    }
}
