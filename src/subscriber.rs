use std::any::Any;
use std::sync::Arc;
use crossbeam::channel::Sender;
use fatline_rs::HubService;
use fatline_rs::proto::{HubEvent, HubEventType, MessageType, on_chain_event, OnChainEventType, SignerEventType, SubscribeRequest};
use fatline_rs::proto::hub_event::Body;
use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tracing::{debug, error, trace};
use crate::service::ServiceState;
use crate::user_models::Signer;
use crate::worker::Task;

pub struct Subscriber {
    handle: JoinHandle<()>,
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
                match message.body {
                    Some(Body::MergeOnChainEventBody(body)) => {
                        body.on_chain_event.map(|event| {
                            if event.r#type() == OnChainEventType::EventTypeSigner {
                                // task add signer here
                                match event.body {
                                    Some(on_chain_event::Body::SignerEventBody(signer_body)) => {
                                        trace!("Handling signer event {:?}", &signer_body);
                                        let signer = Signer {
                                            pk: signer_body.key.clone(),
                                            fid: event.fid as i64,
                                            active: signer_body.event_type() == SignerEventType::Add
                                        };
                                        match sender.send(Task::UpdateSigner(signer)) {
                                            Err(e) => {
                                                error!("Error scheduling signer update for task {e:?}");
                                            },
                                            _ => {
                                                // sent update signer request successfully, expected path
                                            }
                                        };
                                    }
                                    _ => {} // disregard body that isn't a signer event body on signer event type
                                };
                            } else {
                                // on chain event was a different type than event type signer
                                trace!("Not handling event of type: {}", event.r#type().as_str_name());
                            }
                        });
                    },
                    _ => {}
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
