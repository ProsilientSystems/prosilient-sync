use crate::client::types::{is_expired, ClientSignature};
use crate::consts::MAX_MISSED_CLIENT_PINGS;
use crate::db_connections::{Biosensor, Database, Event, Scale, Survey, Telemetry};
use crate::state::{ClientLifecycle, ServerToClientMessage};
use crate::util::{get_proto_message_type, DeviceAuth};
use crate::AppState;
use axum::extract::ws::{Message as axumMessage, WebSocket};
use chrono;
use futures::{SinkExt, StreamExt};
use prosilient_geri_protobuf::client::{
    client_command::Message as ClientMessage, ClientCommand, EventType, SurveyType,
};
use prosilient_geri_protobuf::server::{create_ack, create_simple, SimpleMessage};
use prost::Message;
use sodiumoxide::crypto::sign::ed25519::PublicKey;
use sodiumoxide::crypto::sign::verify;
use sqlx::pool::PoolConnection;
use sqlx::types::chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::types::Uuid;
use sqlx::Postgres;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::Mutex;
use tracing::{debug, error};

async fn cleanup(
    conn: Option<&mut PoolConnection<Postgres>>,
    connections: &Mutex<HashMap<String, Sender<ServerToClientMessage>, RandomState>>,
    machine_id: &String,
    stream: WebSocket,
) -> Result<(), axum::Error> {
    debug!("Client dropped, cleaning up");
    if let Some(conn) = conn {
        Database::update_authorized_device(conn, machine_id.as_str(), None).await;
    }
    let mut connections = connections.lock().await;
    connections.remove(machine_id);
    return stream.close().await;
}

pub async fn websocket(
    device_auth: DeviceAuth,
    stream: WebSocket,
    state: Arc<AppState>,
) -> () {
    if let Err(error)= client(device_auth,stream,state).await{
        error!("Client disconnected with {error:?}");
    }
}

