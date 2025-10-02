use crate::device_view::DeviceEvent::{
    ConnectedEvent, DeviceConnect, DeviceDisconnect, DisconnectedEvent,
};
use crate::device_view::State::{Connected, Connecting, Disconnected, Disconnecting};
use crate::{comms, Message};
use iced::widget::{button, container, text, Column};
use iced::{Element, Length, Task};
use meshtastic::utils::stream::BleId;

enum State {
    Disconnected,
    Connecting(BleId),
    Connected(BleId),
    Disconnecting(BleId),
}

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    DeviceConnect(BleId),
    DeviceDisconnect(BleId),
    ConnectedEvent(BleId),
    DisconnectedEvent(BleId),
}

pub struct DeviceView {
    state: State,
}

impl DeviceView {
    pub fn new() -> Self {
        Self {
            state: Disconnected,
        }
    }

    pub fn connected_device(&self) -> Option<&BleId> {
        match &self.state {
            Connected(id) => Some(id),
            _ => None,
        }
    }

    /// Return a true value to show we can show the device view, false for main to decide
    pub fn update(&mut self, device_event: DeviceEvent) -> (bool, Task<Message>) {
        match device_event {
            DeviceConnect(id) => {
                self.state = Connecting(id.clone());
                (
                    true,
                    Task::perform(comms::do_connect(id.clone()), |result| {
                        Message::Device(ConnectedEvent(result.unwrap()))
                    }),
                )
            }
            DeviceDisconnect(id) => {
                self.state = Disconnecting(id.clone());
                (
                    true,
                    Task::perform(comms::do_disconnect(id.clone()), |result| {
                        Message::Device(DisconnectedEvent(result.unwrap()))
                    }),
                )
            }
            ConnectedEvent(id) => {
                self.state = Connected(id);
                (true, Task::none())
            }
            DisconnectedEvent(_) => {
                self.state = Disconnected;
                (false, Task::none())
            }
        }
    }

    pub fn view(&self) -> Element<'static, Message> {
        let mut main_col = Column::new();

        main_col = main_col.push(button("<-- Back").on_press(Message::NavigationBack));

        match &self.state {
            Disconnected => {
                main_col = main_col.push(text("disconnected"));
            }

            Connecting(id) => {
                main_col = main_col.push(text(format!("connecting to : {id}")));
            }

            Connected(id) => {
                main_col = main_col.push(text(format!("connected to : {id}")));
                main_col = main_col.push(
                    button("Disconnect").on_press(Message::Device(DeviceDisconnect(id.clone()))),
                );
            }
            State::Disconnecting(id) => {
                main_col = main_col.push(text(format!("disconnecting from : {id}")));
            }
        }

        let content = container(main_col)
            .height(Length::Fill)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left);

        content.into()
    }
}
