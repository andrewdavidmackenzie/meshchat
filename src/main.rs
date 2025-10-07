//! MeshChat is an iced GUI app that uses the meshtastic "rust" crate to discover and control
//! meshtastic compatible radios connected to the host running it

mod device_list_view;
mod device_subscription;
mod device_view;
mod discovery;

use crate::device_list_view::DeviceListView;
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnected, Disconnecting};
use crate::device_view::{DeviceView, DeviceViewMessage};
use crate::discovery::{ble_discovery, DiscoveryEvent};
use crate::Message::{Device, Discovery, Exit, Navigation, WindowEvent};
use iced::widget::{button, text, Column, Row};
use iced::{window, Element, Event, Subscription, Task};
use std::cmp::PartialEq;

#[derive(PartialEq)]
enum View {
    DeviceList,
    Device,
}

#[derive(Debug, Clone)]
pub enum NavigationMessage {
    Back,
    DeviceView,
}

struct MeshChat {
    view: View,
    device_list_view: DeviceListView,
    device_view: DeviceView,
}

/// These are the messages that MeshChat responds to
#[derive(Debug, Clone)]
pub enum Message {
    Navigation(NavigationMessage),
    WindowEvent(Event),
    Discovery(DiscoveryEvent),
    Device(DeviceViewMessage),
    Exit,
}

fn main() -> iced::Result {
    iced::application(MeshChat::title, MeshChat::update, MeshChat::view)
        .subscription(MeshChat::subscription)
        .exit_on_close_request(false)
        .resizable(true)
        .run_with(MeshChat::new)
}

impl MeshChat {
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                view: View::DeviceList, // Make the initial view the device list
                device_list_view: DeviceListView::new(),
                device_view: DeviceView::new(),
            },
            Task::batch(vec![]),
        )
    }

    fn title(&self) -> String {
        // Can enhance with the number of unread messages or something
        "MeshChat".to_string()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Navigation(navigation_message) => self.navigate(navigation_message),
            WindowEvent(event) => self.window_handler(event),
            Discovery(discovery_event) => self.device_list_view.update(discovery_event),
            Device(device_event) => self.device_view.update(device_event),
            Exit => window::get_latest().and_then(window::close),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let (inner, back) = match self.view {
            View::DeviceList => (
                self.device_list_view
                    .view(self.device_view.connection_state()),
                false,
            ),
            View::Device => (self.device_view.view(), true),
        };

        let mut outer = Column::new();
        let mut header = Row::new();

        if back {
            header = header.push(button("<-- Back").on_press(Navigation(NavigationMessage::Back)));
        }

        match &self.device_view.connection_state() {
            Disconnected => {
                header = header.push(text("Disconnected"));
            }
            Connecting(id) => {
                header = header.push(text(format!("Connecting to {}", id)));
            }
            Connected(id) => {
                header = header.push(text("Connected to "));
                header = header.push(
                    button(text(id.to_string()))
                        .on_press(Navigation(NavigationMessage::DeviceView)),
                );
            }
            Disconnecting(id) => {
                header = header.push(text(format!("Disconnecting from {}", id)));
            }
        }

        outer = outer.push(header);
        outer = outer.push(inner);

        outer.into()
    }

    /// Subscribe to events from Discover and from Windows
    fn subscription(&self) -> Subscription<Message> {
        #[allow(unused_mut)]
        let mut subscriptions = vec![
            iced::event::listen().map(WindowEvent),
            Subscription::run(ble_discovery).map(Discovery),
            self.device_view.subscription().map(Device),
        ];

        Subscription::batch(subscriptions)
    }

    fn navigate(&mut self, navigation_message: NavigationMessage) -> Task<Message> {
        match navigation_message {
            NavigationMessage::Back => {
                if self.view == View::Device {
                    self.view = View::DeviceList;
                }
            }
            NavigationMessage::DeviceView => {
                self.view = View::Device;
            }
        }
        Task::none()
    }

    fn window_handler(&mut self, event: Event) -> Task<Message> {
        if let Event::Window(window::Event::CloseRequested) = event {
            if let Connected(_id) = self.device_view.connection_state().clone() {
                // TODO send message to subscription to request we disconnect
                Task::none()
            } else {
                window::get_latest().and_then(window::close)
            }
        } else {
            Task::none()
        }
    }
}
