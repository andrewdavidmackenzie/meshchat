use iced::futures::{SinkExt, Stream};
use iced::stream;
use meshtastic::utils::stream::available_ble_devices;
use meshtastic::utils::stream::BleId;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    BLERadioFound(BleId),
    BLERadioLost(BleId),
    Error(String),
}

pub fn compare_bleid(left: &BleId, right: &BleId) -> bool {
    match left {
        BleId::Name(left_name) => match right {
            BleId::Name(right_name) => left_name == right_name,
            BleId::MacAddress(_) => false,
            BleId::NameAndMac(right_name, _) => left_name == right_name,
        },
        BleId::MacAddress(left_address) => match right {
            BleId::Name(_) => false,
            BleId::MacAddress(right_address) => left_address == right_address,
            BleId::NameAndMac(_, right_address) => left_address == right_address,
        },
        BleId::NameAndMac(left_name, left_address) => match right {
            BleId::Name(right_name) => left_name == right_name,
            BleId::MacAddress(right_address) => left_address == right_address,
            BleId::NameAndMac(right_name, right_address) => {
                left_name == right_name && left_address == right_address
            }
        },
    }
}

/// A stream of [DiscoveryEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DiscoveryEvent> {
    stream::channel(100, move |mut gui_sender| async move {
        let mut mesh_radio_ids: Vec<BleId> = vec![];

        // loop scanning for devices
        loop {
            match available_ble_devices(Duration::from_secs(4)).await {
                Ok(radios_now_ids) => {
                    // detect lost radios
                    for id in &mesh_radio_ids {
                        if !radios_now_ids
                            .iter()
                            .any(|other_id| compare_bleid(id, other_id))
                        {
                            // inform GUI of a device lost
                            gui_sender
                                .send(DiscoveryEvent::BLERadioLost(id.clone()))
                                .await
                                .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                        }
                    }

                    // detect new radios found
                    for id in &radios_now_ids {
                        if !mesh_radio_ids
                            .iter()
                            .any(|other_id| compare_bleid(id, other_id))
                        {
                            // track it for the future
                            mesh_radio_ids.push(id.clone());

                            // inform GUI of a new device found
                            gui_sender
                                .send(DiscoveryEvent::BLERadioFound(id.clone()))
                                .await
                                .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                        }
                    }
                }
                Err(e) => {
                    gui_sender
                        .send(DiscoveryEvent::Error(e.to_string()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                }
            }
        }
    })
}
