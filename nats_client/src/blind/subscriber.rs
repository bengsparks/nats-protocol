use std::{num::NonZeroUsize, task::Poll};

use futures::{stream::BoxStream, StreamExt as _};
use nats_codec::{Message, Unsubscribe};
use tokio::sync::mpsc;

use crate::blind::ConnectionCommand;

pub struct Subscriber {
    sid: String,
    messages: BoxStream<'static, Message>,

    conn_chan: mpsc::Sender<ConnectionCommand>,
}

impl std::fmt::Debug for Subscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscriber")
            .field("sid", &self.sid)
            .field("conn_chan", &self.conn_chan)
            .finish()
    }
}

impl Subscriber {
    pub fn new(
        sid: String,
        messages: BoxStream<'static, Message>,
        conn_chan: mpsc::Sender<ConnectionCommand>,
    ) -> Self {
        Self {
            sid,
            conn_chan,
            messages,
        }
    }

    pub fn sid(&self) -> &String {
        &self.sid
    }
}

#[derive(Debug, Default, Clone)]
pub struct SubscriptionOptions {
    pub max_msgs: Option<NonZeroUsize>,
    pub queue_group: Option<String>,
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        let sender = self.conn_chan.clone();
        let sid = self.sid.clone();

        tokio::spawn({
            async move {
                let _ = sender
                    .send(ConnectionCommand::Unsubscribe(Unsubscribe {
                        sid: sid.to_string(),
                        max_msgs: None,
                    }))
                    .await;
            }
        });
    }
}

impl futures::Stream for Subscriber {
    type Item = Message;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.messages.poll_next_unpin(cx)
    }
}