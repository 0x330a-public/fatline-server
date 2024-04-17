use eyre::Result;
use fatline_rs::{posts::PostService, users::UserService};

use crate::service::Service;

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

    let service = Service::new(server_url).await;

    Ok(())
}
