use crate::device_list_view::DeviceListEvent;
use crate::device_list_view::DeviceListEvent::{BLERadioFound, BLERadioLost, Error};
use btleplug::api::{Central, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::Manager;
use futures::SinkExt;
use futures_channel::mpsc::Sender;
use iced::futures::Stream;
use iced::stream;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

const MSH_SERVICE: Uuid = Uuid::from_u128(0x6ba1b218_15a8_461f_9fa8_5dcae273eafd);

/// A stream of [DeviceListEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DeviceListEvent> {
    stream::channel(
        100,
        move |mut gui_sender: Sender<DeviceListEvent>| async move {
            // Device name and the unseen count
            let mut mesh_radio_devices: HashMap<String, i32> = HashMap::new();

            match Manager::new().await {
                Ok(manager) => {
                    // get the first bluetooth adapter
                    match manager.adapters().await {
                        Ok(adapters) => {
                            let central = adapters.into_iter().next().unwrap();

                            // loop scanning for devices
                            loop {
                                // start scanning for MeshTastic radios
                                match central
                                    .start_scan(ScanFilter {
                                        services: vec![MSH_SERVICE],
                                    })
                                    .await
                                {
                                    Ok(()) => {
                                        match central.peripherals().await {
                                            Ok(peripherals) => {
                                                let mut ble_devices_now: HashSet<String> =
                                                    HashSet::new();

                                                for peripheral in peripherals {
                                                    ble_devices_now.insert(
                                                        peripheral
                                                            .properties()
                                                            .await
                                                            .unwrap()
                                                            .unwrap()
                                                            .local_name
                                                            .unwrap(),
                                                    );
                                                }

                                                println!("Found: {:?}", ble_devices_now);

                                                // detect lost radios
                                                for (ble_device, unseen_count) in
                                                    &mut mesh_radio_devices
                                                {
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
                                                            .unwrap_or_else(|e| {
                                                                eprintln!(
                                                                    "Discovery could not send BLERadioLost: {e}"
                                                                )
                                                            });
                                                    }
                                                }
                                                // Clean up the list of devices, removing ones not seen for 3 cycles
                                                mesh_radio_devices
                                                    .retain(|_device, count| *count >= 3);

                                                // detect new radios found
                                                for device in &ble_devices_now {
                                                    if !mesh_radio_devices.contains_key(device) {
                                                        // track it for the future - starting with an unseen count of 0
                                                        mesh_radio_devices
                                                            .insert(device.clone(), 0);
                                                        // inform GUI of a new device found
                                                        gui_sender
                                                            .send(BLERadioFound(device.clone()))
                                                            .await
                                                            .unwrap_or_else(|e| {
                                                                eprintln!(
                                                                    "Discovery could not send BLERadioFound: {e}"
                                                                )
                                                            });
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                gui_sender
                                                    .send(Error(e.to_string()))
                                                    .await
                                                    .unwrap_or_else(|e| {
                                                        eprintln!("Discovery could not get BT peripherals: {e}")
                                                    });
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        gui_sender.send(Error(e.to_string())).await.unwrap_or_else(
                                            |e| eprintln!("Discovery gui send error: {e}"),
                                        );
                                    }
                                }
                                tokio::time::sleep(Duration::from_secs(4)).await;
                            }
                        }
                        Err(e) => {
                            gui_sender
                                .send(Error(e.to_string()))
                                .await
                                .unwrap_or_else(|e| {
                                    eprintln!("Discovery could not BT adapters: {e}")
                                });
                        }
                    }
                }
                Err(e) => {
                    gui_sender
                        .send(Error(e.to_string()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Discovery could not get BT manager: {e}"));
                }
            }
        },
    )
}
