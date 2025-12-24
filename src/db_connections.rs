use crate::util::{
    MessageTypes, GET_ALL_BIOSENORS, GET_EVENT_ID, GET_P_K, INSERT_AUTHORIZED_DEVICES,
    INSERT_BIOSENOR, INSERT_EVENT, INSERT_SCALE, INSERT_SURVEY, INSERT_TELEMETRY,
    MACHINE_AUTH_EXISTS, UPDATE_AUTHORIZED_DEVICES,
};
use anyhow::{anyhow, Result};
use prosilient_geri_protobuf::client::{EventType, SurveyType};
use serde_json::Value as JsonValue;
use sqlx::pool::PoolConnection;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::types::Json;
use sqlx::types::Uuid;
use sqlx::{FromRow, Postgres, Row};
use tracing::error;

#[derive(Debug, FromRow, Clone)]
pub struct Biosensor {
    pub telemetry_id: Uuid,
    pub event_id: Uuid,
    pub x_axis: f64,
    pub y_axis: f64,
    pub z_axis: f64,
    pub created_on: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub struct AuthorizedDevices {
    pub machine_id: String,
    pub public_key: Vec<u8>,
    pub is_online: Option<DateTime<Utc>>,
    pub authorized_on: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub struct Scale {
    pub telemetry_id: Uuid,
    pub event_id: Uuid,
    pub weight: f32,
    pub impedance: i32,
    pub unit: String,
}

#[derive(Debug, FromRow, Clone)]
pub struct Telemetry {
    pub machine_id: String,
    pub server_time_stamp: DateTime<Utc>,
    pub machine_time_stamp: DateTime<Utc>,
    pub message_type: MessageTypes,
}
#[derive(Debug, FromRow, Clone)]
pub struct TelemetryId {
    pub telemetry_id: Uuid,
}

#[derive(Debug, FromRow, Clone, Copy)]
pub struct event_id {
    pub event_id: Uuid,
}

#[derive(Debug, FromRow, Clone)]
pub struct public_key {
    pub public_key: Vec<u8>,
}

#[derive(Debug, FromRow, Clone)]
pub struct Event {
    pub telemetry_id: Uuid,
    pub event: EventType,
    pub event_identifier: Uuid,
    pub machine_event_time: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub struct Survey {
    pub telemetry_id: Uuid,
    pub event_id: Uuid,
    pub survey_id: String,
    pub survey_type: SurveyType,
    pub answers: Json<JsonValue>,
}

impl std::fmt::Display for Biosensor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "- created_on: {} \n- x_axis: {}\n- y_axis: {}\n- z_axis: {}\n- created_on: {}",
            self.telemetry_id, self.x_axis, self.y_axis, self.z_axis, self.created_on
        )
    }
}

impl std::fmt::Display for Scale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "- telemetry_id: {} \n- weight: {}\n- impedance: {}\n- unit: {}\n",
            self.telemetry_id, self.weight, self.impedance, self.unit
        )
    }
}

pub struct Database;

impl Database {
    pub async fn insert_telemetry(
        conn: &mut PoolConnection<Postgres>,
        telemetry: Telemetry,
    ) -> Result<Uuid, sqlx::Error> {
        let query_result = sqlx::query_as::<_, TelemetryId>(INSERT_TELEMETRY)
            .bind(telemetry.machine_id)
            .bind(telemetry.server_time_stamp)
            .bind(telemetry.machine_time_stamp)
            .bind(telemetry.message_type)
            .fetch_one(conn)
            .await;

        if let Err(err) = query_result {
            error!("Error insert_telemetry: {err}");
            return Err(err);
        }
        return Ok(query_result.unwrap().telemetry_id);
    }

    pub async fn insert_event(
        conn: &mut PoolConnection<Postgres>,
        event: Event,
    ) -> Result<bool, sqlx::Error> {
        let query_result = sqlx::query(INSERT_EVENT)
            .bind(event.telemetry_id)
            .bind(event.event)
            .bind(event.event_identifier)
            .bind(event.machine_event_time)
            .fetch_one(conn)
            .await;

        if let Err(err) = query_result {
            error!("Error: {err}");
            return Err(err);
        }
        return Ok(true);
    }

    pub async fn insert_authorized_device(
        conn: &mut PoolConnection<Postgres>,
        authorized_device: AuthorizedDevices,
    ) -> Result<bool, sqlx::Error> {
        let query_result = sqlx::query(INSERT_AUTHORIZED_DEVICES)
            .bind(authorized_device.machine_id)
            .bind(authorized_device.public_key)
            .bind(authorized_device.authorized_on)
            .bind(authorized_device.is_online)
            .fetch_one(conn)
            .await;

        if let Err(err) = query_result {
            error!("Error: {err}");
            return Err(err);
        }
        return Ok(true);
    }

