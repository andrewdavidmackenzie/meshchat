//! MeshChat is an iced GUI app that uses the meshtastic "rust" crate to discover and control
//! meshtastic compatible radios connected to the host running it

mod comms;
mod device_list_view;
mod device_view;
mod discovery;

use crate::device_list_view::DeviceListView;
use crate::device_view::{ConnectionState, DeviceEvent, DeviceView};
use crate::discovery::{ble_discovery, DiscoveryEvent};
use crate::Message::{Device, Discovery, Exit, Navigation, WindowEvent};
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
    Connected,
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
    WindowEvent(iced::Event),
    Discovery(DiscoveryEvent),
    Device(DeviceEvent),
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
        match self.view {
            View::DeviceList => self
                .device_list_view
                .view(self.device_view.connection_state()),
            View::Device => self.device_view.view(),
        }
    }

    /// Subscribe to events from Discover and from Windows
    fn subscription(&self) -> Subscription<Message> {
        #[allow(unused_mut)]
        let mut subscriptions = vec![
            iced::event::listen().map(WindowEvent),
            Subscription::run(ble_discovery).map(Discovery),
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
            NavigationMessage::Connected => {
                self.view = View::Device;
            }
        }
        Task::none()
    }

    fn window_handler(&mut self, event: Event) -> Task<Message> {
        if let Event::Window(window::Event::CloseRequested) = event {
            if let ConnectionState::Connected(id) = self.device_view.connection_state() {
                Task::perform(comms::do_disconnect(id.clone()), |_result| Exit)
            } else {
                window::get_latest().and_then(window::close)
            }
        } else {
            Task::none()
        }
    }
}
