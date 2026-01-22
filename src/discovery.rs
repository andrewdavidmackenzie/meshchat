use crate::device_list_view::DeviceListEvent;
use crate::device_list_view::DeviceListEvent::{BLERadioFound, BLERadioLost, Error};
use futures_channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream};
use iced::stream;
use meshtastic::utils::stream::available_ble_devices;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// A stream of [DeviceListEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DeviceListEvent> {
    stream::channel(
        100,
        move |mut gui_sender: Sender<DeviceListEvent>| async move {
            // Device name and the unseen count
            let mut mesh_radio_devices: HashMap<String, i32> = HashMap::new();

            // loop scanning for devices
            loop {
                match available_ble_devices(Duration::from_secs(4)).await {
                    Ok(devices_now) => {
                        let ble_devices_now: HashSet<String> = devices_now
                            .iter()
                            .map(|ble_device| {
                                ble_device
                                    .name
                                    .clone()
                                    .unwrap_or(ble_device.mac_address.to_string())
                            })
                            .collect();

                        // detect lost radios
                        for (ble_device, unseen_count) in &mut mesh_radio_devices {
                            if !ble_devices_now.contains(ble_device) {
                                *unseen_count += 1;
                                println!("'{}' Unseen once", ble_device);
                            }

                            // if unseen 3 times, then notify
                            if *unseen_count >= 3 {
                                println!("'{}' Unseen 3 times", ble_device);
                                gui_sender
                                    .send(BLERadioLost(ble_device.clone()))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                            }
                        }
                        // Clean up the list of devices, removing ones not seen for 3 cycles
                        mesh_radio_devices.retain(|_device, count| *count >= 3);

                        // detect new radios found
                        for device in &ble_devices_now {
                            if !mesh_radio_devices.contains_key(device) {
                                // track it for the future - starting with an unseen count of 0
                                mesh_radio_devices.insert(device.clone(), 0);
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
