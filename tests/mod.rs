use std::time::Duration;

use alloy::primitives::B256;
use bytes::Bytes;
use futures::StreamExt;
use tokio::time::sleep;
use tracing::{debug, error, info};

mod hurl;

mod utils;
use utils::spin_up_validator;

use dato::{
    CertifiedReadMessageResponse, CertifiedUnavailableMessage, Client, ClientSpec, Message,
    Namespace, Timestamp, ValidatorIdentity,
};

#[tokio::test]
async fn test_write_request() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator(Some(231)).await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    let identity = ValidatorIdentity::new(0, pubkey);
    client.connect_validator(identity, validator_addr).await?;
    info!("Client connected to validator");

    let namespace = Bytes::from_static(b"test").into();
    let message = Message(Bytes::from_static(b"made with chatgpt").into());

    let record = client.write(namespace, message.clone()).await?;
    info!(?record, "Wrote record");

    assert_eq!(record.timestamps.len(), 1);
    assert_eq!(record.message, message);

    Ok(())
}

#[tokio::test]
async fn test_read_request_single_validator() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator(Some(231)).await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    let identity = ValidatorIdentity::new(0, pubkey);
    client.connect_validator(identity, validator_addr).await?;
    info!("Client connected to validator");

    let namespace: Namespace = Bytes::from_static(b"test").into();
    let message = Message(Bytes::from_static(b"made with chatgpt").into());

    let start = Timestamp::now();
    let record = client.write(namespace.clone(), message).await?;
    info!(?record, "Wrote record");

    assert_eq!(record.timestamps.len(), 1);

    sleep(Duration::from_millis(300)).await;
    let end = Timestamp::now();

    let log = client.read(namespace, start, end).await?;
    info!(?log, "Read log");

    assert_eq!(log.records.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_read_request_multiple_validators() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr1, pubkey1) = spin_up_validator(Some(231)).await?;
    info!("Validator 1 listening on: {}", validator_addr1);

    let (validator_addr2, pubkey2) = spin_up_validator(Some(242)).await?;
    info!("Validator 2 listening on: {}", validator_addr2);

    let (validator_addr3, pubkey3) = spin_up_validator(Some(253)).await?;
    info!("Validator 3 listening on: {}", validator_addr3);

    let mut client = Client::new();
    client.connect_validator(ValidatorIdentity::new(0, pubkey1), validator_addr1).await?;
    client.connect_validator(ValidatorIdentity::new(1, pubkey2), validator_addr2).await?;
    client.connect_validator(ValidatorIdentity::new(2, pubkey3), validator_addr3).await?;
    info!("Client connected to validators");

    let namespace: Namespace = Bytes::from_static(b"test").into();
    let message = Message(Bytes::from_static(b"made with chatgpt").into());

    let start = Timestamp::now();
    let record = client.write(namespace.clone(), message.clone()).await?;
    info!(?record, "Wrote record");

    // we expect 2 instead of 3 because the quorum is 2/3
    assert_eq!(record.timestamps.len(), 2);

    sleep(Duration::from_millis(300)).await;
    let end = Timestamp::now();

    let log = client.read(namespace, start, end).await?;
    info!(?log, "Read log");

    assert_eq!(log.records.len(), 3);

    Ok(())
}

#[tokio::test]
async fn test_read_unavailable_message() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator(Some(231)).await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    client.connect_validator(ValidatorIdentity::new(0, pubkey), validator_addr).await?;
    info!("Client connected to validators");

    let namespace: Namespace = Bytes::from_static(b"test").into();
    let msg_id = B256::ZERO;

    let log = client.read_message(namespace, msg_id).await?;
    info!(?log, "Read log");

    match log {
        CertifiedReadMessageResponse::Unavailable(CertifiedUnavailableMessage { .. }) => {}
        _ => eyre::bail!("Expected UnavailableMessage"),
    }

    Ok(())
}

