use crate::Message::{
    AddDeviceAlias, DeviceListViewEvent, DeviceViewEvent, Navigation, RemoveDeviceAlias,
};
use crate::config::Config;
use crate::device_list_view::DeviceListEvent::{
    AliasInput, BLERadioFound, BLERadioLost, Error, StartEditingAlias,
};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::device_view::{ConnectionState, DeviceView};
use crate::styles::{button_chip_style, menu_button_style, text_input_style, tooltip_style};
use crate::{MeshChat, Message, View};
use iced::Bottom;
use iced::widget::scrollable::Scrollbar;
use iced::widget::{
    Column, Container, Id, Row, Space, button, container, operation, scrollable, text, text_input,
    tooltip,
};
use iced::{Center, Element, Fill, Renderer, Task, Theme, alignment};
use iced_aw::{Menu, MenuBar, menu_bar, menu_items};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum DeviceListEvent {
    BLERadioFound(String),
    BLERadioLost(String),
    Error(String),
    StartEditingAlias(String),
    AliasInput(String), // From text_input
}

#[derive(Default)]
pub struct DeviceListView {
    device_list: HashMap<String, String>, // Alias/Name, Original Name
    alias: String,
    editing_alias: Option<String>,
}

async fn empty() {}

const ALIAS_INPUT_TEXT_ID: &str = "alias_input_text";

