use crate::device::DeviceIdentifier;
use crate::device_list::DeviceListEvent;
use crate::device_list::DeviceListEvent::{
    CriticalError, Error, MeshRadioFound, MeshRadioLost, Scanning,
};
use crate::device_list::RadioType;
#[cfg(feature = "meshcore")]
use crate::meshc::MESHCORE_SERVICE_UUID;
#[cfg(feature = "meshtastic")]
use crate::mesht::MESHTASTIC_SERVICE_UUID;
use btleplug::api::{Central, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager};
use futures::SinkExt;
use futures_channel::mpsc::Sender;
use iced::futures::Stream;
use iced::stream;
#[cfg(feature = "meshtastic")]
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
#[cfg(feature = "meshtastic")]
use std::net::IpAddr;
use std::time::Duration;
use uuid::Uuid;

/// mDNS service type advertised by Meshtastic devices for their TCP API (port 4403 by default).
#[cfg(feature = "meshtastic")]
const MESHTASTIC_MDNS_SERVICE: &str = "_meshtastic._tcp.local.";

/// A stream of [DeviceListEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DeviceListEvent> {
    #[allow(unused_mut)]
    let mut service_filter: Vec<Uuid> = vec![];
    #[cfg(feature = "meshtastic")]
    service_filter.push(MESHTASTIC_SERVICE_UUID);
    #[cfg(feature = "meshcore")]
    service_filter.push(MESHCORE_SERVICE_UUID);

    stream::channel(
        100,
        move |mut gui_sender: Sender<DeviceListEvent>| async move {
            match Manager::new().await {
                Ok(manager) => {
                    // get the first bluetooth adapter
                    match manager.adapters().await {
                        Ok(adapters) => match adapters.into_iter().next() {
                            Some(adapter) => {
                                // start scanning for MeshTastic radios
                                match adapter.start_scan(ScanFilter::default()).await {
                                    Ok(()) => scan_for_devices(&mut gui_sender, &adapter).await,
                                    Err(e) => {
                                        gui_sender.send(Error(e.to_string())).await.unwrap_or_else(
                                            |e| eprintln!("Discovery gui send error: {e}"),
                                        );
                                    }
                                }
                            }
                            None => {
                                gui_sender
                                    .send(CriticalError(
                                        "Discovery could not get a BT Adapter".into(),
                                    ))
                                    .await
                                    .unwrap_or_else(|e| {
                                        eprintln!("Discovery could not find a BT adapters: {e}")
                                    });
                            }
                        },
                        Err(e) => {
                            gui_sender
                                .send(CriticalError(e.to_string()))
                                .await
                                .unwrap_or_else(|e| {
                                    eprintln!("Discovery could not get first BT adapter: {e}")
                                });
                        }
                    }
                }
                Err(e) => {
                    gui_sender
                        .send(CriticalError(e.to_string()))
                        .await
                        .unwrap_or_else(|e| eprintln!("Discovery could not get BT manager: {e}"));
                }
            }
        },
    )
}

async fn scan_for_devices(gui_sender: &mut Sender<DeviceListEvent>, adapter: &Adapter) {
    // Device name -> (unseen count, radio type)
    let mut mesh_radio_devices: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

    gui_sender
        .send(Scanning(true))
        .await
        .unwrap_or_else(|e| eprintln!("Discovery could not send Scanning(true): {e}"));

    // loop scanning for devices
    loop {
        match adapter.peripherals().await {
            Ok(peripherals) => {
                announce_device_changes(gui_sender, &peripherals, &mut mesh_radio_devices).await;
            }
            Err(e) => {
                gui_sender
                    .send(Error(e.to_string()))
                    .await
                    .unwrap_or_else(|e| eprintln!("Discovery could not get BT peripherals: {e}"));
            }
        }
        tokio::time::sleep(Duration::from_secs(4)).await;
    }
}

const DEFAULT_MAC: [u8; 6] = [0x00; 6];

