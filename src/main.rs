use std::str::FromStr;
use std::sync::Arc;

use axum::{Json, middleware::from_fn_with_state, Router, routing::get};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use eyre::Result;
use fatline_rs::{posts::PostService, users::UserService};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use serde::Serialize;

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

type ServiceArcState = State<Arc<Mutex<Service>>>;

#[tokio::main]
async fn main() -> Result<()> {
    let server_url = dotenv::var("SERVER_URL").expect("No server URL included");
    let bind_addr = dotenv::var("BIND_ADDR").unwrap_or("127.0.0.1:8000".to_string());

    let service = Service::new(server_url).await;
    let service_arc = Arc::new(Mutex::new(service));

    let app = Router::new()
        .route("/profile/me", get(current_user_profile))
        .route_layer(from_fn_with_state(service_arc.clone(), fid_sig_auth_middleware))
        .with_state(service_arc);

    println!("Running on {}", &bind_addr);
    let tcp_listener = TcpListener::bind(bind_addr).await.expect("Couldn't create tcp listener");
    axum::serve(tcp_listener, app).await?;

    Ok(())
}


// TODO: refactor this to use extensions question mark?
async fn current_user_profile(
    header_map: HeaderMap,
    State(service): ServiceArcState,
) -> Result<Json<fatline_rs::users::Profile>, StatusCode> {
    let mut service = service.lock().await;
    let fid = u64::from_str(header_map[FID_HEADER].to_str().expect("Fid header wasn't str")).map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json::from(service.hub_client.get_user_profile(fid).await.map_err(|_| StatusCode::NOT_FOUND)?))
}