impl DeviceListView {
    pub fn update(&mut self, device_list_event: DeviceListEvent) -> Task<Message> {
        match device_list_event {
            BLERadioFound(device) => {
                if let std::collections::hash_map::Entry::Vacant(vacant_entry) =
                    self.device_list.entry(device.clone())
                {
                    vacant_entry.insert(device);
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
            StartEditingAlias(device) => return self.start_editing_alias(device),
            AliasInput(alias) => self.alias = alias,
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
        state: &'a ConnectionState,
    ) -> Element<'a, Message> {
        let mut header = Row::new()
            .padding(4)
            .align_y(Bottom)
            .push(button("Devices").style(button_chip_style));

        header = header.push(match state {
            Disconnected(_, _) => Row::new()
                .push(Space::new().width(Fill))
                .push(iced::widget::button("Disconnected").style(button_chip_style)),
            Connecting(device) => Row::new().push(Space::new().width(Fill)).push(
                iced::widget::button(text(format!(
                    "Connecting to {}",
                    self.device_name_or_alias(device, config)
                )))
                .style(button_chip_style),
            ),
            Connected(mac_address) => Row::new().push(
                button(text(self.device_name_or_alias(mac_address, config)))
                    .style(button_chip_style)
                    .on_press(Navigation(View::Device(None))),
            ),
            Disconnecting(mac_address) => Row::new().push(
                text(format!(
                    "Disconnecting from {}",
                    self.device_name_or_alias(mac_address, config)
                ))
                .width(Fill)
                .align_x(alignment::Horizontal::Right),
            ),
        });

        // Add a disconnect button on the right if we are connected
        if let Connected(_) = state {
            header = header.push(Space::new().width(Fill)).push(
                button("Disconnect")
                    .on_press(DeviceViewEvent(DisconnectRequest(false)))
                    .style(button_chip_style),
            )
        }

        header.push(DeviceView::settings_button()).into()
    }

    pub fn view<'a>(
        &'a self,
        config: &'a Config,
        connection_state: &'a ConnectionState,
    ) -> Element<'a, Message> {
        if self.device_list.is_empty() {
            return empty_view();
        }

        let mut main_col = Column::new();

        for (ble_device, device_name) in &self.device_list {
            let mut device_row = Row::new().align_y(Center).padding(2);
            let name_element: Element<'a, Message> =
                if let Some(alias) = config.device_aliases.get(ble_device) {
                    tooltip(
                        text(alias).width(250),
                        text(format!("Original device name: {}", device_name)),
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
                    text(device_name).width(250).into()
                };

            device_row = device_row.push(name_element);
            device_row = device_row.push(Space::new().width(6));

            device_row = device_row.push(Self::menu_bar(
                ble_device,
                config.device_aliases.contains_key(device_name),
            ));

            device_row = device_row.push(Space::new().width(6));
            match &connection_state {
                Connected(connected_device) => {
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
                            .on_press(DeviceViewEvent(ConnectRequest(ble_device.clone(), None)))
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

/// SHow a message when there are no devices found
fn empty_view() -> Element<'static, Message> {
    Container::new(text("Searching for compatible Meshtastic radios").size(20))
        .padding(10)
        .width(Fill)
        .align_y(Center)
        .height(Fill)
        .align_x(Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_default() {
        let view = DeviceListView::default();
        assert!(view.device_list.is_empty());
        assert!(view.alias.is_empty());
        assert!(view.editing_alias.is_none());
    }

    #[test]
    fn test_ble_radio_found() {
        let mut view = DeviceListView::default();
        assert!(view.device_list.is_empty());

        let _ = view.update(BLERadioFound("AA:BB:CC:DD:EE:FF".to_string()));

        assert_eq!(view.device_list.len(), 1);
        assert!(view.device_list.contains_key("AA:BB:CC:DD:EE:FF"));
    }

    #[test]
    fn test_ble_radio_found_duplicate() {
        let mut view = DeviceListView::default();

        let _ = view.update(BLERadioFound("AA:BB:CC:DD:EE:FF".to_string()));
        let _ = view.update(BLERadioFound("AA:BB:CC:DD:EE:FF".to_string()));

        // Should still only have 1 entry
        assert_eq!(view.device_list.len(), 1);
    }

    #[test]
    fn test_ble_radio_found_multiple() {
        let mut view = DeviceListView::default();

        let _ = view.update(BLERadioFound("AA:BB:CC:DD:EE:FF".to_string()));
        let _ = view.update(BLERadioFound("11:22:33:44:55:66".to_string()));

        assert_eq!(view.device_list.len(), 2);
    }

    #[test]
    fn test_ble_radio_lost() {
        let mut view = DeviceListView::default();

        let _ = view.update(BLERadioFound("AA:BB:CC:DD:EE:FF".to_string()));
        assert_eq!(view.device_list.len(), 1);

        let _ = view.update(BLERadioLost("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.device_list.is_empty());
    }

    #[test]
    fn test_ble_radio_lost_nonexistent() {
        let mut view = DeviceListView::default();

        // Losing a device that was never found should not panic
        let _ = view.update(BLERadioLost("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.device_list.is_empty());
    }

    #[test]
    fn test_alias_input() {
        let mut view = DeviceListView::default();
        assert!(view.alias.is_empty());

        let _ = view.update(AliasInput("My Radio".to_string()));
        assert_eq!(view.alias, "My Radio");
    }

    #[test]
    fn test_start_editing_alias() {
        let mut view = DeviceListView {
            alias: "existing".to_string(),
            ..Default::default()
        };

        let _ = view.update(StartEditingAlias("AA:BB:CC:DD:EE:FF".to_string()));

        assert_eq!(view.editing_alias, Some("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.alias.is_empty()); // Should be cleared
    }

    #[test]
    fn test_stop_editing_alias() {
        let mut view = DeviceListView {
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
        let view = DeviceListView::default();
        let config = Config::default();

        let name = view.device_name_or_alias("AA:BB:CC:DD:EE:FF", &config);
        assert_eq!(name, "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn test_device_name_or_alias_with_alias() {
        let view = DeviceListView::default();
        let mut config = Config::default();
        config
            .device_aliases
            .insert("AA:BB:CC:DD:EE:FF".to_string(), "My Radio".to_string());

        let name = view.device_name_or_alias("AA:BB:CC:DD:EE:FF", &config);
        assert_eq!(name, "My Radio");
    }

    #[test]
    fn test_device_name_or_alias_different_device() {
        let view = DeviceListView::default();
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
        let mut view = DeviceListView::default();

        // Error should return a task (not Task::none)
        let _task = view.update(Error("Test error".to_string()));
        // The task will be an AppError message when executed
    }

    #[test]
    fn test_workflow_find_alias_lose() {
        let mut view = DeviceListView::default();
        let mut config = Config::default();

        // Find a device
        let _ = view.update(BLERadioFound("AA:BB:CC:DD:EE:FF".to_string()));
        assert_eq!(view.device_list.len(), 1);

        // Start aliasing
        let _ = view.update(StartEditingAlias("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(view.editing_alias.is_some());

        // Input an alias
        let _ = view.update(AliasInput("My Radio".to_string()));
        assert_eq!(view.alias, "My Radio");

        // Simulate saving (would be done by parent)
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
}
