use std::task::Poll;

use futures::{stream::BoxStream, StreamExt as _};
use nats_codec::{Message, Unsub};
use tokio::sync::mpsc;

use crate::ConnectionCommand;

pub struct Subscriber {
    sid: String,
    messages: BoxStream<'static, Message>,

    connection_chan: mpsc::Sender<ConnectionCommand>,
}

impl std::fmt::Debug for Subscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscriber")
            .field("sid", &self.sid)
            .field("connection_chan", &self.connection_chan)
            .finish()
    }
}

impl Subscriber {
    pub fn new(
        sid: String,
        messages: BoxStream<'static, Message>,
        connection_chan: mpsc::Sender<ConnectionCommand>,
    ) -> Self {
        Self {
            sid,
            connection_chan,
            messages,
        }
    }

    pub fn sid(&self) -> &String {
        &self.sid
    }
}

#[derive(Debug, Clone)]
pub struct SubscriptionOptions {
    pub max_msgs: Option<usize>,
    pub queue_group: Option<String>,
}

impl std::default::Default for SubscriptionOptions {
    fn default() -> Self {
        Self {
            max_msgs: Default::default(),
            queue_group: Default::default(),
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        let sender = self.connection_chan.clone();
        let sid = self.sid.clone();

        tokio::spawn({
            async move {
                sender
                    .send(ConnectionCommand::Unsubscribe(Unsub {
                        sid: sid.to_string(),
                        max_msgs: None,
                    }))
                    .await
                    .unwrap()
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
