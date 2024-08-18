use clap::Parser;
use futures::StreamExt;
use rand::Rng;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, task, time::sleep};
use tracing::*;

use dato::CertifiedRecord;

#[derive(Parser, Debug)]
struct Args {
    /// Number of transactions to send
    #[arg(short, long, default_value = "100")]
    num_txns: u64,

    /// API server address
    #[arg(short, long, default_value = "http://localhost:8000")]
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

/// Demo game plan:
/// 1. Subscribe to certified records
/// 2. Start sending write requests and saving the digest + timestamp
/// 3. Aggregate a batch of n responses, and print:
///    - Average time between write request and the certified record event
///    - Median time between write request and the certified record event
///   P1, P(2x/3) and then certified timestamp
#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let args = Args::parse();
    let client = Arc::new(Client::new());
    let semaphore = Arc::new(Semaphore::new(100)); // Limit concurrent requests

    let mut subscription =
        EventSource::get(format!("{}/api/v1/subscribe_certified?namespace=dato", args.api_server));

    tokio::spawn(async move {
        while let Some(event) = subscription.next().await {
            match event {
                Ok(Event::Open) => {
                    info!("Subscribed to certified records")
                }
                Ok(Event::Message(msg)) => {
                    let record = serde_json::from_str::<CertifiedRecord>(&msg.data).unwrap();
                    info!("Received certified record: {:?}", record);
                }
                Err(err) => {
                    error!("Event source error: {:?}", err);
                }
            }
        }
    });

    for _ in 0..args.num_txns {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let api_server = args.api_server.clone();

        let request =
            WriteRequest { namespace: "dato".to_string(), message: generate_random_message() };

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
