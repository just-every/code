use futures::channel::{
    mpsc,
    oneshot::{self, channel as oneshot_channel},
};
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::handler::target::TargetMessage;
use crate::{error::Result, ArcHttpRequest};

type TargetSender = mpsc::Sender<TargetMessage>;

pin_project! {
    pub struct TargetMessageFuture<T> {
        #[pin]
        rx_request: oneshot::Receiver<T>,
        #[pin]
        target_sender: mpsc::Sender<TargetMessage>,

        message: Option<TargetMessage>,
    }
}

impl<T> TargetMessageFuture<T> {
    pub fn new(
        target_sender: TargetSender,
        message: TargetMessage,
        rx_request: oneshot::Receiver<T>,
    ) -> Self {
        Self {
            target_sender,
            rx_request,
            message: Some(message),
        }
    }

    pub fn wait_for_navigation(target_sender: TargetSender) -> TargetMessageFuture<ArcHttpRequest> {
        let (tx, rx_request) = oneshot_channel();

        let message = TargetMessage::WaitForNavigation(tx);

        TargetMessageFuture::new(target_sender, message, rx_request)
    }
}

impl<T> Future for TargetMessageFuture<T> {
    type Output = Result<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        if this.message.is_some() {
            match this.target_sender.poll_ready(cx) {
                Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                Poll::Ready(Ok(_)) => {
                    let message = this.message.take().expect("existence checked above");
                    this.target_sender.start_send(message)?;

                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            this.rx_request.as_mut().poll(cx).map_err(Into::into)
        }
    }
}
