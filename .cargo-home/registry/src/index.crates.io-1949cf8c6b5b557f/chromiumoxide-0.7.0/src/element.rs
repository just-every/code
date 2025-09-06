use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{future, Future, FutureExt, Stream};

use chromiumoxide_cdp::cdp::browser_protocol::dom::{
    BackendNodeId, DescribeNodeParams, GetBoxModelParams, GetContentQuadsParams, Node, NodeId,
    ResolveNodeParams,
};
use chromiumoxide_cdp::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams, Viewport,
};
use chromiumoxide_cdp::cdp::js_protocol::runtime::{
    CallFunctionOnReturns, GetPropertiesParams, PropertyDescriptor, RemoteObjectId,
    RemoteObjectType,
};

use crate::error::{CdpError, Result};
use crate::handler::PageInner;
use crate::layout::{BoundingBox, BoxModel, ElementQuad, Point};
use crate::utils;

/// Represents a [DOM Element](https://developer.mozilla.org/en-US/docs/Web/API/Element).
#[derive(Debug)]
pub struct Element {
    /// The Unique object identifier
    pub remote_object_id: RemoteObjectId,
    /// Identifier of the backend node.
    pub backend_node_id: BackendNodeId,
    /// The identifier of the node this element represents.
    pub node_id: NodeId,
    tab: Arc<PageInner>,
}

impl Element {
    pub(crate) async fn new(tab: Arc<PageInner>, node_id: NodeId) -> Result<Self> {
        let backend_node_id = tab
            .execute(
                DescribeNodeParams::builder()
                    .node_id(node_id)
                    .depth(100)
                    .build(),
            )
            .await?
            .node
            .backend_node_id;

        let resp = tab
            .execute(
                ResolveNodeParams::builder()
                    .backend_node_id(backend_node_id)
                    .build(),
            )
            .await?;

        let remote_object_id = resp
            .result
            .object
            .object_id
            .ok_or_else(|| CdpError::msg(format!("No object Id found for {node_id:?}")))?;
        Ok(Self {
            remote_object_id,
            backend_node_id,
            node_id,
            tab,
        })
    }

    /// Convert a slice of `NodeId`s into a `Vec` of `Element`s
    pub(crate) async fn from_nodes(tab: &Arc<PageInner>, node_ids: &[NodeId]) -> Result<Vec<Self>> {
        future::join_all(
            node_ids
                .iter()
                .copied()
                .map(|id| Element::new(Arc::clone(tab), id)),
        )
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
    }

    /// Returns the first element in the document which matches the given CSS
    /// selector.
    pub async fn find_element(&self, selector: impl Into<String>) -> Result<Self> {
        let node_id = self.tab.find_element(selector, self.node_id).await?;
        Element::new(Arc::clone(&self.tab), node_id).await
    }

    /// Return all `Element`s in the document that match the given selector
    pub async fn find_elements(&self, selector: impl Into<String>) -> Result<Vec<Element>> {
        Element::from_nodes(
            &self.tab,
            &self.tab.find_elements(selector, self.node_id).await?,
        )
        .await
    }

    async fn box_model(&self) -> Result<BoxModel> {
        let model = self
            .tab
            .execute(
                GetBoxModelParams::builder()
                    .backend_node_id(self.backend_node_id)
                    .build(),
            )
            .await?
            .result
            .model;
        Ok(BoxModel {
            content: ElementQuad::from_quad(&model.content),
            padding: ElementQuad::from_quad(&model.padding),
            border: ElementQuad::from_quad(&model.border),
            margin: ElementQuad::from_quad(&model.margin),
            width: model.width as u32,
            height: model.height as u32,
        })
    }

    /// Returns the bounding box of the element (relative to the main frame)
    pub async fn bounding_box(&self) -> Result<BoundingBox> {
        let bounds = self.box_model().await?;
        let quad = bounds.border;

        let x = quad.most_left();
        let y = quad.most_top();
        let width = quad.most_right() - x;
        let height = quad.most_bottom() - y;

        Ok(BoundingBox {
            x,
            y,
            width,
            height,
        })
    }

    /// Returns the best `Point` of this node to execute a click on.
    pub async fn clickable_point(&self) -> Result<Point> {
        let content_quads = self
            .tab
            .execute(
                GetContentQuadsParams::builder()
                    .backend_node_id(self.backend_node_id)
                    .build(),
            )
            .await?;
        content_quads
            .quads
            .iter()
            .filter(|q| q.inner().len() == 8)
            .map(ElementQuad::from_quad)
            .filter(|q| q.quad_area() > 1.)
            .map(|q| q.quad_center())
            .next()
            .ok_or_else(|| CdpError::msg("Node is either not visible or not an HTMLElement"))
    }

