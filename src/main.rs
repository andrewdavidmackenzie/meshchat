//! MeshChat is an iced GUI app that uses the meshtastic "rust" crate to discover and control
//! meshtastic compatible radios connected to the host running it

mod comms;
mod device_list_view;
mod device_view;
mod discovery;

use crate::device_list_view::DeviceListView;
use crate::device_view::{DeviceEvent, DeviceView};
use crate::discovery::{ble_discovery, DiscoveryEvent};
use crate::Message::{Device, Discovery, Exit, NavigationBack, WindowEvent};
use iced::{window, Element, Pixels, Settings, Subscription, Task, Theme};
use std::cmp::PartialEq;

const MESHCHAT_ID: &str = "meshchat";

#[derive(PartialEq)]
enum View {
    DeviceList,
    Device,
}

struct MeshChat {
    view: View,
    device_list_view: DeviceListView,
    device_view: DeviceView,
}

/// These are the messages that MeshChat responds to
#[derive(Debug, Clone)]
pub enum Message {
    NavigationBack,
    WindowEvent(iced::Event),
    Discovery(DiscoveryEvent),
    Device(DeviceEvent),
    Exit,
}

fn main() -> iced::Result {
    let settings = Settings {
        id: Some(MESHCHAT_ID.into()),
        default_text_size: Pixels(14.0),
        ..Default::default()
    };

    iced::application(MeshChat::title, MeshChat::update, MeshChat::view)
        .subscription(MeshChat::subscription)
        .exit_on_close_request(false)
        .resizable(true)
        .settings(settings)
        .theme(|_| Theme::Dark)
        .run_with(MeshChat::new)
}

impl MeshChat {
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                view: View::DeviceList,
                device_list_view: DeviceListView::new(),
                device_view: DeviceView::new(),
            },
            Task::batch(vec![]),
        )
    }

    fn title(&self) -> String {
        "MeshChat".to_string()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            NavigationBack => {
                if self.view == View::Device {
                    self.view = View::DeviceList;
                }
                Task::none()
            }
            WindowEvent(event) => {
                if let iced::Event::Window(window::Event::CloseRequested) = event {
                    if let Some(id) = self.device_view.connected_device() {
                        Task::perform(comms::do_disconnect(id.clone()), |_result| Exit)
                    } else {
                        window::get_latest().and_then(window::close)
                    }
                } else {
                    Task::none()
                }
            }
            Discovery(discovery_event) => self.device_list_view.update(discovery_event),
            Device(device_event) => {
                let (show_device_view, task) = self.device_view.update(device_event);
                if show_device_view {
                    self.view = View::Device;
                } else {
                    self.view = View::DeviceList;
                };
                task
            }
            Exit => window::get_latest().and_then(window::close),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match self.view {
            View::DeviceList => self
                .device_list_view
                .view(self.device_view.connected_device()),
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
}
