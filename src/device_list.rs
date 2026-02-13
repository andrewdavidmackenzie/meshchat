use crate::Message::{
    AddDeviceAlias, DeviceListViewEvent, DeviceViewEvent, Navigation, RemoveDeviceAlias,
};
use crate::config::Config;
use crate::device::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::device::{ConnectionState, Device};
use crate::device_list::DeviceListEvent::{
    AliasInput, BLEMeshRadioFound, BLERadioLost, CriticalError, Error, Scanning, StartEditingAlias,
};
use crate::styles::{button_chip_style, menu_button_style, text_input_style, tooltip_style};
use crate::widgets::easing;
use crate::widgets::linear::Linear;
use crate::{MeshChat, Message, View};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{
    Column, Container, Id, Row, Space, button, container, image, operation, scrollable, text,
    text_input, tooltip,
};
use iced::{Center, Element, Fill, Renderer, Task, Theme, alignment};
use iced_aw::{Menu, MenuBar, menu_bar, menu_items};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// The type of radio firmware detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RadioType {
    #[default]
    None,
    #[cfg(feature = "meshtastic")]
    Meshtastic,
    #[cfg(feature = "meshcore")]
    MeshCore,
}

#[derive(Debug, Clone)]
pub enum DeviceListEvent {
    BLEMeshRadioFound(String, RadioType),
    BLERadioLost(String),
    CriticalError(String),
    Error(String),
    StartEditingAlias(String),
    AliasInput(String), // From text_input
    Scanning(bool),
}

/// Information about a discovered device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub original_name: String,
    pub radio_type: RadioType,
}

#[derive(Default)]
pub struct DeviceList {
    device_list: HashMap<String, DeviceInfo>, // BLE address -> DeviceInfo
    alias: String,
    editing_alias: Option<String>,
    scanning: bool,
}

async fn empty() {}

const ALIAS_INPUT_TEXT_ID: &str = "alias_input_text";

impl DeviceList {
    pub fn update(&mut self, device_list_event: DeviceListEvent) -> Task<Message> {
        match device_list_event {
            BLEMeshRadioFound(device, radio_type) => {
                if let std::collections::hash_map::Entry::Vacant(vacant_entry) =
                    self.device_list.entry(device.clone())
                {
                    vacant_entry.insert(DeviceInfo {
                        original_name: device,
                        radio_type,
                    });
                }
            }
            BLERadioLost(device) => {
                let _ = self.device_list.remove(&device);
            }
            Error(e) => {
                return Task::perform(empty(), move |_| {
                    Message::AppError(
                        "Discovery Error".to_string(),
                        e.to_string(),
                        MeshChat::now(),
                    )
                });
            }
            CriticalError(e) => {
                return Task::perform(empty(), move |_| {
                    Message::CriticalAppError(
                        "Discovery Error".to_string(),
                        e.to_string(),
                        MeshChat::now(),
                    )
                });
            }
            StartEditingAlias(device) => return self.start_editing_alias(device),
            AliasInput(alias) => self.alias = alias,
            Scanning(scanning) => self.scanning = scanning,
        };

        Task::none()
    }

    /// Called when the user selects to alias a device name
    fn start_editing_alias(&mut self, ble_device: String) -> Task<Message> {
        self.editing_alias = Some(ble_device);
        self.alias = String::new();
        operation::focus(Id::from(ALIAS_INPUT_TEXT_ID))
    }

    /// Called from above when we have finished editing the alias
    pub fn stop_editing_alias(&mut self) {
        self.editing_alias = None;
        self.alias = String::new();
    }