async fn announce_device_changes(
    gui_sender: &mut Sender<DeviceListEvent>,
    peripherals: &Vec<impl Peripheral>,
    mesh_radio_devices: &mut HashMap<DeviceIdentifier, (i32, RadioType)>,
) {
    // Map device name -> RadioType
    let mut ble_devices_now: HashMap<DeviceIdentifier, RadioType> = HashMap::new();

    for peripheral in peripherals {
        #[allow(clippy::collapsible_if)]
        if let Ok(Some(properties)) = peripheral.properties().await {
            let identifier = DeviceIdentifier::Ble {
                name: properties.local_name,
                mac: match properties.address.into_inner() {
                    DEFAULT_MAC => None,
                    _ => Some(properties.address),
                },
            };

            // Detect radio type from service UUIDs
            if let Some(radio_type) = detect_radio_type(&properties.services) {
                ble_devices_now.insert(identifier, radio_type);
            }
        }
    }

    let (found, lost) = process_device_changes(&ble_devices_now, mesh_radio_devices);

    // Send lost events
    for device in lost {
        gui_sender
            .send(MeshRadioLost(device))
            .await
            .unwrap_or_else(|e| eprintln!("Discovery could not send MeshRadioLost: {e}"));
    }

    // Send found events with the appropriate radio type
    #[allow(unused_variables)]
    for (device, radio_type) in found {
        gui_sender
            .send(MeshRadioFound(device, radio_type))
            .await
            .unwrap_or_else(|e| eprintln!("Discovery could not send BLEMeshtasticRadioFound: {e}"));
    }
}

/// Detect the radio type from the service UUIDs advertised by the peripheral
#[allow(unused_variables)]
fn detect_radio_type(services: &[Uuid]) -> Option<RadioType> {
    #[cfg(feature = "meshtastic")]
    if services.contains(&MESHTASTIC_SERVICE_UUID) {
        return Some(RadioType::Meshtastic);
    }
    #[cfg(feature = "meshcore")]
    if services.contains(&MESHCORE_SERVICE_UUID) {
        return Some(RadioType::MeshCore);
    }

    None
}

/// Process device changes and return events to send.
/// Returns (devices_found with radio type, devices_lost)
fn process_device_changes(
    current_devices: &HashMap<DeviceIdentifier, RadioType>,
    tracked_devices: &mut HashMap<DeviceIdentifier, (i32, RadioType)>,
) -> (Vec<(DeviceIdentifier, RadioType)>, Vec<DeviceIdentifier>) {
    let mut found = Vec::new();
    let mut lost = Vec::new();

    // detect lost radios
    for (device_identifier, (unseen_count, _radio_type)) in tracked_devices.iter_mut() {
        if current_devices.contains_key(device_identifier) {
            // Reset count if the device is seen again
            *unseen_count = 0;
        } else {
            *unseen_count += 1;
        }

        // if unseen 3 times, then consider lost
        if *unseen_count >= 3 {
            lost.push(device_identifier.clone());
        }
    }

    // Clean up the list of devices, removing ones not seen for 3 cycles
    tracked_devices.retain(|_device, (unseen_count, _)| *unseen_count < 3);

    // detect new radios found
    for (device, radio_type) in current_devices {
        if !tracked_devices.contains_key(device) {
            // track it for the future - starting with an unseen count of 0
            tracked_devices.insert(device.clone(), (0, *radio_type));
            found.push((device.clone(), *radio_type));
        }
    }

    (found, lost)
}

