use std::collections::{HashMap};
use tokio::sync::{mpsc, Mutex};

#[derive(Debug)]
pub enum ClientLifecycle {
    Added,
    Revoked,
    Waiting,
}

#[derive(Debug)]
pub enum ServerToClientMessage{
    Lifecycle(ClientLifecycle),
    Ping,
}

pub struct AppState {
    pub connections: Mutex<HashMap<String, mpsc::Sender<ServerToClientMessage>>>,
    pub concierge: mpsc::Sender<String>,
    pub pool: sqlx::PgPool,
}
