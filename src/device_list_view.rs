use crate::device_view::ConnectionState;
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::DeviceViewMessage::{ConnectRequest, DisconnectRequest};
use crate::discovery::{compare_bleid, DiscoveryEvent};
use crate::Message::{Device, Navigation};
use crate::{name_from_id, Message, NavigationMessage};
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

    pub fn header<'a>(
        &'a self,
        mut header: Row<'a, Message>,
        connection_state: &ConnectionState,
    ) -> Row<'a, Message> {
        header = header.push(button("Devices")).push(text(" / "));

        match connection_state {
            Disconnected => header.push(text("Disconnected")),
            Connecting(id) => header.push(text(format!("Connecting to {}", name_from_id(id)))),
            Connected(id) => {
                let breadcrumb = button(text(name_from_id(id)))
                    .on_press(Navigation(NavigationMessage::DeviceView));
                header.push(breadcrumb)
            }
            Disconnecting(id) => {
                header.push(text(format!("Disconnecting from {}", name_from_id(id))))
            }
        }
    }

    pub fn view(&self, connection_state: &ConnectionState) -> Element<'static, Message> {
        let mut main_col = Column::new();
        // TODO make this a spinner in the view, or a bar back and fore - something visual
        main_col = main_col.push(text("Scanning...Available devices:"));
        // TODO add a scrollable area in case there are a lot of devices

        for id in &self.devices {
            let mut device_row = Row::new();
            device_row = device_row.push(text(name_from_id(id)));
            match &connection_state {
                Connected(connected_device_id) => {
                    if compare_bleid(connected_device_id, id) {
                        device_row = device_row.push(
                            button("Disconnect").on_press(Device(DisconnectRequest(id.clone()))),
                        );
                    }
                }
                Disconnected => {
                    device_row = device_row
                        .push(button("Connect").on_press(Device(ConnectRequest(id.clone()))));
                }
                Connecting(connecting_device) => {
                    if compare_bleid(connecting_device, id) {
                        device_row = device_row.push(text("Connecting"));
                    }
                }
                Disconnecting(disconnecting_device) => {
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