    /// Return the device name or any alias to it that might exist in the config
    pub fn device_name_or_alias<'a>(&'a self, ble_device: &'a str, config: &'a Config) -> String {
        if let Some(alias) = config.device_aliases.get(ble_device) {
            alias.to_string()
        } else {
            ble_device.to_string()
        }
    }

    /// Create a header view for the top of the screen
    pub fn header<'a>(
        &'a self,
        config: &'a Config,
        connection_state: &'a ConnectionState,
    ) -> Element<'a, Message> {
        let mut header_row = Row::new()
            .padding(4)
            .align_y(Center)
            .push(button("Devices").style(button_chip_style));

        header_row = match connection_state {
            Disconnected(_, _) => header_row
                .push(Space::new().width(Fill))
                .push(iced::widget::button("Disconnected").style(button_chip_style)),
            Connecting(device) => {
                let name_button = iced::widget::button(text(format!(
                    "Connecting to {}",
                    self.device_name_or_alias(device, config)
                )))
                .style(button_chip_style);
                header_row.push(Space::new().width(Fill)).push(name_button)
            }
            Connected(mac_address, _) => header_row
                .push(
                    button(text(self.device_name_or_alias(mac_address, config)))
                        .style(button_chip_style)
                        .on_press(Navigation(View::DeviceView(None))),
                )
                .push(Space::new().width(Fill)),
            Disconnecting(device_name) => header_row
                .push(
                    button(text(format!(
                        "📱 {}",
                        self.device_name_or_alias(device_name, config)
                    )))
                    .style(button_chip_style),
                )
                .push(Space::new().width(Fill))
                .push(iced::widget::button("Disconnecting").style(button_chip_style)),
        };

        // Add a disconnect button on the right if we are connected
        if let Connected(_, _) = connection_state {
            header_row = header_row.push(Space::new().width(Fill)).push(
                button("Disconnect")
                    .on_press(DeviceViewEvent(DisconnectRequest(false)))
                    .style(button_chip_style),
            )
        }

        header_row = header_row.push(Device::settings_button());

        // If busy of connecting or disconnecting, add a busy bar to the header
        if self.scanning || matches!(connection_state, Connecting(_) | Disconnecting(_)) {
            Column::new()
                .push(header_row)
                .push(Space::new().width(Fill))
                .push(
                    Linear::new()
                        .easing(easing::emphasized_accelerate())
                        .cycle_duration(Duration::from_secs_f32(2.0))
                        .width(Fill),
                )
                .into()
        } else {
            header_row.into()
        }
    }

    pub fn view<'a>(
        &'a self,
        config: &'a Config,
        connection_state: &'a ConnectionState,
    ) -> Element<'a, Message> {
        if self.device_list.is_empty() {
            return self.empty_view();
        }

        let mut main_col = Column::new();

        for (ble_device, device_info) in &self.device_list {
            let mut device_row = Row::new().align_y(Center).padding(2);

            // Add firmware icon based on a radio type
            let icon_path = match device_info.radio_type {
                RadioType::None => "assets/images/unknown.png",
                #[cfg(feature = "meshtastic")]
                RadioType::Meshtastic => "assets/images/meshtastic.png",
                #[cfg(feature = "meshcore")]
                RadioType::MeshCore => "assets/images/meshcore.png",
            };
            let icon = image(icon_path).width(24).height(24);
            device_row = device_row.push(icon);
            device_row = device_row.push(Space::new().width(8));

            let name_element: Element<'a, Message> =
                if let Some(alias) = config.device_aliases.get(ble_device) {
                    tooltip(
                        text(alias).width(250),
                        text(format!(
                            "Original device name: {}",
                            device_info.original_name
                        )),
                        tooltip::Position::Right,
                    )
                    .style(tooltip_style)
                    .into()
                } else if let Some(editing_device) = &self.editing_alias
                    && editing_device == ble_device
                {
                    text_input("Enter alias for this device", &self.alias)
                        .width(250)
                        .id(Id::from(ALIAS_INPUT_TEXT_ID))
                        .on_input(|s| DeviceListViewEvent(AliasInput(s)))
                        .on_submit(AddDeviceAlias(editing_device.clone(), self.alias.clone()))
                        .style(text_input_style)
                        .into()
                } else {
                    text(&device_info.original_name).width(250).into()
                };

            device_row = device_row.push(name_element);
            device_row = device_row.push(Space::new().width(6));

            device_row = device_row.push(Self::menu_bar(
                ble_device,
                config
                    .device_aliases
                    .contains_key(&device_info.original_name),
            ));

            device_row = device_row.push(Space::new().width(6));
            match &connection_state {
                Connected(connected_device, _) => {
                    if connected_device == ble_device {
                        device_row = device_row.push(
                            button("Disconnect")
                                .on_press(DeviceViewEvent(DisconnectRequest(false)))
                                .style(button_chip_style),
                        );
                    }
                }
                Disconnected(_id, _error) => {
                    device_row = device_row.push(
                        button("Connect")
                            .on_press(DeviceViewEvent(ConnectRequest(
                                ble_device.clone(),
                                device_info.radio_type,
                                None,
                            )))
                            .style(button_chip_style),
                    );
                }
                Connecting(connecting_mac_address) => {
                    if connecting_mac_address == ble_device {
                        device_row = device_row.push(button("Connecting").style(button_chip_style));
                    }
                }
                Disconnecting(disconnecting_mac_address) => {
                    if disconnecting_mac_address == ble_device {
                        device_row =
                            device_row.push(button("Disconnecting").style(button_chip_style));
                    }
                }
            }
            main_col = main_col.push(device_row);
        }

        let scroll = scrollable(main_col)
            .direction({
                let scrollbar = Scrollbar::new().width(10.0);
                scrollable::Direction::Vertical(scrollbar)
            })
            .width(Fill)
            .height(Fill);

        container(scroll)
            .height(Fill)
            .width(Fill)
            .padding(4)
            .align_x(alignment::Horizontal::Left)
            .into()
    }

    fn menu_bar<'a>(ble_device: &str, alias_exists: bool) -> MenuBar<'a, Message, Theme, Renderer> {
        let menu_tpl_1 = |items| Menu::new(items).spacing(3);

        let menu_items = if alias_exists {
            menu_items!(
                (menu_button(
                    "Unalias this device".into(),
                    RemoveDeviceAlias(ble_device.to_string())
                ))
            )
        } else {
            menu_items!(
                (menu_button(
                    "Alias this device".into(),
                    DeviceListViewEvent(StartEditingAlias(ble_device.to_string()))
                ))
            )
        };

        // Create the menu bar with the root button and list of options
        menu_bar!((menu_root_button("▼"), {
            menu_tpl_1(menu_items).width(180)
        }))
        .close_on_background_click(true)
        .close_on_item_click(true)
        .style(menu_button_style)
    }

    /// SHow a message when there are no devices found
    fn empty_view(&self) -> Element<'static, Message> {
        let empty_text = if self.scanning {
            "Searching for compatible Meshtastic radios"
        } else {
            "No compatible Meshtastic radios found."
        };

        Container::new(iced::widget::text(empty_text).size(20))
            .padding(10)
            .width(Fill)
            .align_y(Center)
            .height(Fill)
            .align_x(Center)
            .into()
    }
}

