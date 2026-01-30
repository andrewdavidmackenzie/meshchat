use crate::device_list_view::DeviceListEvent;
use crate::device_list_view::DeviceListEvent::{BLERadioFound, BLERadioLost, Error};
use btleplug::api::{Central, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager};
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
                        Ok(adapters) => match adapters.into_iter().next() {
                            Some(adapter) => {
                                // start scanning for MeshTastic radios
                                match adapter
                                    .start_scan(ScanFilter {
                                        services: vec![MSH_SERVICE],
                                    })
                                    .await
                                {
                                    Ok(()) => {
                                        scan_for_devices(
                                            &mut gui_sender,
                                            &adapter,
                                            &mut mesh_radio_devices,
                                        )
                                        .await
                                    }
                                    Err(e) => {
                                        gui_sender.send(Error(e.to_string())).await.unwrap_or_else(
                                            |e| eprintln!("Discovery gui send error: {e}"),
                                        );
                                    }
                                }
                            }
                            None => {
                                gui_sender
                                    .send(Error("Discovery could not get a BT Adapter".into()))
                                    .await
                                    .unwrap_or_else(|e| {
                                        eprintln!("Discovery could not find a BT adapters: {e}")
                                    });
                            }
                        },
                        Err(e) => {
                            gui_sender
                                .send(Error(e.to_string()))
                                .await
                                .unwrap_or_else(|e| {
                                    eprintln!("Discovery could not get first BT adapter: {e}")
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

async fn scan_for_devices(
    gui_sender: &mut Sender<DeviceListEvent>,
    adapter: &Adapter,
    mesh_radio_devices: &mut HashMap<String, i32>,
) {
    // loop scanning for devices
    loop {
        match adapter.peripherals().await {
            Ok(peripherals) => {
                announce_device_changes(gui_sender, &peripherals, mesh_radio_devices).await;
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

/// Process device changes and return events to send.
/// Returns (devices_found, devices_lost)
fn process_device_changes(
    current_devices: &HashSet<String>,
    tracked_devices: &mut HashMap<String, i32>,
) -> (Vec<String>, Vec<String>) {
    let mut found = Vec::new();
    let mut lost = Vec::new();

    // detect lost radios
    for (device_name, unseen_count) in tracked_devices.iter_mut() {
        if !current_devices.contains(device_name) {
            *unseen_count += 1;
            println!("'{}' Unseen once", device_name);
        } else {
            // Reset count if the device is seen again
            *unseen_count = 0;
        }

        // if unseen 3 times, then notify
        if *unseen_count >= 3 {
            println!("'{}' Unseen 3 times", device_name);
            lost.push(device_name.clone());
        }
    }

    // Clean up the list of devices, removing ones not seen for 3 cycles
    tracked_devices.retain(|_device, unseen_count| *unseen_count < 3);

    // detect new radios found
    for device in current_devices {
        if !tracked_devices.contains_key(device) {
            // track it for the future - starting with an unseen count of 0
            tracked_devices.insert(device.clone(), 0);
            found.push(device.clone());
        }
    }

    (found, lost)
}

async fn announce_device_changes(
    gui_sender: &mut Sender<DeviceListEvent>,
    peripherals: &Vec<impl Peripheral>,
    mesh_radio_devices: &mut HashMap<String, i32>,
) {
    let mut ble_devices_now: HashSet<String> = HashSet::new();

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

    let (found, lost) = process_device_changes(&ble_devices_now, mesh_radio_devices);

    // Send lost events
    for device in lost {
        gui_sender
            .send(BLERadioLost(device))
            .await
            .unwrap_or_else(|e| eprintln!("Discovery could not send BLERadioLost: {e}"));
    }

    // Send found events
    for device in found {
        gui_sender
            .send(BLERadioFound(device))
            .await
            .unwrap_or_else(|e| eprintln!("Discovery could not send BLERadioFound: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hashset(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // Test discovering new devices
    #[test]
    fn test_new_device_found() {
        let current = hashset(&["Device1"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found, vec!["Device1".to_string()]);
        assert!(lost.is_empty());
        assert_eq!(tracked.get("Device1"), Some(&0));
    }

    #[test]
    fn test_multiple_new_devices_found() {
        let current = hashset(&["Device1", "Device2", "Device3"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 3);
        assert!(found.contains(&"Device1".to_string()));
        assert!(found.contains(&"Device2".to_string()));
        assert!(found.contains(&"Device3".to_string()));
        assert!(lost.is_empty());
        assert_eq!(tracked.len(), 3);
    }

    #[test]
    fn test_no_devices() {
        let current: HashSet<String> = HashSet::new();
        let mut tracked: HashMap<String, i32> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert!(tracked.is_empty());
    }

    // Test device still present
    #[test]
    fn test_device_still_present() {
        let current = hashset(&["Device1"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 0);

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(tracked.get("Device1"), Some(&0));
    }

    #[test]
    fn test_device_reappears_resets_count() {
        let current = hashset(&["Device1"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2); // Was unseen twice

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        // Count should be reset to 0
        assert_eq!(tracked.get("Device1"), Some(&0));
    }

    // Test device disappearing
    #[test]
    fn test_device_unseen_once() {
        let current: HashSet<String> = HashSet::new();
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 0);

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty()); // Not lost yet, only unseen once
        assert_eq!(tracked.get("Device1"), Some(&1));
    }

    #[test]
    fn test_device_unseen_twice() {
        let current: HashSet<String> = HashSet::new();
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 1);

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty()); // Not lost yet, only unseen twice
        assert_eq!(tracked.get("Device1"), Some(&2));
    }

    #[test]
    fn test_device_lost_after_three_unseen() {
        let current: HashSet<String> = HashSet::new();
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2);

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert_eq!(lost, vec!["Device1".to_string()]);
        // Device should be removed from tracking
        assert!(!tracked.contains_key("Device1"));
    }

    #[test]
    fn test_device_removed_from_tracking_after_lost() {
        let current: HashSet<String> = HashSet::new();
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2);

        process_device_changes(&current, &mut tracked);

        assert!(tracked.is_empty());
    }

    // Test mixed scenarios
    #[test]
    fn test_one_found_one_still_present() {
        let current = hashset(&["Device1", "Device2"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 0);

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found, vec!["Device2".to_string()]);
        assert!(lost.is_empty());
        assert_eq!(tracked.len(), 2);
    }

    #[test]
    fn test_one_found_one_disappearing() {
        let current = hashset(&["Device2"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 0);

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found, vec!["Device2".to_string()]);
        assert!(lost.is_empty()); // Device1 not lost yet
        assert_eq!(tracked.get("Device1"), Some(&1));
        assert_eq!(tracked.get("Device2"), Some(&0));
    }

    #[test]
    fn test_one_found_one_lost() {
        let current = hashset(&["Device2"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2); // About to be lost

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found, vec!["Device2".to_string()]);
        assert_eq!(lost, vec!["Device1".to_string()]);
        assert!(!tracked.contains_key("Device1"));
        assert_eq!(tracked.get("Device2"), Some(&0));
    }

    #[test]
    fn test_multiple_devices_different_states() {
        let current = hashset(&["Device1", "Device4"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 1); // Will reappear
        tracked.insert("Device2".to_string(), 0); // Will be unseen once
        tracked.insert("Device3".to_string(), 2); // Will be lost

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert_eq!(found, vec!["Device4".to_string()]);
        assert_eq!(lost, vec!["Device3".to_string()]);
        assert_eq!(tracked.get("Device1"), Some(&0)); // Reset
        assert_eq!(tracked.get("Device2"), Some(&1)); // Incremented
        assert!(!tracked.contains_key("Device3")); // Removed
        assert_eq!(tracked.get("Device4"), Some(&0)); // New
    }

    // Test the full lifecycle
    #[test]
    fn test_device_full_lifecycle() {
        let mut tracked: HashMap<String, i32> = HashMap::new();

        // Cycle 1: Device appears
        let current = hashset(&["Device1"]);
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert_eq!(found, vec!["Device1".to_string()]);
        assert!(lost.is_empty());
        assert_eq!(tracked.get("Device1"), Some(&0));

        // Cycle 2: Device still present
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(tracked.get("Device1"), Some(&0));

        // Cycle 3: Device disappears (unseen 1)
        let current: HashSet<String> = HashSet::new();
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(tracked.get("Device1"), Some(&1));

        // Cycle 4: Device still gone (unseen 2)
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert_eq!(tracked.get("Device1"), Some(&2));

        // Cycle 5: Device still gone (unseen 3 - lost)
        let (found, lost) = process_device_changes(&current, &mut tracked);
        assert!(found.is_empty());
        assert_eq!(lost, vec!["Device1".to_string()]);
        assert!(!tracked.contains_key("Device1"));
    }

    #[test]
    fn test_device_reappears_during_disappearing() {
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2); // About to be lost

        // Device reappears just in time
        let current = hashset(&["Device1"]);
        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty()); // Not new, was tracked
        assert!(lost.is_empty()); // Not lost, reappeared
        assert_eq!(tracked.get("Device1"), Some(&0)); // Count reset
    }

    #[test]
    fn test_lost_device_can_be_found_again() {
        let mut tracked: HashMap<String, i32> = HashMap::new();

        // Device appears
        let current = hashset(&["Device1"]);
        process_device_changes(&current, &mut tracked);
        assert!(tracked.contains_key("Device1"));

        // Device is lost after 3 cycles
        let empty: HashSet<String> = HashSet::new();
        process_device_changes(&empty, &mut tracked); // unseen 1
        process_device_changes(&empty, &mut tracked); // unseen 2
        let (_, lost) = process_device_changes(&empty, &mut tracked); // unseen 3, lost
        assert_eq!(lost, vec!["Device1".to_string()]);
        assert!(!tracked.contains_key("Device1"));

        // Device reappears - should be found as new
        let (found, _) = process_device_changes(&current, &mut tracked);
        assert_eq!(found, vec!["Device1".to_string()]);
        assert_eq!(tracked.get("Device1"), Some(&0));
    }

    // Edge cases
    #[test]
    fn test_empty_to_empty() {
        let current: HashSet<String> = HashSet::new();
        let mut tracked: HashMap<String, i32> = HashMap::new();

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert!(lost.is_empty());
        assert!(tracked.is_empty());
    }

    #[test]
    fn test_device_with_special_characters() {
        let current = hashset(&["Device-1_test", "Device 2", "Device\t3"]);
        let mut tracked: HashMap<String, i32> = HashMap::new();

        let (found, _) = process_device_changes(&current, &mut tracked);

        assert_eq!(found.len(), 3);
        assert!(tracked.contains_key("Device-1_test"));
        assert!(tracked.contains_key("Device 2"));
        assert!(tracked.contains_key("Device\t3"));
    }

    #[test]
    fn test_multiple_devices_lost_simultaneously() {
        let current: HashSet<String> = HashSet::new();
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2);
        tracked.insert("Device2".to_string(), 2);
        tracked.insert("Device3".to_string(), 2);

        let (found, lost) = process_device_changes(&current, &mut tracked);

        assert!(found.is_empty());
        assert_eq!(lost.len(), 3);
        assert!(lost.contains(&"Device1".to_string()));
        assert!(lost.contains(&"Device2".to_string()));
        assert!(lost.contains(&"Device3".to_string()));
        assert!(tracked.is_empty());
    }

    // Test MSH_SERVICE constant
    #[test]
    fn test_msh_service_uuid() {
        assert_eq!(
            MSH_SERVICE,
            Uuid::from_u128(0x6ba1b218_15a8_461f_9fa8_5dcae273eafd)
        );
    }

    // Async tests for announce_device_changes
    use futures_channel::mpsc;

    // Mock peripheral for testing - we can't easily implement Peripheral trait,
    // but we can test with empty vectors
    #[tokio::test]
    async fn test_announce_device_changes_empty_peripherals_empty_tracked() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        // Close the sender to signal no more messages
        sender.close_channel();

        // No events should be sent
        assert!(receiver.try_next().unwrap().is_none());
        assert!(tracked.is_empty());
    }

    #[tokio::test]
    async fn test_announce_device_changes_empty_peripherals_with_tracked_unseen_once() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 0);
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // No events should be sent (the device has only been unseen once)
        assert!(receiver.try_next().unwrap().is_none());
        // Device should still be tracked with incremented count
        assert_eq!(tracked.get("Device1"), Some(&1));
    }

    #[tokio::test]
    async fn test_announce_device_changes_empty_peripherals_device_lost() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2); // Will be lost
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // Should receive BLERadioLost event
        let event = receiver.try_next().unwrap();
        assert!(matches!(event, Some(BLERadioLost(name)) if name == "Device1"));

        // No more events
        assert!(receiver.try_next().unwrap().is_none());

        // Device should be removed from tracking
        assert!(!tracked.contains_key("Device1"));
    }

    #[tokio::test]
    async fn test_announce_device_changes_multiple_devices_lost() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 2);
        tracked.insert("Device2".to_string(), 2);
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // Should receive 2 BLERadioLost events
        let mut lost_devices = Vec::new();
        while let Ok(Some(event)) = receiver.try_next() {
            if let BLERadioLost(name) = event {
                lost_devices.push(name);
            }
        }

        assert_eq!(lost_devices.len(), 2);
        assert!(lost_devices.contains(&"Device1".to_string()));
        assert!(lost_devices.contains(&"Device2".to_string()));
        assert!(tracked.is_empty());
    }

    #[tokio::test]
    async fn test_announce_device_changes_mixed_tracked_states() {
        let (mut sender, mut receiver) = mpsc::channel::<DeviceListEvent>(10);
        let mut tracked: HashMap<String, i32> = HashMap::new();
        tracked.insert("Device1".to_string(), 0); // Will be unseen once
        tracked.insert("Device2".to_string(), 1); // Will be unseen twice
        tracked.insert("Device3".to_string(), 2); // Will be lost
        let peripherals: Vec<btleplug::platform::Peripheral> = vec![];

        announce_device_changes(&mut sender, &peripherals, &mut tracked).await;

        sender.close_channel();

        // Should receive only 1 BLERadioLost event for Device3
        let event = receiver.try_next().unwrap();
        assert!(matches!(event, Some(BLERadioLost(name)) if name == "Device3"));

        // No more events
        assert!(receiver.try_next().unwrap().is_none());

        // Device1 and Device2 should still be tracked
        assert_eq!(tracked.get("Device1"), Some(&1));
        assert_eq!(tracked.get("Device2"), Some(&2));
        assert!(!tracked.contains_key("Device3"));
    }
}
