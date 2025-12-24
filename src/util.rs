use std::time::SystemTime;

use axum::{async_trait, extract::FromRequestParts, http::{StatusCode, request::Parts, HeaderMap}, RequestPartsExt};
use prosilient_geri_protobuf::client::{client_command::Message as ClientMessage};
use serde::Deserialize;
use sodiumoxide::base64;
use tracing::debug;

/// Fallible 
pub fn now_secs()-> u64{
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
}

pub const INSERT_AUTHORIZED_DEVICES: &str = "insert into authorized_devices (machine_id, public_key, authorized_on, is_online) values ($1, $2, $3, $4) returning machine_id";
pub const UPDATE_AUTHORIZED_DEVICES: &str = "update authorized_devices set is_online=$2 where machine_id=$1 returning machine_id";
pub const INSERT_BIOSENOR: &str = "insert into biosensor (telemetry_id, event_id, x_axis, y_axis, z_axis, created_on) values ($1, $2, $3, $4, $5, $6) returning telemetry_id";
pub const GET_ALL_BIOSENORS: &str = "select * from biosensor";
pub const INSERT_SCALE: &str = "INSERT INTO scale ( telemetry_id,event_id, weight, impedance, unit) VALUES ( $1, $2, $3, $4, $5) RETURNING telemetry_id";
pub const INSERT_TELEMETRY: &str = "insert into telemetry (machine_id, server_time_stamp, machine_time_stamp, message_type) values ($1, $2, $3, $4) returning telemetry_id";
pub const INSERT_EVENT: &str = "insert into events (telemetry_id, event, event_identifier, machine_event_time) values ($1, $2, $3, $4) returning telemetry_id";
pub const INSERT_SURVEY: &str = "insert into survey (telemetry_id, event_id, survey_id,survey_type ,answers) values ($1, $2, $3, $4, $5) returning telemetry_id";
pub const GET_EVENT_ID: &str = "select event_id from events where event_identifier=$1 and event=$2";
pub const MACHINE_AUTH_EXISTS: &str = "SELECT EXISTS(SELECT 1 FROM authorized_devices WHERE machine_id = $1)";
pub const GET_P_K: &str = "select public_key from authorized_devices where machine_id=$1";




#[derive(sqlx::Type, Debug, Clone)]
#[sqlx(type_name = "message_types")]
pub enum MessageTypes {
    Connect,
    Biosensor, 
    Scale, 
    Survey, 
    Event
}

// Return the type of proto message.
pub fn get_proto_message_type(message: &ClientMessage)-> MessageTypes{
    match message {
        ClientMessage::Biosensor(_data) =>{
            return MessageTypes::Biosensor;
        },
        ClientMessage::Scale(_data)=>{
            return MessageTypes::Scale;
        },
        ClientMessage::Survey(_data)=>{
            return MessageTypes::Survey;
        },
        ClientMessage::Event(_data)=>{
            return MessageTypes::Event;
        }
    }
}


#[derive(Debug, Deserialize)]
pub struct DeviceAuth(pub(Vec<u8>, String));

#[async_trait]
impl<S> FromRequestParts<S> for DeviceAuth
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Some(headers): Option<HeaderMap> = parts.extract().await.unwrap()else {
            return Err((StatusCode::PRECONDITION_FAILED, "🇵🇰"));
        };
        let Some(auth) = headers.get("Authorization")else{
            return Err((StatusCode::BAD_REQUEST, "Authorization Header Missing"));
        };
        let Ok(decoded_auth) = base64::decode(auth, base64::Variant::UrlSafeNoPadding) else {
            return Err((StatusCode::FORBIDDEN, "Incorrect formatting; Base64; UrlSafeNoPadding"));
        };
        let Ok(authenticated_device) = serde_json::from_slice::<DeviceAuth>(decoded_auth.as_slice()) else {
            return Err((StatusCode::UNAUTHORIZED, "Failed to decode"));
        };

        debug!("{:?}", headers.get("User-Agent"));
        
        Ok(authenticated_device)
    }
}