    pub async fn update_authorized_device(
        conn: &mut PoolConnection<Postgres>,
        machine_id: &str,
        is_online: Option<DateTime<Utc>>,
    ) -> Result<bool, sqlx::Error> {
        let query_result = sqlx::query(UPDATE_AUTHORIZED_DEVICES)
            .bind(machine_id)
            .bind(is_online)
            .execute(conn)
            .await;

        if let Err(err) = query_result {
            error!("Error: {err}");
            return Err(err);
        }
        return Ok(true);
    }

    //check if machine is authorized
    pub async fn check_if_machine_auth(
        conn: &mut PoolConnection<Postgres>,
        machine_id: String,
    ) -> Result<bool, sqlx::Error> {
        let query_result = sqlx::query(MACHINE_AUTH_EXISTS)
            .bind(machine_id)
            .fetch_one(conn)
            .await;
        match query_result {
            Ok(res) => {Ok(res.try_get(0).unwrap())},
            Err(err) => {
                error!("{err}");
                Err(err)
            }
        }
    }

    //get pk of authorized machine
    pub async fn get_machine_pk(
        conn: &mut PoolConnection<Postgres>,
        machine_id: String,
    ) -> Result<Vec<u8>, sqlx::Error> {
        let query = sqlx::query_as::<_, public_key>(GET_P_K);

        let query_result = query.bind(machine_id).fetch_one(conn).await;

        if let Err(err) = query_result {
            error!("{err}");
            return Err(err);
        }

        let machines = query_result.unwrap();
        return Ok(machines.public_key);
    }

    pub async fn insert_biosenor(
        conn: &mut PoolConnection<Postgres>,
        biosensor: Biosensor,
    ) -> Result<bool, sqlx::Error> {
        let query_result = sqlx::query(INSERT_BIOSENOR)
            .bind(biosensor.telemetry_id)
            .bind(biosensor.event_id)
            .bind(biosensor.x_axis)
            .bind(biosensor.y_axis)
            .bind(biosensor.z_axis)
            .bind(biosensor.created_on)
            .fetch_one(conn)
            .await;

        if let Err(err) = query_result {
            error!("Error: {err}");
            return Err(err);
        }
        return Ok(true);
    }

    pub async fn get_all_biosenors(
        conn: &mut PoolConnection<Postgres>,
    ) -> Result<Vec<Biosensor>, sqlx::Error> {
        let query = sqlx::query_as::<_, Biosensor>(GET_ALL_BIOSENORS);

        let query_result = query.fetch_all(conn).await;

        if let Err(err) = query_result {
            error!("{err}");
            let machines: Vec<Biosensor> = Vec::new();
            return Ok(machines);
        }

        let machines = query_result.unwrap();
        return Ok(machines);
    }

    pub async fn get_event_identifier(
        conn: &mut PoolConnection<Postgres>,
        event_identifier: String,
        event_type: EventType,
    ) -> Result<Uuid> {
        let query = sqlx::query_as::<_, event_id>(GET_EVENT_ID);

        let query_result = query
            .bind(Uuid::parse_str(&event_identifier).unwrap())
            .bind(event_type)
            .fetch_all(conn)
            .await;

        if let Err(err) = query_result {
            error!("{err}");
            return Err(err.into());
        }

        let machines = query_result.unwrap();
        return machines
            .get(0)
            .ok_or_else(|| anyhow!("Event identifier not found"))
            .and_then(|machine| Ok(machine.event_id));
    }

    pub async fn insert_scale(
        conn: &mut PoolConnection<Postgres>,
        scale: Scale,
    ) -> Result<bool, sqlx::Error> {
        let query_result = sqlx::query(INSERT_SCALE)
            .bind(scale.telemetry_id)
            .bind(scale.event_id)
            .bind(scale.weight)
            .bind(scale.impedance)
            .bind(scale.unit)
            .fetch_one(conn)
            .await;

        if let Err(err) = query_result {
            error!("Error: {err}");
            return Err(err);
        }

        return Ok(true);
    }

    pub async fn insert_survey(
        conn: &mut PoolConnection<Postgres>,
        survey: Survey,
    ) -> Result<bool, sqlx::Error> {
        let query_result = sqlx::query(INSERT_SURVEY)
            .bind(survey.telemetry_id)
            .bind(survey.event_id)
            .bind(survey.survey_id)
            .bind(survey.survey_type)
            .bind(survey.answers)
            .fetch_one(conn)
            .await;
        if let Err(err) = query_result {
            error!("Error: {err}");
            return Err(err);
        }
        return Ok(true);
    }
}
