use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use axum::{Extension, Json, middleware::from_fn_with_state, Router, routing::get};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use crossbeam::channel;
use dashmap::DashMap;
use dotenvy::var;
use eyre::Result;
use fatline_rs::{posts::PostService, users::UserService};
use fatline_rs::users::Profile;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use serde::Serialize;
use tracing::debug;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::{auth_layer::fid_sig_auth_middleware, service::ServiceState};
use crate::subscriber::Subscriber;
use crate::user_repo::UserRepository;
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

// constants for headers
// required headers: pub_hex, timestamp, sig, fid
// optional header: extra_sig_data, maybe use this as route specific signature verification instead of overall?
// wip implementation: H(pub_key (not hex) || timestamp str encode bytes || [optional: extra_sig_data (not hex)]) -> should match sig for pub key
const PUB_HEX_HEADER: &'static str = "key_hex";
const TIMESTAMP_HEADER: &'static str = "timestamp";
const SIGNATURE_DATA_HEADER: &'static str = "extra_sig_data_hex";
const SIGNATURE_HEADER: &'static str = "sig";
const FID_HEADER: &'static str = "fid";

type ServiceArcState = State<Arc<Mutex<ServiceState>>>;

#[tokio::main]
async fn main() -> Result<()> {
    let bind_addr = var("BIND_ADDR").unwrap_or("127.0.0.1:8000".to_string());

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

    debug!("Initializing resources");

    let index_map = Arc::new(DashMap::new());
    let (sender, receiver) = channel::unbounded();

    let service = ServiceState::new(sender.clone());
    let worker_service = ServiceState::new(sender.clone());

    let service_arc = Arc::new(Mutex::new(service.await));
    debug!("Initialized server resources [1/2]");

    let worker_service = Arc::new(Mutex::new(worker_service.await));
    debug!("Initialized worker resources [2/2]");

    let worker = Worker::new(worker_service, receiver.clone(), index_map.clone());
    let subscriber = Subscriber::new(sender.clone()).await;

    let app = Router::new()
        .route("/profile/me", get(current_user_profile))
        .route_layer(from_fn_with_state(service_arc.clone(), fid_sig_auth_middleware))
        .with_state(service_arc);

    debug!("Running on {}", &bind_addr);
    let tcp_listener = TcpListener::bind(bind_addr).await.expect("Couldn't create tcp listener");
    axum::serve(tcp_listener, app).await?;

    worker.cancel();
    subscriber.cancel();

    Ok(())
}

async fn current_user_profile(
    Extension(profile): Extension<Profile>
) -> Result<Json<Profile>, StatusCode> {
    Ok(Json::from(profile))
}