/// A stream of [DeviceListEvent] announcing the discovery or loss of Meshtastic devices reachable
/// over TCP on the local network. Devices advertise themselves via mDNS-SD as
/// `_meshtastic._tcp.local.`.
#[cfg(feature = "meshtastic")]
pub fn mdns_discovery() -> impl Stream<Item = DeviceListEvent> {
    stream::channel(
        100,
        move |mut gui_sender: Sender<DeviceListEvent>| async move {
            let daemon = match ServiceDaemon::new() {
                Ok(d) => d,
                Err(e) => {
                    let _ = gui_sender
                        .send(Error(format!("mDNS daemon could not start: {e}")))
                        .await;
                    return;
                }
            };

            let receiver = match daemon.browse(MESHTASTIC_MDNS_SERVICE) {
                Ok(r) => r,
                Err(e) => {
                    let _ = gui_sender
                        .send(Error(format!("mDNS browse failed: {e}")))
                        .await;
                    return;
                }
            };

            // Track fullname → identifier so that ServiceRemoved events can be mapped back to
            // the same identifier that was emitted on ServiceResolved.
            let mut resolved: HashMap<String, DeviceIdentifier> = HashMap::new();

            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let Some(host) = info
                            .get_addresses()
                            .iter()
                            .find_map(|addr| match addr {
                                IpAddr::V4(v4) => Some(v4.to_string()),
                                _ => None,
                            })
                            .or_else(|| {
                                info.get_addresses()
                                    .iter()
                                    .next()
                                    .map(|addr| addr.to_string())
                            })
                        else {
                            continue;
                        };
                        let port = info.get_port();
                        // Friendly name = the service instance label, e.g. "MyMeshy" from
                        // "MyMeshy._meshtastic._tcp.local."
                        let fullname = info.get_fullname().to_string();
                        let name = fullname
                            .strip_suffix(MESHTASTIC_MDNS_SERVICE)
                            .and_then(|s| s.strip_suffix('.'))
                            .map(|s| s.to_string());

                        let identifier = DeviceIdentifier::Tcp { name, host, port };
                        if resolved.insert(fullname, identifier.clone()).as_ref()
                            != Some(&identifier)
                        {
                            let _ = gui_sender
                                .send(MeshRadioFound(identifier, RadioType::Meshtastic))
                                .await;
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        if let Some(identifier) = resolved.remove(&fullname) {
                            let _ = gui_sender.send(MeshRadioLost(identifier)).await;
                        }
                    }
                    _ => {}
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "meshtastic")]
    /// Helper to create a HashMap of device names with default RadioType
    fn devices(items: &[&str]) -> HashMap<DeviceIdentifier, RadioType> {
        items
            .iter()
            .map(|s| (DeviceIdentifier::from(*s), RadioType::Meshtastic))
            .collect()
    }

    #[cfg(feature = "meshtastic")]
    /// Helper to create a tracked device's HashMap entry
    fn tracked_device(name: &str, unseen_count: i32) -> (DeviceIdentifier, (i32, RadioType)) {
        (
            DeviceIdentifier::from(name),
            (unseen_count, RadioType::Meshtastic),
        )
    }

    // Test discovering new devices
    #[test]
    fn test_new_device_found() {
        let current = devices(&["Device1"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device1"));
        assert!(lost.is_empty());
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        );
    }

    #[test]
    fn test_multiple_new_devices_found() {
        let current = devices(&["Device1", "Device2", "Device3"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 3);
        let found_names: Vec<_> = found.iter().map(|(name, _)| String::from(name)).collect();
        assert!(found_names.contains(&"Device1".to_string()));
        assert!(found_names.contains(&"Device2".to_string()));
        assert!(found_names.contains(&"Device3".to_string()));
        assert!(lost.is_empty());
        assert_eq!(tracked.len(), 3);
    }

    #[test]
    fn test_no_devices() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert!(tracked.is_empty());
    }

    // Test device still present
    #[test]
    fn test_device_still_present() {
        let current = devices(&["Device1"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 0).0,
            tracked_device("Device1", 0).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        );
    }

    #[test]
    fn test_device_reappears_resets_count() {
        let current = devices(&["Device1"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        // Count should be reset to 0
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        );
    }

    // Test device disappearing
    #[test]
    fn test_device_unseen_once() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 0).0,
            tracked_device("Device1", 0).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty()); // Not lost yet, only unseen once
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(1)
        );
    }

    #[test]
    fn test_device_unseen_twice() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 1).0,
            tracked_device("Device1", 1).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty()); // Not lost yet, only unseen twice
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(2)
        );
    }

    #[test]
    fn test_device_lost_after_three_unseen() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert_eq!(lost, vec![DeviceIdentifier::from("Device1")]);
        // Device should be removed from tracking
        assert!(!tracked.contains_key(&DeviceIdentifier::from("Device1")));
    }

    #[test]
    fn test_device_removed_from_tracking_after_lost() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );

        process_device_changes(&current, &mut tracked);

        assert!(tracked.is_empty());
    }

    // Test mixed scenarios
    #[test]
    fn test_one_found_one_still_present() {
        let current = devices(&["Device1", "Device2"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 0).0,
            tracked_device("Device1", 0).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device2"));
        assert!(lost.is_empty());
        assert_eq!(tracked.len(), 2);
    }

    #[test]
    fn test_one_found_one_disappearing() {
        let current = devices(&["Device2"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 0).0,
            tracked_device("Device1", 0).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device2"));
        assert!(lost.is_empty()); // Device1 not lost yet
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(1)
        );
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device2"))
                .map(|(c, _)| *c),
            Some(0)
        );
    }

    #[test]
    fn test_one_found_one_lost() {
        let current = devices(&["Device2"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device2"));
        assert_eq!(lost, vec![DeviceIdentifier::from("Device1")]);
        assert!(!tracked.contains_key(&DeviceIdentifier::from("Device1")));
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device2"))
                .map(|(c, _)| *c),
            Some(0)
        );
    }

    #[test]
    fn test_multiple_devices_different_states() {
        let current = devices(&["Device1", "Device4"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 1).0,
            tracked_device("Device1", 1).1,
        );
        tracked.insert(
            tracked_device("Device2", 0).0,
            tracked_device("Device2", 0).1,
        );
        tracked.insert(
            tracked_device("Device3", 2).0,
            tracked_device("Device3", 2).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device4"));
        assert_eq!(lost, vec![DeviceIdentifier::from("Device3")]);
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        ); // Reset
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device2"))
                .map(|(c, _)| *c),
            Some(1)
        ); // Incremented
        assert!(!tracked.contains_key(&DeviceIdentifier::from("Device3"))); // Removed
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device4"))
                .map(|(c, _)| *c),
            Some(0)
        ); // New
    }

    // Test the full lifecycle
    #[test]
    fn test_device_full_lifecycle() {
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        // Cycle 1: Device appears
        let current = devices(&["Device1"]);
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device1"));
        assert!(lost.is_empty());
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        );

        // Cycle 2: Device still present
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        );

        // Cycle 3: Device disappears (unseen 1)
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(1)
        );

        // Cycle 4: Device still gone (unseen 2)
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(2)
        );

        // Cycle 5: Device still gone (unseen 3 - lost)
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert_eq!(lost, vec![DeviceIdentifier::from("Device1")]);
        assert!(!tracked.contains_key(&DeviceIdentifier::from("Device1")));
    }

    #[test]
    fn test_device_reappears_during_disappearing() {
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );

        // Device reappears just in time
        let current = devices(&["Device1"]);
        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty()); // Not new, was tracked
        assert!(lost.is_empty()); // Not lost, reappeared
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        ); // Count reset
    }

    #[test]
    fn test_lost_device_can_be_found_again() {
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        // Device appears
        let current = devices(&["Device1"]);
        process_device_changes(&current, &mut tracked);
        assert!(tracked.contains_key(&DeviceIdentifier::from("Device1")));

        // Device is lost after 3 cycles
        let empty: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        process_device_changes(&empty, &mut tracked); // unseen 1
        process_device_changes(&empty, &mut tracked); // unseen 2
        let (_, lost) = process_device_changes(&empty, &mut tracked); // unseen 3, lost
        assert_eq!(lost, vec![DeviceIdentifier::from("Device1")]);
        assert!(!tracked.contains_key(&DeviceIdentifier::from("Device1")));

        // Device reappears - should be found as new
        let (found, _) = process_device_changes(&current, &mut tracked);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device1"));
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        );
    }

    // Edge cases
    #[test]
    fn test_empty_to_empty() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert!(tracked.is_empty());
    }

    #[test]
    fn test_device_with_special_characters() {
        let current = devices(&["Device-1_test", "Device 2", "Device\t3"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 3);
        assert!(tracked.contains_key(&DeviceIdentifier::from("Device-1_test")));
        assert!(tracked.contains_key(&DeviceIdentifier::from("Device 2")));
        assert!(tracked.contains_key(&DeviceIdentifier::from("Device\t3")));
    }

    #[test]
    fn test_multiple_devices_lost_simultaneously() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );
        tracked.insert(
            tracked_device("Device2", 2).0,
            tracked_device("Device2", 2).1,
        );
        tracked.insert(
            tracked_device("Device3", 2).0,
            tracked_device("Device3", 2).1,
        );

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert_eq!(lost.len(), 3);
        assert!(lost.contains(&DeviceIdentifier::from("Device1")));
        assert!(lost.contains(&DeviceIdentifier::from("Device2")));
        assert!(lost.contains(&DeviceIdentifier::from("Device3")));
        assert!(tracked.is_empty());
    }

    // Test MSH_SERVICE constant
    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_msh_service_uuid() {
        assert_eq!(
            MESHTASTIC_SERVICE_UUID,
            Uuid::from_u128(0x6ba1b218_15a8_461f_9fa8_5dcae273eafd)
        );
    }

    #[cfg(feature = "meshcore")]
    #[test]
    fn test_meshcore_service_uuid() {
        assert_eq!(
            MESHCORE_SERVICE_UUID,
            Uuid::from_u128(0x6e400001_b5a3_f393_e0a9_e50e24dcca9e)
        );
    }

    // Test detect_radio_type function
    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_detect_radio_type_meshtastic() {
        let services = vec![MESHTASTIC_SERVICE_UUID];
        assert_eq!(detect_radio_type(&services), Some(RadioType::Meshtastic));
    }

    #[cfg(feature = "meshcore")]
    #[test]
    fn test_detect_radio_type_meshcore() {
        let services = vec![MESHCORE_SERVICE_UUID];
        assert_eq!(detect_radio_type(&services), Some(RadioType::MeshCore));
    }

    #[test]
    fn test_detect_radio_type_empty() {
        let services: Vec<Uuid> = vec![];
        // Should return default
        assert_eq!(detect_radio_type(&services), None);
    }

    #[test]
    fn test_detect_radio_type_unknown_uuid() {
        let services = vec![Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc)];
        // Should return default
        assert_eq!(detect_radio_type(&services), None);
    }

    // Async tests for announce_device_changes
    use futures_channel::mpsc;

    // Mock peripheral for testing - we can't easily implement Peripheral trait,
    // but we can test with empty vectors
    #[tokio::test]
    async fn test_announce_device_changes_empty_peripherals_empty_tracked() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        // Close the sender to signal no more messages
        sender.close_channel();

        // No events should be sent
        assert!(
            receiver.try_recv().is_err(),
            "Expected no events but received one"
        );
        assert!(tracked.is_empty());
    }

    #[tokio::test]
    async fn test_announce_device_changes_empty_peripherals_with_tracked_unseen_once() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 0).0,
            tracked_device("Device1", 0).1,
        );
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // No events should be sent (the device has only been unseen once)
        assert!(
            receiver.try_recv().is_err(),
            "Expected no events but received one"
        );
        // Device should still be tracked with incremented count
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(1)
        );
    }

    #[tokio::test]
    async fn test_announce_device_changes_empty_peripherals_device_lost() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // Should receive MeshRadioLost event
        let event = receiver.try_recv().expect("Expected MeshRadioLost event");
        assert!(
            matches!(event, MeshRadioLost(device_id) if device_id == DeviceIdentifier::from("Device1"))
        );

        // No more events
        assert!(
            receiver.try_recv().is_err(),
            "Expected no more events but received one"
        );

        // Device should be removed from tracking
        assert!(!tracked.contains_key(&DeviceIdentifier::from("Device1")));
    }

    #[tokio::test]
    async fn test_announce_device_changes_multiple_devices_lost() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 2).0,
            tracked_device("Device1", 2).1,
        );
        tracked.insert(
            tracked_device("Device2", 2).0,
            tracked_device("Device2", 2).1,
        );
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // Should receive 2 MeshRadioLost events
        let mut lost_devices = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            if let MeshRadioLost(name) = event {
                lost_devices.push(name);
            }
        }

        assert_eq!(lost_devices.len(), 2);
        assert!(lost_devices.contains(&DeviceIdentifier::from("Device1")));
        assert!(lost_devices.contains(&DeviceIdentifier::from("Device2")));
        assert!(tracked.is_empty());
    }

    #[tokio::test]
    async fn test_announce_device_changes_mixed_tracked_states() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            tracked_device("Device1", 0).0,
            tracked_device("Device1", 0).1,
        );
        tracked.insert(
            tracked_device("Device2", 1).0,
            tracked_device("Device2", 1).1,
        );
        tracked.insert(
            tracked_device("Device3", 2).0,
            tracked_device("Device3", 2).1,
        );
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // Should receive only 1 MeshRadioLost event for Device3
        let event = receiver.try_recv().expect("Expected MeshRadioLost event");
        assert!(matches!(event, MeshRadioLost(name) if name == DeviceIdentifier::from("Device3")));

        // No more events
        assert!(
            receiver.try_recv().is_err(),
            "Expected no more events but received one"
        );

        // Device1 and Device2 should still be tracked
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(1)
        );
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device2"))
                .map(|(c, _)| *c),
            Some(2)
        );
        assert!(!tracked.contains_key(&DeviceIdentifier::from("Device3")));
    }

    // Tests for radio type handling in process_device_changes

    #[cfg(feature = "meshtastic")]
    fn devices_meshtastic(items: &[&str]) -> HashMap<DeviceIdentifier, RadioType> {
        items
            .iter()
            .map(|s| (DeviceIdentifier::from(*s), RadioType::Meshtastic))
            .collect()
    }

    #[cfg(feature = "meshcore")]
    fn devices_meshcore(items: &[&str]) -> HashMap<DeviceIdentifier, RadioType> {
        items
            .iter()
            .map(|s| (DeviceIdentifier::from(*s), RadioType::MeshCore))
            .collect()
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_process_device_changes_preserves_meshtastic_radio_type() {
        let current = devices_meshtastic(&["Device1"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device1"));
        assert_eq!(found[0].1, RadioType::Meshtastic);
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(_, rt)| *rt),
            Some(RadioType::Meshtastic)
        );
    }

    #[cfg(feature = "meshcore")]
    #[test]
    fn test_process_device_changes_preserves_meshcore_radio_type() {
        let current = devices_meshcore(&["Device1"]);
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from("Device1"));
        assert_eq!(found[0].1, RadioType::MeshCore);
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(_, rt)| *rt),
            Some(RadioType::MeshCore)
        );
    }

    #[cfg(all(feature = "meshtastic", feature = "meshcore"))]
    #[test]
    fn test_process_device_changes_mixed_radio_types() {
        let mut current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        current.insert(
            DeviceIdentifier::from("MeshtasticDevice"),
            RadioType::Meshtastic,
        );
        current.insert(
            DeviceIdentifier::from("MeshCoreDevice"),
            RadioType::MeshCore,
        );

        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 2);
        let meshtastic_found = found
            .iter()
            .find(|(name, _)| name == &DeviceIdentifier::from("MeshtasticDevice"))
            .expect("MeshtasticDevice should be found");
        let meshcore_found = found
            .iter()
            .find(|(name, _)| name == &DeviceIdentifier::from("MeshCoreDevice"))
            .expect("MeshCoreDevice should be found");

        assert_eq!(meshtastic_found.1, RadioType::Meshtastic);
        assert_eq!(meshcore_found.1, RadioType::MeshCore);
    }

    // Test detect_radio_type with multiple service UUIDs
    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_detect_radio_type_meshtastic_with_other_uuids() {
        let other_uuid = Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc);
        let services = vec![other_uuid, MESHTASTIC_SERVICE_UUID];
        assert_eq!(detect_radio_type(&services), Some(RadioType::Meshtastic));
    }

    #[cfg(feature = "meshcore")]
    #[test]
    fn test_detect_radio_type_meshcore_with_other_uuids() {
        let other_uuid = Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc);
        let services = vec![other_uuid, MESHCORE_SERVICE_UUID];
        assert_eq!(detect_radio_type(&services), Some(RadioType::MeshCore));
    }

    #[cfg(all(feature = "meshtastic", feature = "meshcore"))]
    #[test]
    fn test_detect_radio_type_both_services_meshtastic_first() {
        // When both services are present, meshtastic takes priority (checked first)
        let services = vec![MESHTASTIC_SERVICE_UUID, MESHCORE_SERVICE_UUID];
        assert_eq!(detect_radio_type(&services), Some(RadioType::Meshtastic));
    }

    #[cfg(all(feature = "meshtastic", feature = "meshcore"))]
    #[test]
    fn test_detect_radio_type_both_services_meshcore_first() {
        // Even when meshcore UUID is first, meshtastic still wins (due to check order)
        let services = vec![MESHCORE_SERVICE_UUID, MESHTASTIC_SERVICE_UUID];
        assert_eq!(detect_radio_type(&services), Some(RadioType::Meshtastic));
    }

    #[test]
    fn test_detect_radio_type_many_unknown_uuids() {
        let services: Vec<Uuid> = (0..10)
            .map(|i| Uuid::from_u128(0x10000000_0000_0000_0000_000000000000 + i))
            .collect();
        assert_eq!(detect_radio_type(&services), None);
    }

    #[cfg(feature = "meshtastic")]
    // Test helper functions
    #[test]
    fn test_devices_helper_creates_correct_map() {
        let result = devices(&["A", "B", "C"]);
        assert_eq!(result.len(), 3);
        assert!(result.contains_key(&DeviceIdentifier::from("A")));
        assert!(result.contains_key(&DeviceIdentifier::from("B")));
        assert!(result.contains_key(&DeviceIdentifier::from("C")));
        assert_eq!(
            result.get(&DeviceIdentifier::from("A")),
            Some(&RadioType::Meshtastic)
        );
    }

    #[test]
    fn test_devices_helper_empty() {
        let result = devices(&[]);
        assert!(result.is_empty());
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_tracked_device_helper() {
        let (name, (count, radio_type)) = tracked_device("TestDevice", 5);
        assert_eq!(name, DeviceIdentifier::from("TestDevice"));
        assert_eq!(count, 5);
        assert_eq!(radio_type, RadioType::Meshtastic);
    }

    // Test boundary conditions for unseen count
    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_unseen_count_boundary_at_2() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            DeviceIdentifier::from("Device1"),
            (2, RadioType::Meshtastic),
        );

        let (_, lost) = process_device_changes(&current, &mut tracked);

        // At count 2, after increment it becomes 3, which triggers loss
        assert_eq!(lost.len(), 1);
        assert!(lost.contains(&DeviceIdentifier::from("Device1")));
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_unseen_count_boundary_at_1() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            DeviceIdentifier::from("Device1"),
            (1, RadioType::Meshtastic),
        );

        let (_, lost) = process_device_changes(&current, &mut tracked);

        // At count 1, after increment it becomes 2, not yet lost
        assert!(lost.is_empty());
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(2)
        );
    }

    #[cfg(feature = "meshtastic")]
    // Test that lost event includes correct device names
    #[test]
    fn test_lost_returns_correct_device_names() {
        let current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();
        tracked.insert(
            DeviceIdentifier::from("UniqueDeviceName123"),
            (2, RadioType::Meshtastic),
        );

        let (_, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(lost, vec![DeviceIdentifier::from("UniqueDeviceName123")]);
    }

    #[cfg(feature = "meshtastic")]
    // Test Unicode device names
    #[test]
    fn test_device_with_unicode_name() {
        let mut current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        current.insert(
            DeviceIdentifier::from("日本語デバイス"),
            RadioType::Meshtastic,
        );
        current.insert(DeviceIdentifier::from("Устройство"), RadioType::Meshtastic);
        current.insert(DeviceIdentifier::from("📱Device"), RadioType::Meshtastic);

        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 3);
        assert!(tracked.contains_key(&DeviceIdentifier::from("日本語デバイス")));
        assert!(tracked.contains_key(&DeviceIdentifier::from("Устройство")));
        assert!(tracked.contains_key(&DeviceIdentifier::from("📱Device")));
    }

    #[cfg(feature = "meshtastic")]
    // Test device with an empty name
    #[test]
    fn test_device_with_empty_name() {
        let mut current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        current.insert(DeviceIdentifier::from(""), RadioType::Meshtastic);

        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DeviceIdentifier::from(""));
        assert!(tracked.contains_key(&DeviceIdentifier::from("")));
    }

    #[cfg(feature = "meshtastic")]
    // Test very long device name
    #[test]
    fn test_device_with_long_name() {
        let long_name = DeviceIdentifier::from("A".repeat(1000));
        let mut current: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        current.insert(long_name.clone(), RadioType::Meshtastic);

        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, long_name);
    }

    // Test rapid device cycling
    #[test]
    fn test_rapid_device_cycling() {
        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        // Device appears
        let current = devices(&["Device1"]);
        let (found, _) = process_device_changes(&current, &mut tracked);
        assert_eq!(found.len(), 1);

        // Device disappears
        let empty: HashMap<DeviceIdentifier, RadioType> = HashMap::new();
        process_device_changes(&empty, &mut tracked);
        process_device_changes(&empty, &mut tracked);

        // Device reappears before being lost
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty()); // Not new, was still tracked
        assert!(lost.is_empty()); // Not lost, count reset
        assert_eq!(
            tracked
                .get(&DeviceIdentifier::from("Device1"))
                .map(|(c, _)| *c),
            Some(0)
        );
    }

    #[cfg(feature = "meshtastic")]
    // Test many devices performance
    #[test]
    fn test_many_devices() {
        let device_names: Vec<DeviceIdentifier> = (0..100)
            .map(|i| DeviceIdentifier::from(format!("Device{}", i).as_str()))
            .collect();
        let current: HashMap<DeviceIdentifier, RadioType> = device_names
            .iter()
            .map(|s| (s.clone(), RadioType::Meshtastic))
            .collect();

        let mut tracked: HashMap<DeviceIdentifier, (i32, RadioType)> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 100);
        assert!(lost.is_empty());
        assert_eq!(tracked.len(), 100);
    }
}
