use std::sync::Arc;

use alloy::primitives::Bytes;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{primitives::Request, CertifiedLog, CertifiedRecord, Timestamp};

use super::{Client, ClientSpec};

const WRITE_PATH: &str = "/api/v1/write";
const READ_CERTIFIED_PATH: &str = "/api/v1/read_certified";

pub async fn run_api(client: Client, port: u16) -> eyre::Result<()> {
    let router: Router = Router::new().route(WRITE_PATH, post(write)).with_state(Arc::new(client));

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

#[tracing::instrument(skip(client))]
async fn write(
    State(client): State<Arc<Client>>,
    Json(request): Json<WriteRequest>,
) -> Result<Json<CertifiedRecord>, StatusCode> {
    let namespace = Bytes::from(request.namespace.as_bytes().to_owned());

    client
        .write(namespace, request.message.into())
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
struct ReadParams {
    namespace: String,
    start: Timestamp,
    end: Timestamp,
}

#[tracing::instrument(skip(client))]
async fn read_certified(
    State(client): State<Arc<Client>>,
    params: Query<ReadParams>,
) -> Result<Json<CertifiedLog>, StatusCode> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());

    client
        .read_certified(namespace, params.start, params.end)
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}
