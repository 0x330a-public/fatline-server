use std::any::Any;
use std::str::FromStr;
use std::sync::Arc;

use axum::{
    extract::Request,
    response::IntoResponse
};
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use fatline_rs::{HASH_LENGTH, HubService, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};
use fatline_rs::users::UserService;
use tokio::sync::Mutex;

use crate::{FID_HEADER, PUB_HEX_HEADER, SIGNATURE_DATA_HEADER, SIGNATURE_HEADER, TIMESTAMP_HEADER};
use crate::service::Service;

// probably move this to some enum class header error and handle it in axum routes zz
fn bad_request(_: impl Any) -> StatusCode {
    StatusCode::BAD_REQUEST
}

pub async fn fid_sig_auth_middleware(
    State(state): State<Arc<Mutex<Service>>>,
    request: Request,
    next: Next,
) -> Result<impl IntoResponse, StatusCode> {
    let headers = request.headers();
    // do something with `request`...
    let extra_data_header = headers.get(SIGNATURE_DATA_HEADER); // try extract message from header
    let sig_header = headers.get(SIGNATURE_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?; // try extract sig from header
    let pub_key_header = headers.get(PUB_HEX_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?; // try extract pub key from header
    let timestamp_header = headers.get(TIMESTAMP_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let fid_header = headers.get(FID_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?; // try extract fid from header
    let mut extra_data = Vec::new();
    let mut sig = [0u8; SIGNATURE_LENGTH];
    let mut pub_key = [0u8; PUBLIC_KEY_LENGTH];
    
    if let Some(data) = extra_data_header {
        extra_data.append(&mut hex::decode(data.as_bytes()).map_err(bad_request)?);
    };
    
    hex::decode_to_slice(sig_header, &mut sig).map_err(bad_request)?;
    hex::decode_to_slice(pub_key_header.as_bytes(), &mut pub_key).map_err(bad_request)?;
    
    let timestamp_str = timestamp_header.to_str().map_err(bad_request)?;
    // TODO: check timestamp is withing X minutes / seconds here
    let timestamp: u64 = u64::from_str(timestamp_str).map_err(bad_request)?;
    let fid: u64 = u64::from_str(fid_header.to_str().map_err(bad_request)?).map_err(bad_request)?;
    
    let mut msg = Vec::new();
    msg.append(&mut Vec::from(pub_key.clone()));
    msg.append(&mut Vec::from(timestamp_str.as_bytes()));
    msg.append(&mut extra_data);
    let msg_hash = fatline_rs::utils::truncated_hash(msg.as_slice());

    let validation_result = {
        let mut service_lock = state.lock().await;
        
        validate_fid_and_key(
            &mut service_lock.hub_client,
            msg_hash,
            sig,
            pub_key,
            fid
        ).await
    };

    if validation_result.is_ok() {
        let res = next.run(request).await;
        Ok(res)
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

// Validate that pub_key signed the hash and that pub_key belongs to, and is active on fid
async fn validate_fid_and_key(
    user_service: &mut HubService,
    msg: [u8; HASH_LENGTH],
    sig: [u8; SIGNATURE_LENGTH],
    pub_key: [u8; PUBLIC_KEY_LENGTH],
    fid: u64
) -> Result<(), StatusCode> {

    // Check signer is valid for requested fid
    if !user_service.key_valid_for_fid(&pub_key, fid).await.map_err(|err| StatusCode::INTERNAL_SERVER_ERROR)? {
        return Err(StatusCode::UNAUTHORIZED)
    }

    // check basic signature verification for request
    let verification = fatline_rs::utils::validate_signed_by(
        &msg,
        &sig,
        &pub_key
    ).map_err(|err| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !verification {
        // log maybe or something
        return Err(StatusCode::UNAUTHORIZED);
    }
    // check fid actually has key

    if user_service.key_valid_for_fid(&pub_key, fid).await
        .map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)? {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }

}
