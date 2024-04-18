use std::sync::{Mutex, Arc};

use tokio::net::TcpListener;

use axum::{Router, middleware::{from_fn, from_fn_with_state}, routing::get, http::StatusCode};
use eyre::Result;
use fatline_rs::{posts::PostService, users::UserService};

use crate::{service::Service, auth_layer::fid_sig_auth_middleware};

mod service;
mod auth_layer;

// constants for headers
const PUB_HEX_HEADER: &'static str = "key_hex";
const SIGNATURE_DATA_HEADER: &'static str = "sig_data";
const SIGNATURE_HEADER: &'static str = "sig";
const FID_HEADER: &'static str = "fid";

#[tokio::main]
async fn main() -> Result<()> {

    let server_url = dotenv::var("SERVER_URL").expect("No server URL included");
    let bind_addr = dotenv::var("BIND_ADDR").unwrap_or("127.0.0.1:8000".to_string());

    let service = Service::new(server_url).await;
    let service_arc = Arc::new(Mutex::new(service));

    let app = Router::new()
        .route("/", get(test))
        .route_layer(from_fn_with_state(service_arc.clone(), fid_sig_auth_middleware))
        .with_state(service_arc);

    let tcp_listener = TcpListener::bind(bind_addr).await.expect("Couldn't create tcp listener");

    axum::serve(tcp_listener, app).await?;

    Ok(())
}

async fn test() -> Result<(), StatusCode> {
    todo!()
}