use std::num::NonZeroUsize;

use tokio::sync::mpsc;

#[derive(Debug, Default, Clone)]
pub struct SubscriptionOptions {
    pub max_msgs: Option<NonZeroUsize>,
    pub queue_group: Option<String>,
}

#[derive(Debug)]
pub struct SubscribeResponse {
    pub sid: String,
    pub max_msgs: Option<NonZeroUsize>,

    pub msg_chan: mpsc::Receiver<nats_codec::Message>,
}