    /// Submits a javascript function to the page and returns the evaluated
    /// result
    ///
    /// # Example get the element as JSON object
    ///
    /// ```no_run
    /// # use chromiumoxide::element::Element;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(element: Element) -> Result<()> {
    ///     let js_fn = "function() { return this; }";
    ///     let element_json = element.call_js_fn(js_fn, false).await?;
    ///     # Ok(())
    /// # }
    /// ```
    ///
    /// # Execute an async javascript function
    ///
    /// ```no_run
    /// # use chromiumoxide::element::Element;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(element: Element) -> Result<()> {
    ///     let js_fn = "async function() { return this; }";
    ///     let element_json = element.call_js_fn(js_fn, true).await?;
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn call_js_fn(
        &self,
        function_declaration: impl Into<String>,
        await_promise: bool,
    ) -> Result<CallFunctionOnReturns> {
        self.tab
            .call_js_fn(
                function_declaration,
                await_promise,
                self.remote_object_id.clone(),
            )
            .await
    }

    /// Returns a JSON representation of this element.
    pub async fn json_value(&self) -> Result<serde_json::Value> {
        let element_json = self
            .call_js_fn("function() { return this; }", false)
            .await?;
        element_json.result.value.ok_or(CdpError::NotFound)
    }

    /// Calls [focus](https://developer.mozilla.org/en-US/docs/Web/API/HTMLElement/focus) on the element.
    pub async fn focus(&self) -> Result<&Self> {
        self.call_js_fn("function() { this.focus(); }", true)
            .await?;
        Ok(self)
    }

    /// Scrolls the element into view and uses a mouse event to move the mouse
    /// over the center of this element.
    pub async fn hover(&self) -> Result<&Self> {
        self.scroll_into_view().await?;
        self.tab.move_mouse(self.clickable_point().await?).await?;
        Ok(self)
    }

    /// Scrolls the element into view.
    ///
    /// Fails if the element's node is not a HTML element or is detached from
    /// the document
    pub async fn scroll_into_view(&self) -> Result<&Self> {
        let resp = self
            .call_js_fn(
                "async function() {
                if (!this.isConnected)
                    return 'Node is detached from document';
                if (this.nodeType !== Node.ELEMENT_NODE)
                    return 'Node is not of type HTMLElement';

                const visibleRatio = await new Promise(resolve => {
                    const observer = new IntersectionObserver(entries => {
                        resolve(entries[0].intersectionRatio);
                        observer.disconnect();
                    });
                    observer.observe(this);
                });

                if (visibleRatio !== 1.0)
                    this.scrollIntoView({
                        block: 'center',
                        inline: 'center',
                        behavior: 'instant'
                    });
                return false;
            }",
                true,
            )
            .await?;

