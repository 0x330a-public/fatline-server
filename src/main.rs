use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use axum::{Error, Extension, Json, middleware::from_fn_with_state, Router, routing::get};
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use clap::{Parser, Subcommand};
use crossbeam::channel;
use crossbeam::channel::{bounded, Sender};
use dashmap::DashMap;
use dotenvy::var;
use eyre::{bail, Result};
use fatline_rs::{HubService, MessageTrait, posts::PostService, users::UserService};
use fatline_rs::proto::{FidRequest, Message};
use fatline_rs::users::Profile;
use futures_util::TryFutureExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::{auth_layer::fid_sig_auth_middleware, service::ServiceState};
use crate::signer_repo::SignerRepository;
use crate::subscriber::{signer_from_event, Subscriber};
use crate::user_models::{Link, Signer};
use crate::user_repo::{FollowDirection, UserRepository};
use crate::worker::{Task, Worker};

mod schema;
mod service;
mod auth_layer;
mod timeline;
mod user_models;
mod user_repo;
mod worker;
mod signer_repo;
mod error;
mod subscriber;
mod notifier;

// constants for headers
// required headers: pub_hex, timestamp, sig, fid
// optional header: extra_sig_data, maybe use this as route specific signature verification instead of overall?
// wip implementation: H(pub_key (not hex) || timestamp str encode bytes || [optional: extra_sig_data (not hex)]) -> should match sig for pub key
const PUB_HEX_HEADER: &'static str = "key_hex";
const TIMESTAMP_HEADER: &'static str = "timestamp";
const SIGNATURE_DATA_HEADER: &'static str = "extra_sig_data_hex";
const SIGNATURE_HEADER: &'static str = "sig";
const FID_HEADER: &'static str = "fid";

type ServiceArcState = State<Arc<ServiceState>>;

#[derive(Parser, Debug)]
#[command(name="fatline")]
#[command(about="Fatline server binary")]
struct Args {
    #[command(subcommand)]
    command: Commands
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Sync the signers for fid and return
    #[command(name = "sync", arg_required_else_help = true)]
    SyncSigner {
        #[arg(required = true)]
        fid: u64
    },
    /// Run the server normally
    #[command(name= "run")]
    Run{}
}

async fn index_signers(fid: u64) -> Result<()> {
    let (s,r) = bounded(1);
    let mut state = ServiceState::new(s).await;

    let mut hub_client = ServiceState::hub_client().await;

    let events = hub_client.get_on_chain_signers_by_fid(FidRequest {
        fid,
        page_size: None,
        reverse: None,
        page_token: None
    }).await?.into_inner().events.iter().filter_map(|e| signer_from_event(e)).collect::<Vec<_>>();

    debug!("inserting {} signer events", events.len());

    for signer in events {
        state.insert_signer(signer).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
                "fatline_server=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    if let Commands::SyncSigner{ fid } = args.command {
        index_signers(fid).await?;
        return Ok(())
    };

    let bind_addr = var("BIND_ADDR").unwrap_or("127.0.0.1:8000".to_string());

    debug!("Initializing resources");

    let index_map = Arc::new(DashMap::new());
    let (sender, receiver) = channel::unbounded();

    let service = ServiceState::new(sender.clone()).await;
    let worker_service = ServiceState::new(sender.clone()).await;

    let service_arc = Arc::new(service);
    debug!("Initialized server resources [1/2]");

    let worker_service = Arc::new(worker_service);
    debug!("Initialized worker resources [2/2]");

    let worker = Worker::new(worker_service, receiver.clone(), index_map.clone());
    let subscriber = Subscriber::new(sender.clone()).await;

    let app = Router::new()
        .route("/profile/me", get(current_user_profile))
        .route("/profile/:fid", get(get_user_profile))
        .route("/profile/:fid/follows", get(get_user_followed_by))
        .route("/profile/:fid/following", get(get_user_following))
        .route("/submit_message", post(submit_message))
        .route("/submit_messages", post(submit_messages))
        .route_layer(from_fn_with_state(service_arc.clone(), fid_sig_auth_middleware))
        .with_state(service_arc);

    debug!("Running on {}", &bind_addr);
    let tcp_listener = TcpListener::bind(bind_addr).await.expect("Couldn't create tcp listener");
    axum::serve(tcp_listener, app).await?;

    worker.cancel();
    subscriber.cancel();

    Ok(())
}

