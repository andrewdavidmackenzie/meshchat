use crate::device_list_view::DeviceListEvent;
use crate::device_list_view::DeviceListEvent::{BLERadioFound, BLERadioLost, Error};
use futures_channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream};
use iced::stream;
use meshtastic::utils::stream::{BleDevice, available_ble_devices};
use std::time::Duration;

/// A stream of [DeviceListEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DeviceListEvent> {
    stream::channel(
        100,
        move |mut gui_sender: Sender<DeviceListEvent>| async move {
            let mut mesh_radio_devices: Vec<BleDevice> = vec![];

            // loop scanning for devices
            loop {
                match available_ble_devices(Duration::from_secs(4)).await {
                    Ok(devices_now) => {
                        // detect lost radios
                        for id in &mesh_radio_devices {
                            if !devices_now.iter().any(|other_id| id == other_id) {
                                // inform GUI of a device lost
                                gui_sender
                                    .send(BLERadioLost(id.clone()))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                            }
                        }

                        // detect new radios found
                        for device in &devices_now {
                            if !mesh_radio_devices
                                .iter()
                                .any(|other_device| device == other_device)
                            {
                                // track it for the future
                                mesh_radio_devices.push(device.clone());

                                // inform GUI of a new device found
                                gui_sender
                                    .send(BLERadioFound(device.clone()))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        gui_sender
                            .send(Error(e.to_string()))
                            .await
                            .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                    }
                }
            }
        },
    )
}
