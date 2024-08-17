use std::{net::SocketAddr, time::Duration};

use alloy::primitives::B256;
use blst::min_pk::PublicKey as BlsPublicKey;
use bytes::Bytes;
use tokio::time::sleep;
use tracing::info;

use dato::{
    bls::random_bls_secret, run_api, CertifiedReadMessageResponse, CertifiedUnavailableMessage,
    Client, ClientSpec, Message, Namespace, Timestamp, Validator, ValidatorIdentity,
};

#[tokio::test]
async fn test_write_request() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator().await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    let identity = ValidatorIdentity::new(0, pubkey);
    client.connect(identity, validator_addr).await?;
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
async fn test_api_write_request() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator().await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    let identity = ValidatorIdentity::new(0, pubkey);
    client.connect(identity, validator_addr).await?;
    info!("Client connected to validator");

    run_api(client, 8089).await?;

    tokio::time::sleep(Duration::from_secs(6000)).await;

    // let namespace = Bytes::from_static(b"test").into();
    // let message = Message(Bytes::from_static(b"made with chatgpt").into());

    // let record = client.write(namespace, message.clone()).await?;
    // info!(?record, "Wrote record");

    // assert_eq!(record.timestamps.len(), 1);
    // assert_eq!(record.message, Some(message));

    Ok(())
}

#[tokio::test]
async fn test_read_request_single_validator() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let (validator_addr, pubkey) = spin_up_validator().await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    let identity = ValidatorIdentity::new(0, pubkey);
    client.connect(identity, validator_addr).await?;
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

    let (validator_addr1, pubkey1) = spin_up_validator().await?;
    info!("Validator 1 listening on: {}", validator_addr1);

    let (validator_addr2, pubkey2) = spin_up_validator().await?;
    info!("Validator 2 listening on: {}", validator_addr2);

    let (validator_addr3, pubkey3) = spin_up_validator().await?;
    info!("Validator 3 listening on: {}", validator_addr3);

    let mut client = Client::new();
    client.connect(ValidatorIdentity::new(0, pubkey1), validator_addr1).await?;
    client.connect(ValidatorIdentity::new(1, pubkey2), validator_addr2).await?;
    client.connect(ValidatorIdentity::new(2, pubkey3), validator_addr3).await?;
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

    let (validator_addr, pubkey) = spin_up_validator().await?;
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    client.connect(ValidatorIdentity::new(0, pubkey), validator_addr).await?;
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

async fn spin_up_validator() -> eyre::Result<(SocketAddr, BlsPublicKey)> {
    let dummy_sk = random_bls_secret();
    let pubkey = dummy_sk.sk_to_pk();
    let mut validator = Validator::new_in_memory(dummy_sk, 0).await?;
    let validator_addr = validator.local_addr().expect("Listening");
    tokio::spawn(async move { validator.run().await });
    Ok((validator_addr, pubkey))
}