fn queue_index_fid(sender: &Sender<Task>, fid: u64) {
    if let Err(e) = sender.send(Task::IndexFid(fid, false)) {
        error!("Couldn't queue fid index task {e}");
    }
}

fn queue_index_links(sender: &Sender<Task>, fid: u64) {
    if let Err(e) = sender.send(Task::IndexLinks(fid)) {
        error!("Couldn't queue fid link index task {e}");
    }
}

fn queue_index_casts(sender: &Sender<Task>, fid: u64) {
    if let Err(e) = sender.send(Task::IndexFidCasts(fid, false)) {
        error!("Couldn't queue fid cast index task {e}");
    }
}

async fn handle_message(message: Vec<u8>, signer: &Signer, hub_service: &mut HubService) -> Result<()> {
    if let Ok(parsed_message) = Message::decode(message.as_slice()) {
        // Deny submitting messages for other people todo: does this make sense?
        if parsed_message.signer != signer.pk || !signer.active {
            bail!("Message isn't from signer");
        }

        if let Err(e) = hub_service.submit_message(parsed_message).await {
            bail!("Error submitting to hub {e}")
        };
    } else {
        bail!("Couldn't process message as Message");
    }
    debug!("successfully forwarded message to hub");
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct Messages {
    pub updates: Vec<Vec<u8>>
}

async fn submit_messages(
    State(state): ServiceArcState,
    Extension(signer): Extension<Signer>,
    Json(messages): Json<Messages>
) -> Result<(), StatusCode> {
    let mut hub = state.hub_client.lock().await;
    for message in messages.updates {
        handle_message(message, &signer, &mut hub).await.map_err(|e| {
            error!("error posting update {}",e.to_string());
            StatusCode::BAD_REQUEST
        })?
    }
    Ok(())
}

async fn submit_message(
    State(state): ServiceArcState,
    Extension(signer): Extension<Signer>,
    body_bytes: Bytes
) -> Result<(), StatusCode> {
    let mut hub_client = state.hub_client.lock().await;
    handle_message(body_bytes.to_vec(), &signer, &mut hub_client).await.map_err(|_|StatusCode::BAD_REQUEST)
}


async fn current_user_profile(
    State(state): ServiceArcState,
    Extension(profile): Extension<Profile>
) -> Result<Json<Profile>, StatusCode> {

    queue_index_fid(&state.work_sender, profile.fid);
    queue_index_links(&state.work_sender, profile.fid);
    queue_index_casts(&state.work_sender, profile.fid);

    Ok(Json::from(profile))
}

async fn get_user_profile(
    State(state): ServiceArcState,
    Path(fid): Path<u64>
) -> Result<Json<Profile>, StatusCode> {

    queue_index_fid(&state.work_sender, fid);
    queue_index_links(&state.work_sender, fid);
    queue_index_casts(&state.work_sender, fid);

    match state.get_user_profile(fid, false).await {
        Ok(profile) => {
            Ok(Json(profile))
        }
        Err(_) => {
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn get_user_following(
    State(mut state): ServiceArcState,
    Path(fid): Path<u64>,
) -> Result<Json<Vec<Profile>>, StatusCode> {

    queue_index_fid(&state.work_sender, fid);
    queue_index_links(&state.work_sender, fid);
    queue_index_casts(&state.work_sender, fid);

    match state.get_profile_links(fid, true, FollowDirection::Following).await {
        Ok(links) => {
            Ok(Json(links))
        },
        Err(e) => {
            error!("Couldn't get profile links {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_user_followed_by(
    State(mut state): ServiceArcState,
    Path(fid): Path<u64>,
) -> Result<Json<Vec<Profile>>, StatusCode> {

    queue_index_fid(&state.work_sender, fid);
    queue_index_links(&state.work_sender, fid);
    queue_index_casts(&state.work_sender, fid);

    match state.get_profile_links(fid, true, FollowDirection::FollowedBy).await {
        Ok(links) => {
            Ok(Json(links))
        },
        Err(e) => {
            error!("Couldn't get profile links {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
