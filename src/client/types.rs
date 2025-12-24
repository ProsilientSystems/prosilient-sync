use serde::Deserialize;
use crate::consts::CLIENT_EXPIRATION_SECS;
use crate::util::now_secs;

#[derive(Deserialize, Debug)]
pub struct ClientSignature {
    pub msg: String,
    pub iat: u64
}

pub fn is_expired(ts:i64, now: Option<i64>)-> bool{
    if let Some(now) = now {
        ts + CLIENT_EXPIRATION_SECS > now
    }else {
        ts + CLIENT_EXPIRATION_SECS > chrono::Utc::now().timestamp()
    }
}