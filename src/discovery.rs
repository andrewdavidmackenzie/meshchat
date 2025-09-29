use std::time::Duration;
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, PeripheralId};

use iced::futures::{SinkExt, Stream};
use iced::stream;
use uuid::Uuid;

const MSH_SERVICE: Uuid = Uuid::from_u128(0x6ba1b218_15a8_461f_9fa8_5dcae273eafd);

#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    BLERadioFound(PeripheralId, String),
    BLERadioLost(PeripheralId),
}

/// A stream of [DiscoveryEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DiscoveryEvent> {
    stream::channel(100, move |mut gui_sender| async move {
        // get the first bluetooth adapter
        let manager = Manager::new().await.unwrap();
        let adapters = manager.adapters().await.unwrap();
        let bt_adapter = adapters.into_iter().next().unwrap();

        let mut mesh_radio_ids : Vec<PeripheralId> = vec![];

        // loop scanning for devices
        loop {
            bt_adapter
                .start_scan(ScanFilter {
                    services: vec![MSH_SERVICE],
                })
                .await.unwrap();

            tokio::time::sleep(Duration::from_secs(1)).await;            let radios_now = bt_adapter.peripherals().await.unwrap();

            let radios_now_ids: Vec<PeripheralId> = radios_now.iter().map(|p| p.id()).collect::<Vec<PeripheralId>>();

            // detect lost radios
            for id in &mesh_radio_ids {
                if ! radios_now_ids.contains(id) {
                    // inform GUI of a new device found
                    gui_sender
                        .send(DiscoveryEvent::BLERadioLost(id.clone()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }

            }

            // detect new radios found
            for p in &radios_now {
                if ! mesh_radio_ids.contains(&p.id()) &&
                    let Some(name) = p.properties().await.unwrap().unwrap().local_name {
                        // track it for the future
                        mesh_radio_ids.push(p.id());

                        // inform GUI of a new device found
                        gui_sender
                            .send(DiscoveryEvent::BLERadioFound(p.id().clone(), name.clone()))
                            .await
                            .unwrap_or_else(|e| eprintln!("Send error: {e}"));
                }
            }
        }
    })
}