fn menu_button(
    label: String,
    message: Message,
) -> button::Button<'static, Message, Theme, Renderer> {
    button(text(label))
        .padding([4, 8])
        .style(button_chip_style)
        .on_press(message)
        .width(Fill)
}

fn menu_root_button(label: &str) -> button::Button<'_, Message, Theme, Renderer> {
    button(text(label).size(14))
        .padding([0, 4])
        .style(button_chip_style)
        .on_press(Message::None) // Needed for styling to work
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_default() {
        let view = DeviceList::default();
        assert!(view.device_list.is_empty());
        assert!(view.alias.is_empty());
        assert!(view.editing_alias.is_none());
    }

    #[test]
    fn test_ble_radio_found() {
        let mut view = DeviceList::default();
        assert!(view.device_list.is_empty());

        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::None,
        ));

        assert_eq!(view.device_list.len(), 1);
        assert!(view.device_list.contains_key("AA:BB:CC:DD:EE:FF"));
    }

    #[test]
    fn test_ble_radio_found_duplicate() {
        let mut view = DeviceList::default();

        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::None,
        ));
        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::None,
        ));

        // Should still only have 1 entry
        assert_eq!(view.device_list.len(), 1);
    }

    #[test]
    fn test_ble_radio_found_multiple() {
        let mut view = DeviceList::default();

        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::None,
        ));
        let _ = view.update(BLEMeshRadioFound(
            "11:22:33:44:55:66".to_string(),
            RadioType::None,
        ));

        assert_eq!(view.device_list.len(), 2);
    }

    #[test]
    fn test_ble_radio_lost() {
        let mut view = DeviceList::default();

        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::None,
        ));
        assert_eq!(view.device_list.len(), 1);

        let _ = view.update(BLERadioLost("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.device_list.is_empty());
    }

    #[test]
    fn test_ble_radio_lost_nonexistent() {
        let mut view = DeviceList::default();

        // Losing a device that was never found should not panic
        let _ = view.update(BLERadioLost("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.device_list.is_empty());
    }

    #[test]
    fn test_alias_input() {
        let mut view = DeviceList::default();
        assert!(view.alias.is_empty());

        let _ = view.update(AliasInput("My Radio".to_string()));
        assert_eq!(view.alias, "My Radio");
    }

    #[test]
    fn test_start_editing_alias() {
        let mut view = DeviceList {
            alias: "existing".to_string(),
            ..Default::default()
        };

        let _ = view.update(StartEditingAlias("AA:BB:CC:DD:EE:FF".to_string()));

        assert_eq!(view.editing_alias, Some("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.alias.is_empty()); // Should be cleared
    }

    #[test]
    fn test_stop_editing_alias() {
        let mut view = DeviceList {
            editing_alias: Some("AA:BB:CC:DD:EE:FF".to_string()),
            alias: "My Radio".to_string(),
            ..Default::default()
        };

        view.stop_editing_alias();

        assert!(view.editing_alias.is_none());
        assert!(view.alias.is_empty());
    }

    #[test]
    fn test_device_name_or_alias_no_alias() {
        let view = DeviceList::default();
        let config = Config::default();

        let name = view.device_name_or_alias("AA:BB:CC:DD:EE:FF", &config);
        assert_eq!(name, "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn test_device_name_or_alias_with_alias() {
        let view = DeviceList::default();
        let mut config = Config::default();
        config
            .device_aliases
            .insert("AA:BB:CC:DD:EE:FF".to_string(), "My Radio".to_string());

        let name = view.device_name_or_alias("AA:BB:CC:DD:EE:FF", &config);
        assert_eq!(name, "My Radio");
    }

    #[test]
    fn test_device_name_or_alias_different_device() {
        let view = DeviceList::default();
        let mut config = Config::default();
        config
            .device_aliases
            .insert("AA:BB:CC:DD:EE:FF".to_string(), "My Radio".to_string());

        // Different device should return original name
        let name = view.device_name_or_alias("11:22:33:44:55:66", &config);
        assert_eq!(name, "11:22:33:44:55:66");
    }

    #[test]
    fn test_error_returns_task() {
        let mut view = DeviceList::default();

        // Error should return a task (not Task::none)
        let _task = view.update(Error("Test error".to_string()));
        // The task will be an AppError message when executed
    }

    #[test]
    fn test_workflow_find_alias_lose() {
        let mut view = DeviceList::default();
        let mut config = Config::default();

        // Find a device
        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::None,
        ));
        assert_eq!(view.device_list.len(), 1);

        // Start aliasing
        let _ = view.update(StartEditingAlias("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.editing_alias.is_some());

        // Input an alias
        let _ = view.update(AliasInput("My Radio".to_string()));
        assert_eq!(view.alias, "My Radio");

        // Simulate saving (would be done by the parent)
        config
            .device_aliases
            .insert("AA:BB:CC:DD:EE:FF".to_string(), "My Radio".to_string());
        view.stop_editing_alias();

        // Check alias is used
        assert_eq!(
            view.device_name_or_alias("AA:BB:CC:DD:EE:FF", &config),
            "My Radio"
        );

        // Lose the device
        let _ = view.update(BLERadioLost("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.device_list.is_empty());
    }

    #[cfg(feature = "meshtastic")]
    // Test DeviceListEvent enum
    #[test]
    fn test_device_list_event_debug() {
        let event = BLEMeshRadioFound("device1".into(), RadioType::Meshtastic);
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("BLEMeshRadioFound"));
        assert!(debug_str.contains("device1"));
    }

    #[test]
    fn test_device_list_event_clone() {
        let event = BLERadioLost("device1".into());
        let cloned = event.clone();
        assert!(matches!(cloned, BLERadioLost(name) if name == "device1"));
    }

    #[test]
    fn test_device_list_event_error_debug() {
        let event = Error("test error".into());
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("Error"));
        assert!(debug_str.contains("test error"));
    }

    #[test]
    fn test_device_list_event_start_editing_alias_debug() {
        let event = StartEditingAlias("device1".into());
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("StartEditingAlias"));
    }

    #[test]
    fn test_device_list_event_alias_input_debug() {
        let event = AliasInput("my alias".into());
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("AliasInput"));
        assert!(debug_str.contains("my alias"));
    }

    // Additional update tests
    #[test]
    fn test_update_alias_input_empty() {
        let mut view = DeviceList::default();
        view.alias = "existing".into();

        let _ = view.update(AliasInput("".into()));
        assert!(view.alias.is_empty());
    }

    #[test]
    fn test_update_alias_input_special_chars() {
        let mut view = DeviceList::default();

        let _ = view.update(AliasInput("My Radio 🔊 #1".into()));
        assert_eq!(view.alias, "My Radio 🔊 #1");
    }

    #[test]
    fn test_multiple_devices_found_and_lost() {
        let mut view = DeviceList::default();

        // Add multiple devices
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));
        let _ = view.update(BLEMeshRadioFound("device2".into(), RadioType::None));
        let _ = view.update(BLEMeshRadioFound("device3".into(), RadioType::None));
        assert_eq!(view.device_list.len(), 3);

        // Remove one
        let _ = view.update(BLERadioLost("device2".into()));
        assert_eq!(view.device_list.len(), 2);
        assert!(view.device_list.contains_key("device1"));
        assert!(!view.device_list.contains_key("device2"));
        assert!(view.device_list.contains_key("device3"));

        // Remove another
        let _ = view.update(BLERadioLost("device1".into()));
        assert_eq!(view.device_list.len(), 1);

        // Remove last
        let _ = view.update(BLERadioLost("device3".into()));
        assert!(view.device_list.is_empty());
    }

    #[test]
    fn test_start_editing_alias_clears_previous() {
        let mut view = DeviceList::default();

        // Start editing for device1
        let _ = view.update(StartEditingAlias("device1".into()));
        let _ = view.update(AliasInput("alias1".into()));
        assert_eq!(view.editing_alias, Some("device1".into()));
        assert_eq!(view.alias, "alias1");

        // Start editing for device2 - should clear the previous state
        let _ = view.update(StartEditingAlias("device2".into()));
        assert_eq!(view.editing_alias, Some("device2".into()));
        assert!(view.alias.is_empty()); // Should be cleared
    }

    #[test]
    fn test_device_name_or_alias_empty_string() {
        let view = DeviceList::default();
        let config = Config::default();

        let name = view.device_name_or_alias("", &config);
        assert_eq!(name, "");
    }

    #[test]
    fn test_device_name_or_alias_with_multiple_aliases() {
        let view = DeviceList::default();
        let mut config = Config::default();
        config
            .device_aliases
            .insert("device1".into(), "Radio 1".into());
        config
            .device_aliases
            .insert("device2".into(), "Radio 2".into());
        config
            .device_aliases
            .insert("device3".into(), "Radio 3".into());

        assert_eq!(view.device_name_or_alias("device1", &config), "Radio 1");
        assert_eq!(view.device_name_or_alias("device2", &config), "Radio 2");
        assert_eq!(view.device_name_or_alias("device3", &config), "Radio 3");
        assert_eq!(view.device_name_or_alias("device4", &config), "device4");
    }

    #[test]
    fn test_editing_alias_while_device_lost() {
        let mut view = DeviceList::default();

        // Find device
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        // Start editing alias
        let _ = view.update(StartEditingAlias("device1".into()));
        let _ = view.update(AliasInput("My Alias".into()));
        assert_eq!(view.editing_alias, Some("device1".into()));

        // Device is lost while editing
        let _ = view.update(BLERadioLost("device1".into()));
        assert!(view.device_list.is_empty());

        // Editing state is still preserved (caller needs to handle this)
        assert_eq!(view.editing_alias, Some("device1".into()));
        assert_eq!(view.alias, "My Alias");
    }

    #[test]
    fn test_stop_editing_alias_when_not_editing() {
        let mut view = DeviceList::default();
        assert!(view.editing_alias.is_none());

        // Should not panic when not editing
        view.stop_editing_alias();
        assert!(view.editing_alias.is_none());
        assert!(view.alias.is_empty());
    }

    #[test]
    fn test_error_with_empty_message() {
        let mut view = DeviceList::default();
        let _task = view.update(Error("".into()));
        // Should not panic
    }

    #[test]
    fn test_error_with_long_message() {
        let mut view = DeviceList::default();
        let long_error = "A".repeat(1000);
        let _task = view.update(Error(long_error));
        // Should not panic
    }

    #[test]
    fn test_ble_radio_found_with_long_name() {
        let mut view = DeviceList::default();
        let long_name = "B".repeat(100);

        let _ = view.update(BLEMeshRadioFound(long_name.clone(), RadioType::None));
        assert!(view.device_list.contains_key(&long_name));
    }

    #[test]
    fn test_device_list_view_default_values() {
        let view = DeviceList::default();
        assert!(view.device_list.is_empty());
        assert!(view.alias.is_empty());
        assert!(view.editing_alias.is_none());
    }

    #[cfg(feature = "meshcore")]
    #[test]
    fn test_ble_meshcore_radio_found() {
        let mut view = DeviceList::default();
        assert!(view.device_list.is_empty());

        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::MeshCore,
        ));

        assert_eq!(view.device_list.len(), 1);
        assert!(view.device_list.contains_key("AA:BB:CC:DD:EE:FF"));
        assert_eq!(
            view.device_list
                .get("AA:BB:CC:DD:EE:FF")
                .expect("Device should exist after BLEMeshCoreRadioFound event")
                .radio_type,
            RadioType::MeshCore
        );
    }

    #[cfg(feature = "meshcore")]
    #[test]
    fn test_ble_meshcore_radio_found_duplicate() {
        let mut view = DeviceList::default();

        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::MeshCore,
        ));
        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::MeshCore,
        ));

        // Should still only have 1 entry
        assert_eq!(view.device_list.len(), 1);
    }

    #[test]
    fn test_radio_type_default() {
        let radio_type = RadioType::default();
        assert_eq!(radio_type, RadioType::None);
    }

    #[test]
    fn test_radio_type_debug() {
        let radio_type = RadioType::None;
        let debug_str = format!("{:?}", radio_type);
        assert!(debug_str.contains("None"));
    }

    #[test]
    fn test_radio_type_clone() {
        let radio_type = RadioType::None;
        let cloned = radio_type;
        assert_eq!(radio_type, cloned);
    }

    #[test]
    fn test_device_info_debug() {
        let device_info = DeviceInfo {
            original_name: "Test Device".to_string(),
            radio_type: RadioType::None,
        };
        let debug_str = format!("{:?}", device_info);
        assert!(debug_str.contains("DeviceInfo"));
        assert!(debug_str.contains("Test Device"));
    }

    #[test]
    fn test_device_info_clone() {
        let device_info = DeviceInfo {
            original_name: "Test Device".to_string(),
            radio_type: RadioType::None,
        };
        let cloned = device_info.clone();
        assert_eq!(cloned.original_name, "Test Device");
    }

    #[test]
    fn test_device_list_event_ble_radio_lost_clone() {
        let event = BLERadioLost("device1".into());
        let cloned = event.clone();
        assert!(
            matches!(cloned, BLERadioLost(ref name) if name == "device1"),
            "Clone of BLERadioLost should preserve device name, got {:?}",
            cloned
        );
    }

    #[test]
    fn test_device_list_event_error_clone() {
        let event = Error("test error".into());
        let cloned = event.clone();
        assert!(
            matches!(cloned, Error(ref msg) if msg == "test error"),
            "Clone of Error should preserve error message, got {:?}",
            cloned
        );
    }

    #[test]
    fn test_device_list_event_start_editing_alias_clone() {
        let event = StartEditingAlias("device1".into());
        let cloned = event.clone();
        assert!(
            matches!(cloned, StartEditingAlias(ref device) if device == "device1"),
            "Clone of StartEditingAlias should preserve device name, got {:?}",
            cloned
        );
    }

    #[test]
    fn test_device_list_event_alias_input_clone() {
        let event = AliasInput("my alias".into());
        let cloned = event.clone();
        assert!(
            matches!(cloned, AliasInput(ref alias) if alias == "my alias"),
            "Clone of AliasInput should preserve alias value, got {:?}",
            cloned
        );
    }

    #[cfg(feature = "meshtastic")]
    #[test]
    fn test_radio_type_meshtastic() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound(
            "AA:BB:CC:DD:EE:FF".to_string(),
            RadioType::Meshtastic,
        ));

        let device_info = view
            .device_list
            .get("AA:BB:CC:DD:EE:FF")
            .expect("Device should exist after BLEMeshRadioFound event");
        assert_eq!(device_info.radio_type, RadioType::Meshtastic);
    }

    #[test]
    fn test_alias_unicode() {
        let mut view = DeviceList::default();
        let _ = view.update(AliasInput("📱 My Device 日本語".into()));
        assert_eq!(view.alias, "📱 My Device 日本語");
    }

    #[test]
    fn test_device_list_empty_after_all_lost() {
        let mut view = DeviceList::default();

        // Add some devices
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));
        let _ = view.update(BLEMeshRadioFound("device2".into(), RadioType::None));
        assert_eq!(view.device_list.len(), 2);

        // Remove all
        let _ = view.update(BLERadioLost("device1".into()));
        let _ = view.update(BLERadioLost("device2".into()));
        assert!(view.device_list.is_empty());
    }

    // View function tests - verify Element creation without panicking

    #[test]
    fn test_view_empty_device_list() {
        let view = DeviceList::default();
        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Should not panic and return the "Searching" message element
    }

    #[test]
    fn test_view_with_devices_disconnected() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));
        let _ = view.update(BLEMeshRadioFound("device2".into(), RadioType::None));

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_view_with_device_connecting() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();
        let connection_state = Connecting("device1".into());
        let _element = view.view(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_view_with_device_connected() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();
        let connection_state = Connected("device1".into(), RadioType::None);
        let _element = view.view(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_view_with_device_disconnecting() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();
        let connection_state = Disconnecting("device1".into());
        let _element = view.view(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_view_with_alias() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let mut config = Config::default();
        config
            .device_aliases
            .insert("device1".into(), "My Radio".into());

        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Should not panic and show alias
    }

    #[test]
    fn test_view_while_editing_alias() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));
        let _ = view.update(StartEditingAlias("device1".into()));
        let _ = view.update(AliasInput("New Alias".into()));

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Should not panic and show text input
    }

    #[test]
    fn test_header_disconnected() {
        let view = DeviceList::default();
        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.header(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_header_connecting() {
        let view = DeviceList::default();
        let config = Config::default();
        let connection_state = Connecting("device1".into());
        let _element = view.header(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_header_connected() {
        let view = DeviceList::default();
        let config = Config::default();
        let connection_state = Connected("device1".into(), RadioType::None);
        let _element = view.header(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_header_disconnecting() {
        let view = DeviceList::default();
        let config = Config::default();
        let connection_state = Disconnecting("device1".into());
        let _element = view.header(&config, &connection_state);
        // Should not panic
    }

    #[test]
    fn test_header_with_alias() {
        let view = DeviceList::default();
        let mut config = Config::default();
        config
            .device_aliases
            .insert("device1".into(), "My Radio".into());

        let connection_state = Connected("device1".into(), RadioType::None);
        let _element = view.header(&config, &connection_state);
        // Should not panic and use alias in display
    }

    #[test]
    fn test_view_many_devices() {
        let mut view = DeviceList::default();
        for i in 0..20 {
            let _ = view.update(BLEMeshRadioFound(format!("device{}", i), RadioType::None));
        }

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Should not panic with many devices
    }

    #[test]
    fn test_view_with_error_in_connection_state() {
        let view = DeviceList::default();
        let config = Config::default();
        let connection_state =
            Disconnected(Some("device1".into()), Some("Connection failed".into()));
        let _element = view.header(&config, &connection_state);
        // Should not panic
    }

    // Tests for view logic branches - verifying correct behavior for different states

    #[test]
    fn test_view_shows_connect_button_when_disconnected() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();

        // When disconnected, the view should show the Connect button (exercises that branch)
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Exercises the Disconnected match arm in view()
    }

    #[test]
    fn test_view_shows_disconnect_button_when_connected_to_this_device() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();

        // When connected to this specific device, should show Disconnect button
        let connection_state = Connected("device1".into(), RadioType::None);
        let _element = view.view(&config, &connection_state);
        // Exercises the Connected match arm where connected_device == ble_device
    }

    #[test]
    fn test_view_shows_nothing_when_connected_to_different_device() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();

        // When connected to a different device, no special button for this device
        let connection_state = Connected("device2".into(), RadioType::None);
        let _element = view.view(&config, &connection_state);
        // Exercises Connected where connected_device != ble_device
    }

    #[test]
    fn test_view_shows_connecting_when_connecting_to_this_device() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();

        // When connecting to this device, should show "Connecting" indicator
        let connection_state = Connecting("device1".into());
        let _element = view.view(&config, &connection_state);
        // Exercises Connecting where connecting_mac_address == ble_device
    }

    #[test]
    fn test_view_shows_nothing_when_connecting_to_different_device() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();

        // When connecting to a different device
        let connection_state = Connecting("device2".into());
        let _element = view.view(&config, &connection_state);
        // Exercises Connecting where connecting_mac_address != ble_device
    }

    #[test]
    fn test_view_shows_disconnecting_when_disconnecting_this_device() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();

        let connection_state = Disconnecting("device1".into());
        let _element = view.view(&config, &connection_state);
        // Exercises Disconnecting where disconnecting_mac_address == ble_device
    }

    #[test]
    fn test_view_shows_nothing_when_disconnecting_different_device() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));

        let config = Config::default();

        let connection_state = Disconnecting("device2".into());
        let _element = view.view(&config, &connection_state);
        // Exercises Disconnecting where disconnecting_mac_address != ble_device
    }

    #[test]
    fn test_view_alias_vs_original_name() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));
        let _ = view.update(BLEMeshRadioFound("device2".into(), RadioType::None));

        let mut config = Config::default();
        // device1 has an alias, device2 does not
        config
            .device_aliases
            .insert("device1".into(), "My Radio".into());

        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Exercises both the alias branch and the original name branch
    }

    #[test]
    fn test_view_editing_alias_shows_text_input() {
        let mut view = DeviceList::default();
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));
        let _ = view.update(BLEMeshRadioFound("device2".into(), RadioType::None));

        // Start editing alias for device1
        let _ = view.update(StartEditingAlias("device1".into()));
        let _ = view.update(AliasInput("New Name".into()));

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Exercises the editing_alias == Some(device) branch showing text_input
    }

    #[test]
    fn test_menu_bar_with_alias_exists() {
        // When alias exists, the menu should show "Unalias this device"
        let _menu = DeviceList::menu_bar("device1", true);
        // Exercises alias_exists == true branch
    }

    #[test]
    fn test_menu_bar_without_alias() {
        // When no alias, the menu should show "Alias this device"
        let _menu = DeviceList::menu_bar("device1", false);
        // Exercises alias_exists == false branch
    }

    #[test]
    fn test_view_all_radio_types() {
        let mut view = DeviceList::default();

        // Add devices of different types
        #[cfg(feature = "meshtastic")]
        let _ = view.update(BLEMeshRadioFound(
            "meshtastic_device".into(),
            RadioType::None,
        ));

        #[cfg(feature = "meshcore")]
        let _ = view.update(BLEMeshRadioFound("meshcore_device".into(), RadioType::None));

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Exercises the icon_path matching for different radio types
    }

    // Tests for empty_view() - exercised through view() when device_list is empty

    #[test]
    fn test_empty_view_not_scanning() {
        let view = DeviceList::default();
        // scanning is false by default, device_list is empty
        assert!(!view.scanning);
        assert!(view.device_list.is_empty());

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Exercises empty_view() with scanning = false
        // Should show "No compatible Meshtastic radios found."
    }

    #[test]
    fn test_empty_view_while_scanning() {
        let mut view = DeviceList::default();
        // Set scanning to true
        let _ = view.update(Scanning(true));
        assert!(view.scanning);
        assert!(view.device_list.is_empty());

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Exercises empty_view() with scanning = true
        // Should show "Searching for compatible Meshtastic radios"
    }

    #[test]
    fn test_scanning_state_toggle() {
        let mut view = DeviceList::default();
        assert!(!view.scanning);

        // Start scanning
        let _ = view.update(Scanning(true));
        assert!(view.scanning);

        // Stop scanning
        let _ = view.update(Scanning(false));
        assert!(!view.scanning);
    }

    #[test]
    fn test_empty_view_after_all_devices_lost_while_scanning() {
        let mut view = DeviceList::default();

        // Start scanning and find a device
        let _ = view.update(Scanning(true));
        let _ = view.update(BLEMeshRadioFound("device1".into(), RadioType::None));
        assert_eq!(view.device_list.len(), 1);

        // Lose the device while still scanning
        let _ = view.update(BLERadioLost("device1".into()));
        assert!(view.device_list.is_empty());
        assert!(view.scanning);

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Should show the "Searching..." message since scanning is still true
    }

    #[test]
    fn test_empty_view_after_scanning_stops() {
        let mut view = DeviceList::default();

        // Start scanning
        let _ = view.update(Scanning(true));
        assert!(view.scanning);

        // Stop scanning without finding any devices
        let _ = view.update(Scanning(false));
        assert!(!view.scanning);
        assert!(view.device_list.is_empty());

        let config = Config::default();
        let connection_state = Disconnected(None, None);
        let _element = view.view(&config, &connection_state);
        // Should show "No compatible Meshtastic radios found." message
    }
}