#[tokio::test]
async fn test_read_certified() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator(Some(231)).await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    client.connect_validator(ValidatorIdentity::new(0, pubkey), validator_addr).await?;
    info!("Client connected to validators");

    let namespace: Namespace = Bytes::from_static(b"test").into();
    let message = Message(Bytes::from_static(b"made with chatgpt").into());

    let start = Timestamp::now();
    let record = client.write(namespace.clone(), message.clone()).await?;
    info!(?record, "Wrote record");

    assert_eq!(record.timestamps.len(), 1);

    sleep(Duration::from_millis(300)).await;
    let end = Timestamp::now();

    let log = client.read_certified(namespace, start, end).await?;
    info!(?log, "Read log");

    assert_eq!(log.records.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_subscribe() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator(Some(231)).await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    client.connect_validator(ValidatorIdentity::new(0, pubkey), validator_addr).await?;

    let namespace: Namespace = Bytes::from_static(b"test").into();
    let mut stream = client.subscribe(namespace.clone()).await?;
    info!("Subscribed to namespace");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let message = Message(Bytes::from_static(b"made with chatgpt").into());
    let record = client.write(namespace.clone(), message).await?;
    debug!(?record, "Wrote record 1");

    let received = stream.next().await.expect("Received message");
    assert_eq!(received.message, record.message);
    debug!(?received, "Received message");

    let message = Message(Bytes::from_static(b"made with chatgpt 2").into());
    let record = client.write(namespace.clone(), message).await?;
    debug!(?record, "Wrote record 2");

    let received = stream.next().await.expect("Received message");
    assert_eq!(received.message, record.message);
    debug!(?received, "Received message");

    Ok(())
}

#[tokio::test]
async fn test_subscribe_certified() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator(Some(231)).await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    client.connect_validator(ValidatorIdentity::new(0, pubkey), validator_addr).await?;

    let namespace: Namespace = Bytes::from_static(b"test").into();
    let mut stream = client.subscribe_certified(namespace.clone()).await?;
    info!("Subscribed to certified namespace");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let message = Message(Bytes::from_static(b"made with chatgpt").into());
    let record = client.write(namespace.clone(), message).await?;
    debug!(?record, "Wrote record 1");

    let received = stream.next().await.expect("Received message");
    assert_eq!(received.message, record.message);

    Ok(())
}

#[tokio::test]
async fn test_subscribe_certified_many() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr1, pubkey1) = spin_up_validator(Some(231)).await?;
    info!("Validator 1 listening on: {}", validator_addr1);

    let (validator_addr2, pubkey2) = spin_up_validator(Some(232)).await?;
    info!("Validator 2 listening on: {}", validator_addr2);

    let (validator_addr3, pubkey3) = spin_up_validator(Some(233)).await?;
    info!("Validator 3 listening on: {}", validator_addr3);

    let mut client = Client::new();
    client.connect_validator(ValidatorIdentity::new(0, pubkey1), validator_addr1).await?;
    client.connect_validator(ValidatorIdentity::new(1, pubkey2), validator_addr2).await?;
    client.connect_validator(ValidatorIdentity::new(2, pubkey3), validator_addr3).await?;
    info!("Client connected to validators");

    let namespace: Namespace = Bytes::from_static(b"test").into();
    let mut stream = client.subscribe_certified(namespace.clone()).await?;
    info!("Subscribed to certified namespace");

    tokio::time::sleep(Duration::from_millis(300)).await;

    for i in 1..=25 {
        let message_content = format!("message {}", i);
        let message = Message(Bytes::from(message_content.clone()).into());
        let record = client.write(namespace.clone(), message).await?;
        debug!(?record, "Wrote record {}", i);

        if let Some(received) = stream.next().await {
            info!("Received certified record {}: {:?}", i, received);
            assert_eq!(received.message, record.message);
        } else {
            error!("Failed to receive certified record {}", i);
        }
    }

    Ok(())
}
