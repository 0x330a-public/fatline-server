use std::any::Any;
use std::ops::DerefMut;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use axum::{
    extract::Request,
    response::Response
};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use fatline_rs::{HASH_LENGTH, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};
use fatline_rs::users::UserService;
use tower::{Layer, Service as TowerService};
use crate::service::Service;
use crate::{FID_HEADER, PUB_HEX_HEADER, SIGNATURE_DATA_HEADER, SIGNATURE_HEADER};

fn bad_request(_: impl Any) -> StatusCode {
    StatusCode::BAD_REQUEST
}

pub async fn fid_sig_auth_middleware(
    headers: HeaderMap,
    State(state): State<Arc<Mutex<Service>>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // do something with `request`...
    let msg_header = headers.get(SIGNATURE_DATA_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?; // try extract message from header
    let sig_header = headers.get(SIGNATURE_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?; // try extract sig from header
    let pub_key_header = headers.get(PUB_HEX_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?; // try extract pub key from header
    let fid_header = headers.get(FID_HEADER)
        .ok_or(StatusCode::BAD_REQUEST)?; // try extract fid from header
    let mut msg = [0u8;HASH_LENGTH];
    let mut sig = [0u8; SIGNATURE_LENGTH];
    let mut pub_key = [0u8; PUBLIC_KEY_LENGTH];

    hex::decode_to_slice(msg_header.as_bytes(), &mut msg).map_err(bad_request)?;
    hex::decode_to_slice(sig_header, &mut sig).map_err(bad_request)?;
    hex::decode_to_slice(pub_key_header.as_bytes(), &mut pub_key).map_err(bad_request)?;

    let fid: u64 = u64::from_str(fid_header.to_str().map_err(bad_request)?).map_err(bad_request)?;

    let mut service_lock = state.lock().map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;

    match validate_fid_and_key(
        &mut service_lock.hub_client,
        msg,
        sig,
        pub_key,
        fid
    ).await {
        Ok(_) => Ok(next.run(request).await),
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

// Validate that pub_key signed the hash and that pub_key belongs to, and is active on fid
async fn validate_fid_and_key(
    user_service: &mut dyn UserService,
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
        .map_err(|e|StatusCode::INTERNAL_SERVER_ERROR)? {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }

}
