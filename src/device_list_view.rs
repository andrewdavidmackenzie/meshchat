use crate::Message::{Device, Navigation};
use crate::device_list_view::DiscoveryEvent::{BLERadioFound, BLERadioLost, Error};
use crate::device_view::ConnectionState;
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::styles::button_chip_style;
use crate::{Message, View, name_from_id};
use iced::futures::{SinkExt, Stream};
use iced::stream;
use iced::widget::{Column, Row, Space, button, container, text};
use iced::{Element, Fill, Task, alignment};
use meshtastic::utils::stream::{BleDevice, available_ble_devices};
use std::collections::HashSet;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    BLERadioFound(BleDevice),
    BLERadioLost(BleDevice),
    Error(String),
}

#[derive(Default)]
pub struct DeviceListView {
    discovered_devices: HashSet<BleDevice>,
}

async fn empty() {}

impl DeviceListView {
    pub fn update(&mut self, discovery_event: DiscoveryEvent) -> Task<Message> {
        match discovery_event {
            BLERadioFound(device) => {
                if !self.discovered_devices.contains(&device) {
                    self.discovered_devices.insert(device);
                }
            }
            BLERadioLost(device) => {
                let _ = self.discovered_devices.remove(&device);
            }
            Error(e) => {
                return Task::perform(empty(), move |_| {
                    Message::AppError("Discovery Error".to_string(), e.to_string())
                });
            }
        };

        Task::none()
    }

    pub fn header<'a>(&'a self, connection_state: &'a ConnectionState) -> Element<'a, Message> {
        let mut header = match connection_state {
            Disconnected(_, _) => Row::new()
                .push(Space::with_width(Fill))
                .push(iced::widget::button("Disconnected").style(button_chip_style)),
            Connecting(device) => Row::new().push(Space::with_width(Fill)).push(
                iced::widget::button(text(format!(
                    "Connecting to {}",
                    device.name.as_ref().unwrap()
                )))
                .style(button_chip_style),
            ),
            Connected(device) => Row::new().push(
                button(text(device.name.as_ref().unwrap()))
                    .style(button_chip_style)
                    .on_press(Navigation(View::Device)),
            ),
            Disconnecting(device) => Row::new().push(
                text(format!(
                    "Disconnecting from {}",
                    device.name.as_ref().unwrap()
                ))
                .width(Fill)
                .align_x(alignment::Horizontal::Right),
            ),
        };

        // Add a disconnect button on the right if we are connected
        if let Connected(device) = connection_state {
            header = header.push(Space::new(Fill, 1)).push(
                button("Disconnect")
                    .on_press(Device(DisconnectRequest(device.clone(), false)))
                    .style(button_chip_style),
            )
        }

        header.into()
    }

    pub fn view(&self, connection_state: &ConnectionState) -> Element<'static, Message> {
        let mut main_col = Column::new();
        // TODO add a scrollable area in case there are a lot of devices

        for device in &self.discovered_devices {
            let mut device_row = Row::new().align_y(alignment::Vertical::Center);
            device_row = device_row.push(text(name_from_id(device)));
            device_row = device_row.push(Space::new(6, 0));
            match &connection_state {
                Connected(connected_device) => {
                    device_row = device_row.push(
                        button("Disconnect")
                            .on_press(Device(DisconnectRequest(connected_device.clone(), false)))
                            .style(button_chip_style),
                    );
                }
                Disconnected(_id, _error) => {
                    device_row = device_row.push(
                        button("Connect")
                            .on_press(Device(ConnectRequest(device.clone(), None)))
                            .style(button_chip_style),
                    );
                }
                Connecting(connecting_device) => {
                    if connecting_device == device {
                        device_row = device_row.push(text("Connecting"));
                    }
                }
                Disconnecting(disconnecting_device) => {
                    if disconnecting_device == device {
                        device_row = device_row.push(text("Disconnecting"));
                    }
                }
            }
            main_col = main_col.push(device_row);
        }

        container(main_col)
            .height(Fill)
            .width(Fill)
            .align_x(alignment::Horizontal::Left)
            .into()
    }
}
/// A stream of [DiscoveryEvent] announcing the discovery or loss of devices via BLE
pub fn ble_discovery() -> impl Stream<Item = DiscoveryEvent> {
    stream::channel(100, move |mut gui_sender| async move {
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
    })
}