        if resp.result.r#type == RemoteObjectType::String {
            let error_text = resp.result.value.unwrap().as_str().unwrap().to_string();
            return Err(CdpError::ScrollingFailed(error_text));
        }
        Ok(self)
    }

    /// This focuses the element by click on it
    ///
    /// Bear in mind that if `click()` triggers a navigation this element may be
    /// not exist anymore.
    pub async fn click(&self) -> Result<&Self> {
        let center = self.scroll_into_view().await?.clickable_point().await?;
        self.tab.click(center).await?;
        Ok(self)
    }

    /// Type the input
    ///
    /// # Example type text into an input element
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let element = page.find_element("input#searchInput").await?;
    ///     element.click().await?.type_str("this goes into the input field").await?;
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn type_str(&self, input: impl AsRef<str>) -> Result<&Self> {
        self.tab.type_str(input).await?;
        Ok(self)
    }

    /// Presses the key.
    ///
    /// # Example type text into an input element and hit enter
    ///
    /// ```no_run
    /// # use chromiumoxide::page::Page;
    /// # use chromiumoxide::error::Result;
    /// # async fn demo(page: Page) -> Result<()> {
    ///     let element = page.find_element("input#searchInput").await?;
    ///     element.click().await?.type_str("this goes into the input field").await?
    ///          .press_key("Enter").await?;
    ///     # Ok(())
    /// # }
    /// ```
    pub async fn press_key(&self, key: impl AsRef<str>) -> Result<&Self> {
        self.tab.press_key(key).await?;
        Ok(self)
    }

    /// The description of the element's node
    pub async fn description(&self) -> Result<Node> {
        Ok(self
            .tab
            .execute(
                DescribeNodeParams::builder()
                    .backend_node_id(self.backend_node_id)
                    .depth(100)
                    .build(),
            )
            .await?
            .result
            .node)
    }

    /// Attributes of the `Element` node in the form of flat array `[name1,
    /// value1, name2, value2]
    pub async fn attributes(&self) -> Result<Vec<String>> {
        let node = self.description().await?;
        Ok(node.attributes.unwrap_or_default())
    }

    /// Returns the value of the element's attribute
    pub async fn attribute(&self, attribute: impl AsRef<str>) -> Result<Option<String>> {
        let js_fn = format!(
            "function() {{ return this.getAttribute('{}'); }}",
            attribute.as_ref()
        );
        let resp = self.call_js_fn(js_fn, false).await?;
        if let Some(value) = resp.result.value {
            Ok(serde_json::from_value(value)?)
        } else {
            Ok(None)
        }
    }

    /// A `Stream` over all attributes and their values
    pub async fn iter_attributes(
        &self,
    ) -> Result<impl Stream<Item = (String, Result<Option<String>>)> + '_> {
        let attributes = self.attributes().await?;
        Ok(AttributeStream {
            attributes,
            fut: None,
            element: self,
        })
    }

    /// The inner text of this element.
    pub async fn inner_text(&self) -> Result<Option<String>> {
        self.string_property("innerText").await
    }

    /// The inner HTML of this element.
    pub async fn inner_html(&self) -> Result<Option<String>> {
        self.string_property("innerHTML").await
    }

    /// The outer HTML of this element.
    pub async fn outer_html(&self) -> Result<Option<String>> {
        self.string_property("outerHTML").await
    }

    /// Returns the string property of the element.
    ///
    /// If the property is an empty String, `None` is returned.
    pub async fn string_property(&self, property: impl AsRef<str>) -> Result<Option<String>> {
        let property = property.as_ref();
        let value = self.property(property).await?.ok_or(CdpError::NotFound)?;
        let txt: String = serde_json::from_value(value)?;
        if !txt.is_empty() {
            Ok(Some(txt))
        } else {
            Ok(None)
        }
    }

    /// Returns the javascript `property` of this element where `property` is
    /// the name of the requested property of this element.
    ///
    /// See also `Element::inner_html`.
    pub async fn property(&self, property: impl AsRef<str>) -> Result<Option<serde_json::Value>> {
        let js_fn = format!("function() {{ return this.{}; }}", property.as_ref());
        let resp = self.call_js_fn(js_fn, false).await?;
        Ok(resp.result.value)
    }

    /// Returns a map with all `PropertyDescriptor`s of this element keyed by
    /// their names
    pub async fn properties(&self) -> Result<HashMap<String, PropertyDescriptor>> {
        let mut params = GetPropertiesParams::new(self.remote_object_id.clone());
        params.own_properties = Some(true);

        let properties = self.tab.execute(params).await?;

        Ok(properties
            .result
            .result
            .into_iter()
            .map(|p| (p.name.clone(), p))
            .collect())
    }

    /// Scrolls the element into and takes a screenshot of it
    pub async fn screenshot(&self, format: CaptureScreenshotFormat) -> Result<Vec<u8>> {
        let mut bounding_box = self.scroll_into_view().await?.bounding_box().await?;
        let viewport = self.tab.layout_metrics().await?.css_layout_viewport;

        bounding_box.x += viewport.page_x as f64;
        bounding_box.y += viewport.page_y as f64;

        let clip = Viewport {
            x: viewport.page_x as f64 + bounding_box.x,
            y: viewport.page_y as f64 + bounding_box.y,
            width: bounding_box.width,
            height: bounding_box.height,
            scale: 1.,
        };

        self.tab
            .screenshot(
                CaptureScreenshotParams::builder()
                    .format(format)
                    .clip(clip)
                    .build(),
            )
            .await
    }

    /// Save a screenshot of the element and write it to `output`
    pub async fn save_screenshot(
        &self,
        format: CaptureScreenshotFormat,
        output: impl AsRef<Path>,
    ) -> Result<Vec<u8>> {
        let img = self.screenshot(format).await?;
        utils::write(output.as_ref(), &img).await?;
        Ok(img)
    }
}

pub type AttributeValueFuture<'a> = Option<(
    String,
    Pin<Box<dyn Future<Output = Result<Option<String>>> + 'a>>,
)>;

/// Stream over all element's attributes
#[must_use = "streams do nothing unless polled"]
#[allow(missing_debug_implementations)]
pub struct AttributeStream<'a> {
    attributes: Vec<String>,
    fut: AttributeValueFuture<'a>,
    element: &'a Element,
}

impl<'a> Stream for AttributeStream<'a> {
    type Item = (String, Result<Option<String>>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pin = self.get_mut();

        if pin.fut.is_none() {
            if let Some(name) = pin.attributes.pop() {
                let fut = Box::pin(pin.element.attribute(name.clone()));
                pin.fut = Some((name, fut));
            } else {
                return Poll::Ready(None);
            }
        }

        if let Some((name, mut fut)) = pin.fut.take() {
            if let Poll::Ready(res) = fut.poll_unpin(cx) {
                return Poll::Ready(Some((name, res)));
            } else {
                pin.fut = Some((name, fut));
            }
        }
        Poll::Pending
    }
}
