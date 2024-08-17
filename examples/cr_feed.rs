use std::{sync::Arc, time::Duration};

use clap::Parser;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::{sync::Semaphore, task, time::sleep};
use tracing::{debug, error, info, instrument};

#[derive(Parser, Debug)]
struct Args {
    /// Number of transactions to send
    #[arg(short, long, default_value = "100")]
    num_txns: u64,

    /// API server address
    #[arg(short, long, default_value = "http://localhost:8080")]
    api_server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteRequest {
    namespace: String,
    message: String,
}

/// Send a write request to the API server
async fn send_write_request(client: Arc<Client>, api_server: String, request: WriteRequest) {
    let url = format!("{}/api/v1/write", api_server);

    match client.post(&url).json(&request).send().await {
        Ok(response) => {
            if response.status().is_success() {
                debug!("Successfully sent write request: {:?}", request);
            } else {
                error!("Failed to send write request: {:?}", response.status());
            }
        }
        Err(err) => {
            error!("Request error: {:?}", err);
        }
    }
}

#[instrument]
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let client = Arc::new(Client::new());
    let semaphore = Arc::new(Semaphore::new(100)); // Limit concurrent requests

    for _ in 0..args.num_txns {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let api_server = args.api_server.clone();

        let request = WriteRequest {
            namespace: "default_namespace".to_string(),
            message: generate_random_message(),
        };

        task::spawn(async move {
            send_write_request(client, api_server, request).await;
            drop(permit); // Release semaphore permit
        });

        sleep(Duration::from_millis(50)).await; // Throttle requests
    }

    info!("Finished sending {} transactions", args.num_txns);
}

fn generate_random_message() -> String {
    let mut rng = rand::thread_rng();
    let hex_chars: Vec<char> = "0123456789abcdef".chars().collect();
    format!("0x{}", (0..32).map(|_| hex_chars[rng.gen_range(0..16)]).collect::<String>())
}
