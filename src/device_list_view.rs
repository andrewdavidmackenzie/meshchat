use crate::device_view::DeviceEvent::{DeviceConnect, DeviceDisconnect};
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

    pub fn view(&self, connected_device: Option<&BleId>) -> Element<'static, Message> {
        let mut main_col = Column::new();
        main_col = main_col.push(text("Scanning...Available devices:"));

        for id in &self.devices {
            let mut device_row = Row::new();
            let mut device_button = button(text(id.to_string()));
            if let Some(connected_device) = connected_device {
                if compare_bleid(connected_device, id) {
                    device_row = device_row.push(device_button);
                    device_row = device_row
                        .push(button("Disconnect").on_press(Device(DeviceDisconnect(id.clone()))));
                }
            } else {
                device_button = device_button.on_press(Device(DeviceConnect(id.clone())));
                device_row = device_row.push(device_button);
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
