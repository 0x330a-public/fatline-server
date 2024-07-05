use std::any::Any;
use std::sync::Arc;
use crossbeam::channel::Sender;
use fatline_rs::HubService;
use fatline_rs::proto::{HubEvent, HubEventType, Message, MessageData, MessageType, on_chain_event, OnChainEvent, OnChainEventType, SignerEventType, SubscribeRequest};
use fatline_rs::proto::hub_event::Body;
use fatline_rs::proto::message_data::Body as MBody;
use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tracing::{debug, error, trace};
use crate::service::ServiceState;
use crate::user_models::Signer;
use crate::worker::Task;

pub struct Subscriber {
    handle: JoinHandle<()>,
}

pub(crate) fn signer_from_event(event: &OnChainEvent) -> Option<Signer> {
    if event.r#type() == OnChainEventType::EventTypeSigner {
        if let Some(on_chain_event::Body::SignerEventBody(signer_body)) = &event.body {
            Some(Signer {
                pk: signer_body.key.clone(),
                fid: event.fid as i64,
                active: signer_body.event_type() == SignerEventType::Add
            })
        } else {
            None
        }
    } else {
        None
    }
}

fn handle_merge_message(message: Message, sender: &Sender<Task>) {

    let data = message.data.unwrap_or_default();

    if let Some(body) = data.body {
        match body {
            MBody::CastAddBody(_) => {}
            MBody::CastRemoveBody(_) => {}
            MBody::ReactionBody(_) => {}
            MBody::VerificationAddAddressBody(_) => {}
            MBody::VerificationRemoveBody(_) => {}
            MBody::UserDataBody(_user_data) => {
                // process actual user_data and insert into DB here instead of queuing the index task
                let _ = sender.send(Task::IndexFid(data.fid, true));
            }
            MBody::LinkBody(_) => {}
            MBody::UsernameProofBody(_) => {}
            MBody::FrameActionBody(_) => {}
            MBody::LinkCompactStateBody(_) => {}
        }
    }
}

async fn subscribe(mut hub_client: HubService, sender: Sender<Task>) {
    let subscription_response = hub_client.subscribe(SubscribeRequest::default())
        .await
        .expect("Couldn't build subscription");

    let mut sub = subscription_response.into_inner();
    while let Some(Ok(message)) = sub.next().await {
        trace!("received subscription message: {message:?}");
        match &message.r#type() {
            HubEventType::None => {}
            HubEventType::MergeMessage => {
                if let Some(Body::MergeMessageBody(body)) = message.body {
                    if let Some(message) = body.message {
                        handle_merge_message(message, &sender);
                    }
                }
            }
            HubEventType::PruneMessage => {
                // handle updated prunes
            }
            HubEventType::RevokeMessage => {
                // handle revokes
            }
            HubEventType::MergeUsernameProof => {
                // probably don't bother with this
            }
            HubEventType::MergeOnChainEvent => {
                if let Some(Body::MergeOnChainEventBody(body)) = message.body {
                    body.on_chain_event.map(|event| {
                        if let Some(signer) = signer_from_event(&event) {
                            let _ = sender.send(Task::UpdateSigner(signer));
                        }
                    });
                };
            }
        }
    }
}

impl Subscriber {
    pub async fn new(sender: Sender<Task>) -> Self {
        let hub_client = ServiceState::hub_client().await;
        let handle = tokio::spawn(subscribe(hub_client, sender));
        Self {
            handle
        }
    }

    pub fn cancel(&self) {
        self.handle.abort();
    }
}