pub async fn client(
    device_auth: DeviceAuth,
    stream: WebSocket,
    state: Arc<AppState>,
) -> Result<(), axum::Error> {
    let machine_id = device_auth.0 .1;
    let mut conn = match state.pool.clone().acquire().await {
        Ok(conn) => conn,
        Err(err) => {
            error!("Failed to acquire database connection: {err:?}");
            return cleanup(None, &state.connections, &machine_id, stream).await;
        }
    };

    let (mut sender, mut ws_receiver) = stream.split();
    let (client_send, mut client_receive) = mpsc::channel::<ServerToClientMessage>(10);
    let signature: Vec<u8> = device_auth.0 .0;

    let mut authorized_on: i64 = 0;
    {
        let mut connections = state.connections.lock().await;
        if connections.contains_key(&machine_id) {
            sender.send(axumMessage::Binary(create_simple(SimpleMessage::UnknownError,))).await;
        } else {
            connections.insert(machine_id.clone(), client_send.clone());
            state.concierge.send(machine_id.clone()).await;
        }
    }

    let mut missed_ping_count:i32 = 0;

    loop {
        let heart_beat = tokio::time::sleep(Duration::from_millis(2000));
        tokio::pin!(heart_beat);

        tokio::select! {
                _ = &mut heart_beat, if authorized_on != 0 =>{
                    missed_ping_count += 1;
                    if missed_ping_count > MAX_MISSED_CLIENT_PINGS {
                        // Connection has died
                        error!("Client has missed {missed_ping_count}, disconnecting");
                        return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                    }
                },
                msg_result = ws_receiver.next() => {
                    match msg_result {
                        Some(Ok(axumMessage::Binary(bin_msg))) =>{
                            if let Ok(command) = ClientCommand::decode(&bin_msg[..]){
                                if authorized_on == 0 {
                                    sender.send(axumMessage::Binary(create_simple(SimpleMessage::Unauthenticated))).await;
                                }
                                else {
                                    if let Some(message) = command.message {
                                        debug!("{machine_id} Sent Message: {:?}", message);
                                        //add all message with type to telemetry table.
                                        // let message_type = message.clone();
                                        let message_type = get_proto_message_type(&message);

                                        let machine_timestamp = NaiveDateTime::from_timestamp_opt(command.machine_ts as i64, 0);
                                        if machine_timestamp.is_none(){
                                            sender.send(axumMessage::Binary(create_simple(SimpleMessage::UnknownError))).await;
                                            return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                        }

                                        let telemetry = Telemetry{
                                            machine_id: machine_id.clone(),
                                            server_time_stamp: chrono::offset::Utc::now(),
                                            machine_time_stamp: DateTime::<Utc>::from_utc(machine_timestamp.unwrap(), Utc),
                                            message_type: message_type
                                        };
                                        //insert message with type into telemetry table and get telemetryID.
                                        let telemetry_result = Database::insert_telemetry(&mut conn, telemetry).await;
                                        if let Err(err) = telemetry_result {
                                            debug!("Error in machine {}, inserting telemetry: {}", machine_id, err);
                                            //todo: handle error
                                            return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                        }
                                        let inserted_uuid = telemetry_result.unwrap();


                                        match message {
                                            ClientMessage::Biosensor(data) =>{
                                                let identifier = data.event_identifier;
                                                sender.send(axumMessage::Binary(create_ack(identifier.clone()))).await.unwrap();
                                                // let joined_string = data.samples.iter().map(|i| (*i as i16).to_le_bytes()).collect::<Vec<[u8;2]>>().concat().iter().map(|x| *x as char).collect::<Vec<char>>();
                                                let converted_samples = data.samples.iter().map(|i| *i as i16).collect::<Vec<i16>>();
                                                debug!("ACC: {:?}",converted_samples);
                                                let frequency = 10;
                                                let values: Vec<_> = converted_samples
                                                .chunks_exact(3)
                                                .enumerate()
                                                .map(|(sample, val)| {
                                                    (
                                                        machine_id.to_owned(),
                                                        val[0],
                                                        val[1],
                                                        val[2],
                                                        ((data.start_ts * 1000) + (100 * sample) as u64),
                                                    )
                                                })
                                                .collect();
                                                for value in values {
                                                    let created_timestamp = NaiveDateTime::from_timestamp_opt(value.4 as i64, 0);
                                                    if created_timestamp.is_none(){
                                                        debug!("Error in machine {}, inserting biosensor timestamp", machine_id);
                                                        sender.send(axumMessage::Binary(create_simple(SimpleMessage::UnknownError))).await;
                                                        return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                    }
                                                    //GET EVENT_ID
                                                    let get_id_result = Database::get_event_identifier(&mut conn,identifier.clone(), EventType::ProtocolStarted).await;
                                                    if let Err(err) = get_id_result {
                                                        debug!("Error in machine {machine_id}, inserting Biosensor while fetching event_id: {err}");
                                                        return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                    }
                                                    let fetched_event_id = get_id_result.unwrap();

                                                    let biosensor = Biosensor{
                                                        telemetry_id: inserted_uuid,
                                                        event_id: fetched_event_id,
                                                        x_axis: f64::from(value.1),
                                                        y_axis: f64::from(value.2),
                                                        z_axis: f64::from(value.3),
                                                        created_on: DateTime::<Utc>::from_utc(created_timestamp.unwrap(), Utc),
                                                    };
                                                    let biosensor_insert = Database::insert_biosenor(&mut conn,biosensor).await;
                                                    if let Err(err) = biosensor_insert {
                                                        debug!("Error in machine {machine_id}, inserting Biosensor: {err}");
                                                        sender.send(axumMessage::Binary(create_simple(SimpleMessage::UnknownError))).await;
                                                        return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                    }
                                                }
                                            },
                                            ClientMessage::Scale(data)=>{
                                                let identifier = data.event_identifier;
                                                sender.send(axumMessage::Binary(create_ack(identifier.clone()))).await.unwrap();
                                                //GET EVENT_ID
                                                let get_id_result = Database::get_event_identifier(&mut conn,identifier.clone(), EventType::WeightCompleted).await;
                                                if let Err(err) = get_id_result {
                                                    debug!("Error in machine {machine_id}, inserting Scale while fetching event_id: {err}");
                                                    return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                }
                                                let fetched_event_id = get_id_result.unwrap();

                                                // insert into postgres
                                                let scale = Scale{
                                                    telemetry_id: inserted_uuid,
                                                    event_id: fetched_event_id,
                                                    weight: data.weight,
                                                    impedance: data.impedance,
                                                    unit: data.unit,
                                                };
                                                let scale_insert_result = Database::insert_scale(&mut conn, scale).await;
                                                if let Err(err) = scale_insert_result {
                                                    debug!("Error in machine {machine_id}, inserting scale: {err}");
                                                    return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                }
                                            },
                                            ClientMessage::Survey(data)=>{
                                                let identifier = data.event_identifier;
                                                sender.send(axumMessage::Binary(create_ack(identifier.clone()))).await.unwrap();
                                                //GET EVENT_ID
                                                let data_survey_type = SurveyType::from_i32(data.survey_type).unwrap();
                                                let get_id_result = if data_survey_type == SurveyType::FollowUp {
                                                    Database::get_event_identifier(&mut conn,identifier.clone(), EventType::FollowupCompleted).await
                                                } else {
                                                    Database::get_event_identifier(&mut conn,identifier.clone(), EventType::SurveyCompleted).await
                                                };                                            

                                                if let Err(err) = get_id_result {
                                                    debug!("Error in machine {machine_id}, inserting Survey while fetching event_id: {err}");
                                                    return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                }
                                                let fetched_event_id = get_id_result.unwrap();
                                                // insert into postgres
                                                let survey = Survey{
                                                    telemetry_id: inserted_uuid,
                                                    event_id: fetched_event_id,
                                                    survey_id: data.survey_id,
                                                    survey_type: data_survey_type,
                                                    answers: serde_json::from_str(data.answers_json.as_str()).unwrap()
                                                };
                                                let survey_insert_result = Database::insert_survey(&mut conn,survey).await;
                                                if let Err(err) = survey_insert_result {
                                                    debug!("Error in machine {machine_id}, inserting survey: {err}");
                                                    return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                }
                                            },
                                            ClientMessage::Event(data)=>{
                                                let identifier = data.event_identifier;
                                                debug!("Sending ack for: {identifier}");
                                                sender.send(axumMessage::Binary(create_ack(identifier.clone()))).await.unwrap();

                                                let client_event_timestamp = NaiveDateTime::from_timestamp_opt(data.client_event_time as i64, 0);
                                                if client_event_timestamp.is_none(){
                                                    debug!("Error in machine {}, inserting event timestamp", machine_id);
                                                    sender.send(axumMessage::Binary(create_simple(SimpleMessage::UnknownError))).await;
                                                    return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                }
                                                let event = Event{
                                                    telemetry_id: inserted_uuid,
                                                    event: EventType::from_i32(data.event_type).unwrap(),
                                                    event_identifier: Uuid::parse_str(&identifier).unwrap(),
                                                    machine_event_time: DateTime::<Utc>::from_utc(client_event_timestamp.unwrap(), Utc),
                                                };
                                                let event_result = Database::insert_event(&mut conn,event).await;
                                                if let Err(err) = event_result {
                                                    debug!("Error in machine {machine_id}, inserting event: {err}");
                                                    return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok(axumMessage::Ping(msg)))=>{
                            let parsed = String::from_utf8(msg).unwrap();
                            debug!("Received Ping from {machine_id} {parsed}");
                            if missed_ping_count > 0{
                                missed_ping_count-=1;
                            }
                        }
                        Some(Err(error)) => {
                            error!("Error [{}]: {}", machine_id, error);
                            return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                        }
                        None=>{
                            error!("Error [{}]: Socket Closed", machine_id);
                            return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                        }
                        _=> ()// ignore

                    }
                    // respond with invalid
                },

                // wait for concierge to send a connected message
                Some(message) = client_receive.recv() => {
                    debug!("Concierge to {machine_id}: {:?}", message);
                    match message {
                        ServerToClientMessage::Lifecycle(ClientLifecycle::Added) =>{
                            let machine_pk_result = Database::get_machine_pk(&mut conn, machine_id.to_string()).await;
                            if let Err(err) = machine_pk_result {
                                error!("Error: {err}");
                                debug!("Error fetching public key in machine {machine_id}");

                                return cleanup(Some(&mut conn), &state.connections, &machine_id, sender.reunite(ws_receiver).unwrap()).await;
                            }
                            let pk = machine_pk_result.unwrap();
                            debug!("pk: {:?}", pk);
                                // make public key from str
                            if let Some(pk) = PublicKey::from_slice(&pk){
                                debug!("public key: {:?}", pk);
                                // Verify the signature
                                if let Ok(payload) = verify(&signature, &pk){
                                    debug!("Verified: {:?}", payload);
                                    // Deserialize the payload
                                    match serde_json::from_slice::<ClientSignature>(payload.as_slice()) {
                                        Ok(signature)=> {
                                            // if is_expired(signature.iat, Some(now_ts)) || signature.msg != machine_id {
                                            debug!("Authorized on : {:?}, {:?}", signature, machine_id);
                                            if signature.msg != machine_id {
                                                sender.send(axumMessage::Binary(create_simple(SimpleMessage::Expired))).await;
                                            }else {
                                                authorized_on = chrono::Utc::now().timestamp();
                                                Database::update_authorized_device(&mut conn, machine_id.as_str(), Some(chrono::Utc::now())).await;
                                                debug!("Authorized on : {:?}", authorized_on);
                                                // Set device to online in the autorized devices table, this ensures that the device is online
                                                sender.send(axumMessage::Binary(create_simple(SimpleMessage::Authenticated))).await;
                                            }
                                        }
                                        Err(err)=>{
                                            debug!("Error deserializing: {:?}", err);
                                        }
                                    }
                                }
                            }
                            // }
                            sender.send(axumMessage::Binary(create_simple(SimpleMessage::UnknownError))).await;
                        }
                        // if a client with the same machine_id is trying to connect get, check if that clients
                        // session is still alive
                        ServerToClientMessage::Ping => {
                            if !is_expired(authorized_on, None) {
                                // Send helthcheck message to client
                                sender.send(axumMessage::Binary(create_simple(SimpleMessage::HealthCheck))).await;
                            }
                        }
                        // ignore the rest
                        _ => ()
                    }
                },
            }
    }
}
