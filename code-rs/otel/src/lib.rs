pub mod config;

pub mod otel_event_manager;
#[cfg(feature = "otel")]
pub mod otel_provider;

#[cfg(not(feature = "otel"))]
mod imp {
    use reqwest::header::HeaderMap;
    use tracing::Span;
    use tracing_subscriber::Layer;
    use tracing_subscriber::registry::LookupSpan;

    pub struct OtelProvider;

    impl OtelProvider {
        pub fn from(_settings: &crate::config::OtelSettings) -> Option<Self> {
            None
        }

        pub fn headers(_span: &Span) -> HeaderMap {
            HeaderMap::new()
        }

        pub fn logger_layer<S>(&self) -> Option<impl Layer<S> + Send + Sync>
        where
            S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
        {
            None::<tracing_subscriber::fmt::Layer<S>>
        }
    }
}

#[cfg(not(feature = "otel"))]
pub use imp::OtelProvider;
