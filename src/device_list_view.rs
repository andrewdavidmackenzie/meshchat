use crate::Message;
use crate::Message::Device;
use crate::device_view::DeviceEvent::DeviceConnect;
use crate::discovery::DiscoveryEvent;
use btleplug::platform::PeripheralId;
use iced::widget::{Column, button, container, text};
use iced::{Element, Length, Task};
use std::collections::HashMap;

pub struct DeviceListView {
    devices: HashMap<PeripheralId, String>,
}

impl DeviceListView {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
        }
    }

    pub fn update(&mut self, discovery_event: DiscoveryEvent) -> Task<Message> {
        match discovery_event {
            DiscoveryEvent::BLERadioFound(id, name) => self.devices.insert(id, name),
            DiscoveryEvent::BLERadioLost(id) => self.devices.remove(&id),
        };

        Task::none()
    }

    pub fn view(&self) -> Element<'static, Message> {
        let mut main_col = Column::new();

        for (id, name) in &self.devices {
            main_col = main_col
                .push(button(text(name.clone())).on_press(Device(DeviceConnect(id.clone()))));
        }

        let content = container(main_col)
            .height(Length::Fill)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left);

        content.into()
    }
}
