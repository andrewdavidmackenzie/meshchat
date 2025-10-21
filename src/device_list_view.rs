use crate::device_view::ConnectionState;
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::discovery::{compare_bleid, DiscoveryEvent};
use crate::Message::{Device, Navigation};
use crate::{name_from_id, Message, NavigationMessage};
use iced::widget::{button, container, text, Column, Row};
use iced::{Element, Length, Task};
use meshtastic::utils::stream::BleId;

#[derive(Default)]
pub struct DeviceListView {
    devices: Vec<BleId>,
}

async fn empty() {}

impl DeviceListView {
    pub fn update(&mut self, discovery_event: DiscoveryEvent) -> Task<Message> {
        match discovery_event {
            DiscoveryEvent::BLERadioFound(id) => self.devices.push(id),
            DiscoveryEvent::BLERadioLost(id) => self
                .devices
                .retain(|other_id| !compare_bleid(other_id, &id)),
            DiscoveryEvent::Error(e) => {
                return Task::perform(empty(), move |_| {
                    Message::AppError("Discovery Error".to_string(), e.to_string())
                });
            }
        };

        Task::none()
    }

    pub fn header<'a>(
        &'a self,
        mut header: Row<'a, Message>,
        connection_state: &'a ConnectionState,
    ) -> Row<'a, Message> {
        header = header.push(button("Devices")).push(text(" / "));

        match connection_state {
            Disconnected(_, _) => header.push(text("Disconnected")),
            Connecting(name) => header.push(text(format!("Connecting to {}", name))),
            Connected(name) => {
                let breadcrumb =
                    button(text(name)).on_press(Navigation(NavigationMessage::DeviceView));
                header.push(breadcrumb)
            }
            Disconnecting(name) => header.push(text(format!("Disconnecting from {}", name))),
        }
    }

    pub fn view(&self, connection_state: &ConnectionState) -> Element<'static, Message> {
        let mut main_col = Column::new();
        // TODO add a scanning bar at the top
        // TODO add a scrollable area in case there are a lot of devices

        for id in &self.devices {
            let mut device_row = Row::new();
            device_row = device_row.push(text(name_from_id(id)));
            match &connection_state {
                Connected(connected_device_name) => {
                    if connected_device_name.eq(&name_from_id(id)) {
                        device_row = device_row.push(
                            button("Disconnect")
                                .on_press(Device(DisconnectRequest(name_from_id(id)))),
                        );
                    }
                }
                // TODO maybe show an error against it if present?
                Disconnected(_id, _error) => {
                    device_row = device_row.push(
                        button("Connect").on_press(Device(ConnectRequest(name_from_id(id), None))),
                    );
                }
                Connecting(connecting_device) => {
                    if connecting_device.eq(&name_from_id(id)) {
                        device_row = device_row.push(text("Connecting"));
                    }
                }
                Disconnecting(disconnecting_device) => {
                    if disconnecting_device.eq(&name_from_id(id)) {
                        device_row = device_row.push(text("Disconnecting"));
                    }
                }
            }
            main_col = main_col.push(device_row);
        }

        container(main_col)
            .height(Length::Fill)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .into()
    }
}
