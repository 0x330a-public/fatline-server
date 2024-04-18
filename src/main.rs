use std::sync::Arc;

use axum::{middleware::from_fn_with_state, Router, routing::get};
use axum::http::StatusCode;
use eyre::Result;
use fatline_rs::{posts::PostService, users::UserService};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::{auth_layer::fid_sig_auth_middleware, service::Service};

mod service;
mod auth_layer;

// constants for headers
// required headers: pub_hex, timestamp, sig, fid
// optional header: extra_sig_data, maybe use this as route specific signature verification instead of overall?
// wip implementation: H(pub_key (not hex) || timestamp str encode bytes || [optional: extra_sig_data (not hex)]) -> should match sig for pub key
const PUB_HEX_HEADER: &'static str = "key_hex";
const TIMESTAMP_HEADER: &'static str = "timestamp";
const SIGNATURE_DATA_HEADER: &'static str = "extra_sig_data_hex";
const SIGNATURE_HEADER: &'static str = "sig";
const FID_HEADER: &'static str = "fid";

#[tokio::main]
async fn main() -> Result<()> {
    let server_url = dotenv::var("SERVER_URL").expect("No server URL included");
    let bind_addr = dotenv::var("BIND_ADDR").unwrap_or("127.0.0.1:8000".to_string());

    let service = Service::new(server_url).await;
    let service_arc = Arc::new(Mutex::new(service));

    let app = Router::new()
        .route("/", get(|| async { Ok::<(), StatusCode>(()) }))
        .route_layer(from_fn_with_state(service_arc.clone(), fid_sig_auth_middleware))
        .with_state(service_arc);

    let tcp_listener = TcpListener::bind(bind_addr).await.expect("Couldn't create tcp listener");

    axum::serve(tcp_listener, app).await?;

    Ok(())
}
