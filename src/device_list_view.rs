use crate::Message::{
    AddDeviceAlias, DeviceListViewEvent, DeviceViewEvent, Navigation, RemoveDeviceAlias,
};
use crate::config::Config;
use crate::device_list_view::DeviceListEvent::{
    BLERadioFound, BLERadioLost, Error, StartEditingAlias,
};
use crate::device_view::ConnectionState;
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::styles::{button_chip_style, menu_button_style, text_input_style, tooltip_style};
use crate::{Message, View};
use btleplug::api::BDAddr;
use futures_channel::mpsc::Sender;
use iced::advanced::text::Shaping::Advanced;
use iced::futures::{SinkExt, Stream};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{
    Column, Container, Row, Space, button, container, scrollable, text, text_input, tooltip,
};
use iced::{Bottom, stream};
use iced::{Center, Element, Fill, Renderer, Task, Theme, alignment};
use iced_aw::{Menu, MenuBar, menu_bar, menu_items};
use meshtastic::utils::stream::{BleDevice, available_ble_devices};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum DeviceListEvent {
    BLERadioFound(BleDevice),
    BLERadioLost(BleDevice),
    Error(String),
    StartEditingAlias(BDAddr),
    AliasInput(String), // From text_input
}

#[derive(Default)]
pub struct DeviceListView {
    device_list: HashMap<BDAddr, Option<String>>,
    alias: String,
    editing_alias: Option<BDAddr>,
}

async fn empty() {}

impl DeviceListView {
    pub fn update(&mut self, device_list_event: DeviceListEvent) -> Task<Message> {
        match device_list_event {
            BLERadioFound(device) => {
                if let std::collections::hash_map::Entry::Vacant(e) =
                    self.device_list.entry(device.mac_address)
                {
                    e.insert(device.name.clone());
                }
            }
            BLERadioLost(device) => {
                let _ = self.device_list.remove(&device.mac_address);
            }
            Error(e) => {
                return Task::perform(empty(), move |_| {
                    Message::AppError("Discovery Error".to_string(), e.to_string())
                });
            }
            StartEditingAlias(device) => self.start_editing_alias(device),
            DeviceListEvent::AliasInput(alias) => self.alias = alias,
        };

        Task::none()
    }

    /// Called when the user selects to alias a device name
    fn start_editing_alias(&mut self, mac_address: BDAddr) {
        self.editing_alias = Some(mac_address);
        self.alias = String::new();
    }

    /// Called from above when we have finished editing the alias
    pub fn stop_editing_alias(&mut self) {
        self.editing_alias = None;
        self.alias = String::new();
    }

    /// Return the device name or any alias to it that might exist in the config
    pub fn device_name_or_alias<'a>(
        &'a self,
        mac_address: &'a BDAddr,
        config: &'a Config,
    ) -> &'a str {
        if let Some(alias) = config.device_aliases.get(mac_address) {
            alias
        } else {
            self.device_list.get(mac_address).unwrap().as_ref().unwrap()
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
        if let Connected(mac_address) = state {
            header = header.push(Space::new().width(Fill)).push(
                button("Disconnect")
                    .on_press(DeviceViewEvent(DisconnectRequest(*mac_address, false)))
                    .style(button_chip_style),
            )
        }

        header.into()
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

        for (mac_address, device_name) in &self.device_list {
            let mut device_row = Row::new().align_y(Center).padding(2);

            let name_element: Element<'a, Message> =
                if let Some(alias) = config.device_aliases.get(mac_address) {
                    tooltip(
                        text(alias).shaping(Advanced).width(250),
                        text(format!(
                            "Original device name: {}",
                            device_name.as_ref().unwrap()
                        ))
                        .shaping(Advanced),
                        tooltip::Position::Right,
                    )
                    .style(tooltip_style)
                    .into()
                } else if let Some(editing_mac) = &self.editing_alias
                    && editing_mac == mac_address
                {
                    text_input("Enter alias for this device", &self.alias)
                        .width(250)
                        .on_input(|s| DeviceListViewEvent(DeviceListEvent::AliasInput(s)))
                        .on_submit(AddDeviceAlias(*editing_mac, self.alias.clone()))
                        .style(text_input_style)
                        .into()
                } else {
                    text(device_name.as_ref().unwrap()).shaping(Advanced).into()
                };

            device_row = device_row.push(name_element);
            device_row = device_row.push(Space::new().width(6));

            device_row = device_row.push(Self::menu_bar(
                mac_address,
                config.device_aliases.contains_key(mac_address),
            ));

            device_row = device_row.push(Space::new().width(6));
            match &connection_state {
                Connected(connected_mac_address) => {
                    device_row = device_row.push(
                        button("Disconnect")
                            .on_press(DeviceViewEvent(DisconnectRequest(
                                *connected_mac_address,
                                false,
                            )))
                            .style(button_chip_style),
                    );
                }
                Disconnected(_id, _error) => {
                    device_row = device_row.push(
                        button("Connect")
                            .on_press(DeviceViewEvent(ConnectRequest(*mac_address, None)))
                            .style(button_chip_style),
                    );
                }
                Connecting(connecting_mac_address) => {
                    if connecting_mac_address == mac_address {
                        device_row = device_row.push(button("Connecting").style(button_chip_style));
                    }
                }
                Disconnecting(disconnecting_mac_address) => {
                    if disconnecting_mac_address == mac_address {
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

    fn menu_bar<'a>(
        mac_address: &BDAddr,
        alias_exists: bool,
    ) -> MenuBar<'a, Message, Theme, Renderer> {
        let menu_tpl_1 = |items| Menu::new(items).spacing(3);

        let menu_items = if alias_exists {
            menu_items!(
                (menu_button(
                    "Unalias this device".into(),
                    RemoveDeviceAlias(*mac_address)
                ))
            )
        } else {
            menu_items!(
                (menu_button(
                    "Alias this device".into(),
                    DeviceListViewEvent(StartEditingAlias(*mac_address))
                ))
            )
        };

        // Create the menu bar with the root button and list of options
        menu_bar!((menu_root_button("▼"), {
            menu_tpl_1(menu_items).width(140)
        }))
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

/// A stream of [DeviceListEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DeviceListEvent> {
    stream::channel(
        100,
        move |mut gui_sender: Sender<DeviceListEvent>| async move {
            let mut mesh_radio_ids: Vec<BleDevice> = vec![];

            // loop scanning for devices
            loop {
                match available_ble_devices(Duration::from_secs(4)).await {
                    Ok(radios_now_ids) => {
                        // detect lost radios
                        for id in &mesh_radio_ids {
                            if !radios_now_ids.iter().any(|other_id| id == other_id) {
                                // inform GUI of a device lost
                                gui_sender
                                    .send(BLERadioLost(id.clone()))
                                    .await
                                    .unwrap_or_else(|e| eprintln!("Discovery gui send error: {e}"));
                            }
                        }

                        // detect new radios found
                        for id in &radios_now_ids {
                            if !mesh_radio_ids.iter().any(|other_id| id == other_id) {
                                // track it for the future
                                mesh_radio_ids.push(id.clone());

                                // inform GUI of a new device found
                                gui_sender
                                    .send(BLERadioFound(id.clone()))
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
