//! Stock GstAudioSink scheduling with a bounded, epoch-owned CPAL writer.
//! Native resets outside an owner-initiated teardown fail the session closed.

use super::playback::PlaybackWriter;
use audio::prelude::*;
use audio::subclass::prelude::*;
use gst::glib;
use gstreamer as gst;
use gstreamer_audio as audio;
use std::sync::OnceLock;

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Sink {
        pub(super) writer: OnceLock<PlaybackWriter>,
        #[cfg(test)]
        first_event: OnceLock<&'static str>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Sink {
        const NAME: &'static str = "CodexPrivateAudioSink";
        type Type = super::Sink;
        type ParentType = audio::AudioSink;
    }
    impl ObjectImpl for Sink {}
    impl GstObjectImpl for Sink {}
    impl ElementImpl for Sink {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static META: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
            Some(META.get_or_init(|| {
                gst::subclass::ElementMetadata::new(
                    "Private audio",
                    "Sink/Audio",
                    "Owned speaker output",
                    "OpenAI",
                )
            }))
        }
        fn pad_templates() -> &'static [gst::PadTemplate] {
            static PADS: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
            PADS.get_or_init(|| {
                gst::PadTemplate::new(
                    "sink",
                    gst::PadDirection::Sink,
                    gst::PadPresence::Always,
                    &gst::Caps::builder("audio/x-raw")
                        .field("format", "F32LE")
                        .field("layout", "interleaved")
                        .field("channels", 1i32)
                        .field(
                            "rate",
                            gst::IntRange::<i32>::new(/*min*/ 8000, /*max*/ 384000),
                        )
                        .build(),
                )
                .into_iter()
                .collect()
            })
        }
    }
    impl BaseSinkImpl for Sink {}
    impl AudioBaseSinkImpl for Sink {}
    impl AudioSinkImpl for Sink {
        fn prepare(&self, spec: &mut audio::AudioRingBufferSpec) -> Result<(), gst::LoggableError> {
            let writer = self
                .writer
                .get()
                .ok_or_else(|| gst::loggable_error!(gst::CAT_RUST, "speaker not bound"))?;
            let info = spec.audio_info();
            if info.rate() != writer.rate()
                || info.channels() != 1
                || info.format() != audio::AudioFormat::F32le
                || info.layout() != audio::AudioLayout::Interleaved
            {
                return Err(gst::loggable_error!(
                    gst::CAT_RUST,
                    "unsupported speaker format"
                ));
            }
            spec.set_segsize((writer.rate() / 100 * 4) as i32);
            spec.set_segtotal(4);
            spec.set_latency_time(10_000);
            spec.set_buffer_time(40_000);
            Ok(())
        }
        fn unprepare(&self) -> Result<(), gst::LoggableError> {
            Ok(())
        }
        fn write(&self, bytes: &[u8]) -> Result<i32, gst::LoggableError> {
            let writer = self
                .writer
                .get()
                .ok_or_else(|| gst::loggable_error!(gst::CAT_RUST, "speaker not bound"))?;
            writer.write(bytes).map(|n| n as i32).map_err(|_error| {
                #[cfg(test)]
                self.first_event.get_or_init(|| {
                    eprintln!("native sink first event: {_error}");
                    _error
                });
                writer.fail_if_current();
                gst::loggable_error!(gst::CAT_RUST, "speaker write failed")
            })
        }
        fn delay(&self) -> u32 {
            self.writer.get().map_or(0, PlaybackWriter::delay)
        }
        fn reset(&self) {
            if let Some(writer) = self.writer.get() {
                #[cfg(test)]
                self.first_event.get_or_init(|| {
                    eprintln!("native sink first event: reset");
                    "reset"
                });
                writer.fail_if_current();
            }
        }
    }
}

glib::wrapper! {
    pub struct Sink(ObjectSubclass<imp::Sink>) @extends audio::AudioSink, audio::AudioBaseSink, audio::gst_base::BaseSink, gst::Element, gst::Object;
}

impl Sink {
    pub(super) fn new(writer: PlaybackWriter) -> Self {
        let sink: Self = glib::Object::new();
        assert!(sink.imp().writer.set(writer).is_ok());
        sink.set_provide_clock(false);
        sink.set_property_from_str("slave-method", "none");
        sink.set_sync(false);
        sink.set_async(false);
        sink.set_enable_last_sample(false);
        sink
    }
}
