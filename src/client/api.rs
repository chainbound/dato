use std::{convert::Infallible, pin::Pin, sync::Arc, time::Duration};

use alloy::primitives::{Bytes, B256};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    BoxError, Json, Router,
};
use futures::{stream::once, Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, instrument};

use crate::{
    primitives::Request, CertifiedLog, CertifiedReadMessageResponse, CertifiedRecord, Log,
    Timestamp,
};

use super::{Client, ClientSpec};

const WRITE_PATH: &str = "/api/v1/write";
const READ_PATH: &str = "/api/v1/read";
const READ_CERTIFIED_PATH: &str = "/api/v1/read_certified";
const READ_MESSAGE_PATH: &str = "/api/v1/read_message";
const SUBSCRIBE_PATH: &str = "/api/v1/subscribe";
const SUBSCRIBE_CERTIFIED_PATH: &str = "/api/v1/subscribe_certified";

impl Client {
    pub async fn run_api(self, port: u16) -> std::io::Result<JoinHandle<()>> {
        let router: Router = Router::new()
            .route(WRITE_PATH, post(write))
            .route(READ_PATH, get(read))
            .route(READ_CERTIFIED_PATH, get(read_certified))
            .route(READ_MESSAGE_PATH, get(read_message))
            .route(SUBSCRIBE_PATH, get(subscribe))
            .route(SUBSCRIBE_CERTIFIED_PATH, get(subscribe_certified))
            .with_state(Arc::new(self));

        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;

        let addr = listener.local_addr()?;

        info!("API server running on {addr}");

        Ok(tokio::spawn(async move {
            if let Err(err) = axum::serve(listener, router).await {
                error!(?err, "API Server error");
            }
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteRequest {
    namespace: String,
    message: Bytes,
}

#[instrument(skip(client, request))]
async fn write(
    State(client): State<Arc<Client>>,
    Json(request): Json<WriteRequest>,
) -> Result<Json<CertifiedRecord>, StatusCode> {
    let namespace = Bytes::from(request.namespace.as_bytes().to_owned());
    debug!(namespace = %request.namespace, "New write request");

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

#[instrument(skip(client, params))]
async fn read(
    State(client): State<Arc<Client>>,
    Query(params): Query<ReadParams>,
) -> Result<Json<Log>, StatusCode> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    debug!(namespace = %params.namespace, "New read request");

    client
        .read(namespace, params.start.into(), params.end.into())
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}

#[instrument(skip(client, params))]
async fn read_certified(
    State(client): State<Arc<Client>>,
    Query(params): Query<ReadParams>,
) -> Result<Json<CertifiedLog>, StatusCode> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    debug!(namespace = %params.namespace, "New read_certified request");

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

#[instrument(skip(client, params))]
async fn read_message(
    State(client): State<Arc<Client>>,
    Query(params): Query<ReadMessageParams>,
) -> Result<Json<CertifiedReadMessageResponse>, StatusCode> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    debug!("New read_message request for namespace: {namespace}");

    client
        .read_message(namespace, params.msg_id)
        .await
        .map(Json)
        .map_err(|e| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
struct NamespaceParams {
    namespace: String,
}

#[instrument(skip(client, params))]
async fn subscribe(
    State(client): State<Arc<Client>>,
    Query(params): Query<NamespaceParams>,
) -> Sse<impl Stream<Item = Result<Event, BoxError>>> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    debug!("New subscribe request for namespace: {namespace}");

    let record_stream = match client.subscribe(namespace).await {
        Ok(stream) => stream,
        Err(e) => {
            error!(?e, "Failed to subscribe to namespace");
            // TODO: fix error handling here, compiler error if doing the thing below
            // let stream = once(async { Err(BoxError::from("Internal server error")) });
            // return Sse::new(stream);
            panic!();
        }
    };

    let filtered = record_stream.map(|record| match serde_json::to_string(&record) {
        Ok(json) => {
            Ok(Event::default().data(json).event("record").retry(Duration::from_millis(50)))
        }
        Err(err) => {
            error!(?err, "Failed to serialize record");
            Err(BoxError::from("Internal server error"))
        }
    });

    Sse::new(filtered).keep_alive(KeepAlive::default())
}

#[instrument(skip(client, params))]
async fn subscribe_certified(
    State(client): State<Arc<Client>>,
    Query(params): Query<NamespaceParams>,
) -> Sse<impl Stream<Item = Result<Event, BoxError>>> {
    let namespace = Bytes::from(params.namespace.as_bytes().to_owned());
    debug!("New subscribe request for namespace: {namespace}");

    let certified_record_stream = match client.subscribe_certified(namespace).await {
        Ok(stream) => stream,
        Err(e) => {
            error!(?e, "Failed to subscribe to namespace");
            // TODO: fix error handling here, compiler error if doing the thing below
            // let stream = once(async { Err(BoxError::from("Internal server error")) });
            // return Sse::new(stream);
            panic!();
        }
    };

    let filtered = certified_record_stream.map(|record| match serde_json::to_string(&record) {
        Ok(json) => {
            Ok(Event::default().data(json).event("record").retry(Duration::from_millis(50)))
        }
        Err(err) => {
            error!(?err, "Failed to serialize record");
            Err(BoxError::from("Internal server error"))
        }
    });

    Sse::new(filtered).keep_alive(KeepAlive::default())
}
