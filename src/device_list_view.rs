use crate::Message::{DeviceViewEvent, Navigation};
use crate::device_list_view::DiscoveryEvent::{BLERadioFound, BLERadioLost, Error};
use crate::device_view::ConnectionState;
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::styles::button_chip_style;
use crate::{Message, View};
use iced::futures::{SinkExt, Stream};
use iced::widget::scrollable::Scrollbar;
use iced::widget::{Column, Container, Row, Space, button, container, scrollable, text};
use iced::{Bottom, stream};
use iced::{Center, Element, Fill, Task, alignment};
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

    /// Create a header view for the top of the screen
    pub fn header<'a>(&'a self, state: &'a ConnectionState) -> Element<'a, Message> {
        let mut header = Row::new()
            .padding(4)
            .align_y(Bottom)
            .push(button("Devices").style(button_chip_style));

        header = header.push(match state {
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
        });

        // Add a disconnect button on the right if we are connected
        if let Connected(device) = state {
            header = header.push(Space::new(Fill, 1)).push(
                button("Disconnect")
                    .on_press(DeviceViewEvent(DisconnectRequest(device.clone(), false)))
                    .style(button_chip_style),
            )
        }

        header.into()
    }

    pub fn view(&self, connection_state: &ConnectionState) -> Element<'_, Message> {
        if self.discovered_devices.is_empty() {
            return empty_view();
        }

        let mut main_col = Column::new();

        for device in &self.discovered_devices {
            let mut device_row = Row::new().align_y(Center).padding(2);
            device_row = device_row.push(text(device.name.as_ref().unwrap()).width(150));
            device_row = device_row.push(Space::new(6, 0));
            match &connection_state {
                Connected(connected_device) => {
                    device_row = device_row.push(
                        button("Disconnect")
                            .on_press(DeviceViewEvent(DisconnectRequest(
                                connected_device.clone(),
                                false,
                            )))
                            .style(button_chip_style),
                    );
                }
                Disconnected(_id, _error) => {
                    device_row = device_row.push(
                        button("Connect")
                            .on_press(DeviceViewEvent(ConnectRequest(device.clone(), None)))
                            .style(button_chip_style),
                    );
                }
                Connecting(connecting_device) => {
                    if connecting_device == device {
                        device_row = device_row.push(button("Connecting").style(button_chip_style));
                    }
                }
                Disconnecting(disconnecting_device) => {
                    if disconnecting_device == device {
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
