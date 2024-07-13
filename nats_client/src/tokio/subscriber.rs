use std::task::Poll;

use futures::{stream::BoxStream, StreamExt as _};
use nats_codec::Message;
use nats_sans_io::ConnectionCommand;
use tokio::sync::mpsc;

pub struct Subscriber {
    pub sid: String,
    pub messages: BoxStream<'static, nats_codec::Message>,
    pub conn_chan: mpsc::Sender<nats_sans_io::ConnectionCommand>,
}

impl std::fmt::Debug for Subscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscriber")
            .field("sid", &self.sid)
            .field("conn_chan", &self.conn_chan)
            .finish()
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        let sender = self.conn_chan.clone();
        let sid = self.sid.clone();

        sender
            .try_send(ConnectionCommand::Unsubscribe {
                sid: sid.to_string(),
                max_msgs: None,
            })
            .unwrap();
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
