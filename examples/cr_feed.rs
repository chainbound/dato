use clap::Parser;
use futures::StreamExt;
use rand::Rng;
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, Semaphore},
    task,
    time::sleep,
};
use tracing::{debug, error, info};

use dato::CertifiedRecord;

#[derive(Parser, Debug)]
struct Args {
    /// Number of transactions to send
    #[arg(short, long, default_value = "1000")]
    num_txns: u64,

    /// API server address
    #[arg(short, long, default_value = "http://localhost:8000")]
    api_server: String,

    /// Batch size
    #[arg(short, long, default_value = "100")]
    logs_batch_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteRequest {
    namespace: String,
    message: String,
}

/// Send a write request to the API server
async fn send_write_request(
    client: Arc<Client>,
    api_server: String,
    request: WriteRequest,
    timestamps: Arc<Mutex<HashMap<String, Instant>>>,
) {
    let url = format!("{}/api/v1/write", api_server);
    let message_id = request.message.clone();

    match client.post(&url).json(&request).send().await {
        Ok(response) => {
            if response.status().is_success() {
                debug!("Successfully sent write request: {:?}", request);
                let mut ts = timestamps.lock().await;
                ts.insert(message_id, Instant::now());
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
    let timestamps: Arc<Mutex<HashMap<String, Instant>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut subscription =
        EventSource::get(format!("{}/api/v1/subscribe_certified?namespace=dato", args.api_server));

    let timestamps_clone = timestamps.clone();
    tokio::spawn(async move {
        let mut durations = Vec::new();
        while let Some(event) = subscription.next().await {
            match event {
                Ok(Event::Open) => {
                    info!("Subscribed to certified records")
                }
                Ok(Event::Message(msg)) => {
                    let mut record = serde_json::from_str::<CertifiedRecord>(&msg.data).unwrap();
                    let median_timestamp = record.certified_timestamp();
                    // Print the first last and median timestamp

                    let mut ts = timestamps_clone.lock().await;
                    if let Some(start_time) =
                        ts.remove(&alloy::hex::encode_prefixed(record.message.0))
                    {
                        info!(
                            "First timestamp: {:?}  Median timestamp: {:?} Last timestamp: {:?}",
                            record.timestamps[record.timestamps.len() - 1]
                                .duration_since(start_time),
                            median_timestamp.duration_since(start_time),
                            record.timestamps[0].duration_since(start_time),
                        );
                        let duration = start_time.elapsed();
                        durations.push(duration);

                        if durations.len() == args.logs_batch_size as usize {
                            calculate_and_print_statistics(durations.clone());
                            durations.clear();
                        }
                    }
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
        let timestamps = timestamps.clone();

        let request =
            WriteRequest { namespace: "dato".to_string(), message: generate_random_message() };

        task::spawn(async move {
            send_write_request(client, api_server, request, timestamps).await;
            drop(permit); // Release semaphore permit
        });

        sleep(Duration::from_millis(250)).await; // Throttle requests
    }

    info!("Finished sending {} transactions", args.num_txns);
}

fn generate_random_message() -> String {
    let mut rng = rand::thread_rng();
    let hex_chars: Vec<char> = "0123456789abcdef".chars().collect();
    format!("0x{}", (0..32).map(|_| hex_chars[rng.gen_range(0..16)]).collect::<String>())
}

/// Calculate and print the required statistics
fn calculate_and_print_statistics(durations: Vec<Duration>) {
    let count = durations.len() as u32;
    let sum = durations.iter().sum::<Duration>();
    let avg = sum / count;

    info!("Statistics for the batch: Average time: {:?}", avg);
}
