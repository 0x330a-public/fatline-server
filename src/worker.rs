use std::any::Any;
use std::future::Future;
use std::ops::{Add, Deref};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crossbeam::atomic::AtomicCell;
use crossbeam::channel::{Receiver, after};
use crossbeam::{scope, select};
use dashmap::DashMap;
use fatline_rs::posts::CastId;
use tokio::sync::Mutex;
use tracing::{debug, error, Level, span, warn};
use tokio::task::JoinHandle;
use tokio::time::{Interval, interval};
use crate::service::ServiceState;
use crate::ServiceArcState;
use crate::signer_repo::SignerRepository;
use crate::user_models::Signer;
use crate::user_repo::UserRepository;

#[derive(Debug,Hash,Eq,PartialEq,Clone)]
pub enum Task {
    IndexFid(u64),
    IndexLinks(u64),
    IndexReactions(CastId),
    IndexCast(CastId),
    UpdateSigner(Signer)
}

pub struct Worker {
    handle: JoinHandle<()>
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/**
* returns the time to insert as "latest" and whether to schedule based on the last and gap supplied.
*/
fn should_schedule(last: u64, gap: usize) -> (u64, bool) {
    let now = now();
    (now, now > Duration::from_secs(last).add(Duration::from_secs(gap as u64)).as_secs())
}

async fn index_fid(fid: u64, service_state: Arc<Mutex<ServiceState>>) {
    let mut state = service_state.lock().await;
    match state.get_user_profile(fid).await {
        Ok(p) => {
            debug!("Successfully indexed fid profile for {}", p.username.unwrap_or("user".to_string()));
        }
        Err(e) => {
            error!("Error indexing profile {e}");
        }
    };
}

async fn handle_signer_event(signer: Signer, service_state: Arc<Mutex<ServiceState>>) {
    let mut state = service_state.lock().await;
    let insert_result = state.insert_signer(signer).await;
    match insert_result {
        Ok(r) => {
            // successfully stored signer
            debug!("Successfully stored signer for fid {}", r.fid);
        },
        Err(e) => {
            error!("Error saving signer {e}");
        }
    }
}

const ONE_MINUTE: usize = 60;

async fn schedule_task(task: Task, service_state: Arc<Mutex<ServiceState>>, index_map: Arc<DashMap<Task, u64>>, last: Option<u64>) {
    let last_call = last.unwrap_or(0);
    match task.clone() {
        Task::UpdateSigner(signer_event) => {
            debug!("kicking off signer event for {:?}", signer_event.fid);
            handle_signer_event(signer_event.clone(), service_state.clone()).await;
        },
        Task::IndexFid(fid) => {
            // do check
            let (now, should_schedule) = should_schedule(last_call, ONE_MINUTE * 60);
            if should_schedule {
                // kick off job
                debug!("kicking off index for fid {fid}");
                tokio::spawn(index_fid(fid.to_owned(), service_state.clone()));
                index_map.insert(task, now);
            } else {
                debug!("skipping task {:?}", &task);
            }
        }
        _ => {
            // handle different types
        }
    }
}

async fn consume_receiver(service_state: Arc<Mutex<ServiceState>>, receiver: Receiver<Task>, index_map: Arc<DashMap<Task, u64>>) {
    debug!("Starting consumer");
    loop {
        let span = span!(Level::DEBUG, "worker loop");
        let _enter = span.enter();
        select! {
            recv(receiver)->next => {
                match next {
                    Ok(task) => {
                        let option = index_map.get(&task).map(|task| task.value().clone());
                        schedule_task(task, service_state.clone(), index_map.clone(), option).await;
                    },
                    Err( e) => {
                        error!("Error receiving task from worker {:?}", e);
                    }
                }
            },
        }
    }
}

impl Worker {

    pub fn new(service_state: Arc<Mutex<ServiceState>>, receiver: Receiver<Task>, index_map: Arc<DashMap<Task,u64>>) -> Self {
        let handle = tokio::spawn(consume_receiver(service_state, receiver, index_map));
        Worker {
            handle
        }
    }

    pub fn cancel(&self) {
        self.handle.abort();
    }

}
