use std::sync::Arc;

use alloy::primitives::{Bytes, B256};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    primitives::Request, CertifiedLog, CertifiedReadMessageResponse, CertifiedRecord, Log,
    Timestamp,
};

use super::{Client, ClientSpec};

const WRITE_PATH: &str = "/api/v1/write";
const READ_PATH: &str = "/api/v1/read";
const READ_CERTIFIED_PATH: &str = "/api/v1/read_certified";
const READ_MESSAGE_PATH: &str = "/api/v1/read_message";

pub async fn run_api(client: Client, port: u16) -> eyre::Result<()> {
    let router: Router = Router::new()
        .route(WRITE_PATH, post(write))
        .route(READ_PATH, get(read))
        .route(READ_CERTIFIED_PATH, get(read_certified))
        .route(READ_MESSAGE_PATH, get(read_message))
        .with_state(Arc::new(client));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;

    let addr = listener.local_addr().expect("Failed to get local address");

    tracing::info!("API server running on {addr}");

    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router).await {
            tracing::error!(?err, "Commitments API Server error");
        }
    });

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteRequest {
    namespace: String,
    message: Bytes,
}

#[tracing::instrument(skip(client, request))]
async fn write(
    State(client): State<Arc<Client>>,
    Json(request): Json<WriteRequest>,
) -> Result<Json<CertifiedRecord>, StatusCode> {
    let namespace = Bytes::from(request.namespace.as_bytes().to_owned());
    tracing::debug!("New write request for namespace: {namespace}");

    client
        .write(namespace, request.message.into())
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
struct ReadParams {
    namespace: String,
    start: u64,
    end: u64,
}

#[tracing::instrument(skip(client, params))]
async fn read(
    State(client): State<Arc<Client>>,
    params: Query<ReadParams>,
) -> Result<Json<Log>, StatusCode> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    tracing::debug!("New read request for namespace: {namespace}");

    client
        .read(namespace, params.start.into(), params.end.into())
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}

#[tracing::instrument(skip(client, params))]
async fn read_certified(
    State(client): State<Arc<Client>>,
    params: Query<ReadParams>,
) -> Result<Json<CertifiedLog>, StatusCode> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    tracing::debug!("New read_certified request for namespace: {namespace}");

    client
        .read_certified(namespace, params.start.into(), params.end.into())
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
struct ReadMessageParams {
    namespace: String,
    msg_id: B256,
}

#[tracing::instrument(skip(client, params))]
async fn read_message(
    State(client): State<Arc<Client>>,
    params: Query<ReadMessageParams>,
) -> Result<Json<CertifiedReadMessageResponse>, StatusCode> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    tracing::debug!("New read_message request for namespace: {namespace}");

    client
        .read_message(namespace, params.msg_id)
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}
