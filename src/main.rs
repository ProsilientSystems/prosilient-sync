use anyhow::Result;
mod db_connections;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use base64::{decode, Variant};
use db_connections::{AuthorizedDevices, Database};
use sodiumoxide::base64;
use sqlx::postgres::PgPoolOptions;
use state::{AppState, ClientLifecycle, ServerToClientMessage};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tower_http::trace::TraceLayer;
use tracing::{debug, error};
use util::DeviceAuth;

use std::env;
use tokio::{
    sync::{
        mpsc::{self, Receiver},
        Mutex,
    },
    task::JoinHandle,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
pub mod client;
pub mod consts;
pub mod state;
pub mod util;
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "prosilient_sync=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let connections = Mutex::new(HashMap::new());

    let db_connection_str = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:123@localhost:5432/prosilient".into());

    // setup connection pool
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_connection_str)
        .await?;

    let (send_to_concierge, received_by_concierge) = mpsc::channel(100);

    let app_state = Arc::new(AppState {
        connections,
        concierge: send_to_concierge,
        pool,
    });

    let app = Router::new()
        .route("/", get(|| async { Html("Alive") }))
        .route("/admin", post(admin_approval))
        .route("/websocket", get(websocket_handler))
        .with_state(app_state.clone())
        .layer(TraceLayer::new_for_http());

    let port = env::var("PORT")
        .unwrap_or("8080".to_string())
        .parse()
        .unwrap();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    debug!("Server listening on {}", addr);
    let concierge = start_concierge(received_by_concierge, app_state);

    tokio::select! {
        Ok(()) = tokio::signal::ctrl_c()=> {
            return Ok(())
        },
        Err(err) = axum::Server::bind(&addr)
        .serve(app.into_make_service()) =>{
            Ok(())
        },
        conierge_ended = concierge =>{
            error!("Concierge Closed");
            error!("{:?}", conierge_ended);
            Ok(())
        }
    }
}

fn start_concierge(
    mut received_by_concierge: Receiver<String>,
    app_state: Arc<AppState>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut conn = app_state.pool.clone().acquire().await.unwrap();
        while let Some(new_machine_id) = received_by_concierge.recv().await {
            debug!("Concierge received: {new_machine_id}");

            let is_machine_auth =
                Database::check_if_machine_auth(&mut conn, new_machine_id.to_string()).await;
            if let Err(err) = is_machine_auth {
                error!("Error in fetching authotized devices {new_machine_id}: {err}");
                continue;
            }
            let machine_auth = is_machine_auth.unwrap();
            if !machine_auth {
                debug!("Not authorized");
                continue;
            }
            if let Some(send_to_machine) = app_state.connections.lock().await.get(&new_machine_id) {
                debug!("{new_machine_id} is Authorized");
                match send_to_machine
                    .send(ServerToClientMessage::Lifecycle(ClientLifecycle::Added))
                    .await
                {
                    Ok(_) => {}
                    Err(err) => {
                        error!("{:?}", err);
                    }
                }
            }
        }
    })
}

async fn websocket_handler(
    device_auth: DeviceAuth,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| client::handler::websocket(device_auth, socket, state))
}

async fn admin_approval(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let machine_id = headers.get("X-MACHINE-ID");
    let public_key = headers.get("X-PUBLIC-KEY");
    if machine_id.is_none() || public_key.is_none() {
        return (
            StatusCode::PRECONDITION_REQUIRED,
            format!(
                "machine_id: {}\npublic_key: {}",
                machine_id.is_none(),
                public_key.is_none()
            ),
        );
    }
    if let (Ok(public_key), Some(machine_id)) = (
        decode(public_key.unwrap(), Variant::UrlSafeNoPadding),
        machine_id.and_then(|m| m.to_str().ok()),
    ) {
        // state.authorized.lock().await.insert(machine_id.to_string(), public_key);
        let mut conn = match state.pool.clone().acquire().await {
            Ok(conn) => conn,
            Err(err) => {
                error!("Failed to acquire database connection: {err:?}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Cannot connect to database".to_string(),
                );
            }
        };
        let authorized_device = AuthorizedDevices {
            machine_id: machine_id.to_string(),
            public_key: public_key,
            is_online: None,
            authorized_on: chrono::offset::Utc::now(),
        };
        let authorized_device_insert =
            Database::insert_authorized_device(&mut conn, authorized_device).await;
        if let Err(err) = authorized_device_insert {
            debug!("Error in authorizing machine {machine_id}: {err}");
            (StatusCode::CONFLICT, "Already Exists".to_string())
        } else {
            state.concierge.send(machine_id.to_string()).await;
            (StatusCode::ACCEPTED, "Approved".to_string())
        }
    } else {
        (StatusCode::UNAUTHORIZED, "Invalid Format".to_string())
    }
}
