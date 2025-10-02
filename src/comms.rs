use meshtastic::utils::stream::BleId;

pub async fn do_connect(id: BleId) -> Result<BleId, anyhow::Error> {
    println!("Connecting to {}", id);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    Ok(id)
}

pub async fn do_disconnect(id: BleId) -> Result<BleId, anyhow::Error> {
    println!("Disconnecting from {}", id);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    Ok(id)
}
