use crate::Message;
use crate::device_view::DeviceEvent::{ConnectedEvent, DeviceConnect, DeviceDisconnect};
use crate::device_view::State::{Connected, Connecting, Disconnected};
use btleplug::platform::PeripheralId;
use iced::widget::{Column, button, container, text};
use iced::{Element, Length, Task};

enum State {
    Disconnected,
    Connecting(PeripheralId),
    Connected(PeripheralId),
}

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    DeviceConnect(PeripheralId),
    DeviceDisconnect(PeripheralId),
    ConnectedEvent(PeripheralId),
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

    /// Return a true value to show we can show the device view, false for main to decide
    pub fn update(&mut self, device_event: DeviceEvent) -> (bool, Task<Message>) {
        match device_event {
            DeviceConnect(id) => {
                self.state = Connecting(id.clone());
                (
                    true,
                    Task::perform(Self::do_connect(id.clone()), |result| {
                        Message::Device(ConnectedEvent(result.unwrap()))
                    }),
                )
            }
            DeviceDisconnect(_id) => {
                self.state = Disconnected;
                (false, Task::none())
            }
            ConnectedEvent(id) => {
                self.state = Connected(id);
                (true, Task::none())
            }
        }
    }

    async fn do_connect(id: PeripheralId) -> Result<PeripheralId, btleplug::Error> {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        Ok(id)
    }

    pub fn view(&self) -> Element<'static, Message> {
        let mut main_col = Column::new();

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
        }

        let content = container(main_col)
            .height(Length::Fill)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left);

        content.into()
    }
}
