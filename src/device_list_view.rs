use crate::device_view::ConnectionState;
use crate::device_view::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::discovery::{compare_bleid, DiscoveryEvent};
use crate::Message;
use crate::Message::Device;
use iced::widget::{button, container, text, Column, Row};
use iced::{Element, Length, Task};
use meshtastic::utils::stream::BleId;

pub struct DeviceListView {
    devices: Vec<BleId>,
}

impl DeviceListView {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    pub fn update(&mut self, discovery_event: DiscoveryEvent) -> Task<Message> {
        match discovery_event {
            DiscoveryEvent::BLERadioFound(id) => self.devices.push(id),
            DiscoveryEvent::BLERadioLost(id) => self
                .devices
                .retain(|other_id| !compare_bleid(other_id, &id)),
            DiscoveryEvent::Error(_) => {}
        };

        Task::none()
    }

    pub fn view(&self, connection_state: &ConnectionState) -> Element<'static, Message> {
        let mut main_col = Column::new();
        main_col = main_col.push(text("Scanning...Available devices:"));

        for id in &self.devices {
            let mut device_row = Row::new();
            device_row = device_row.push(text(id.to_string()));
            match &connection_state {
                ConnectionState::Connected(connected_device_id) => {
                    if compare_bleid(connected_device_id, id) {
                        device_row = device_row.push(
                            button("Disconnect").on_press(Device(DisconnectRequest(id.clone()))),
                        );
                    }
                }
                ConnectionState::Disconnected => {
                    device_row = device_row
                        .push(button("Connect").on_press(Device(ConnectRequest(id.clone()))));
                }
                ConnectionState::Connecting(connecting_device) => {
                    if compare_bleid(connecting_device, id) {
                        device_row = device_row.push(text("Connecting"));
                    }
                }
                ConnectionState::Disconnecting(disconnecting_device) => {
                    if compare_bleid(disconnecting_device, id) {
                        device_row = device_row.push(text("Disconnecting"));
                    }
                }
            }
            main_col = main_col.push(device_row);
        }

        let content = container(main_col)
            .height(Length::Fill)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left);

        content.into()
    }
}
