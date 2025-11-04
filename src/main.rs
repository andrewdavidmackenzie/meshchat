//! MeshChat is an iced GUI app that uses the meshtastic "rust" crate to discover and control
//! meshtastic compatible radios connected to the host running it

use crate::config::{load_config, save_config, Config};
use crate::device_list_view::DeviceListView;
use crate::device_view::ConnectionState::Connected;
use crate::device_view::{ConnectionState, DeviceView, DeviceViewMessage};
use crate::discovery::{ble_discovery, DiscoveryEvent};
use crate::linear::Linear;
use crate::styles::chip_style;
use crate::Message::{
    AppError, AppNotification, Device, Discovery, Exit, Navigation, NewConfig, RemoveNotification,
    SaveConfig, WindowEvent,
};
use iced::border::Radius;
use iced::widget::container::Style;
use iced::widget::{button, Column, Container, Row};
use iced::{window, Border, Bottom, Color, Event, Subscription, Task, Theme};
use iced::{Background, Element};
use meshtastic::utils::stream::BleDevice;
use std::cmp::PartialEq;
use std::time::Duration;

mod channel_message;
mod channel_view;
mod config;
mod device_list_view;
mod device_subscription;
mod device_view;
mod discovery;
mod easing;
mod linear;
mod styles;
// mod router;

#[derive(PartialEq, Default)]
enum View {
    #[default]
    DeviceList,
    Device,
}

#[derive(Debug, Clone)]
pub enum NavigationMessage {
    DevicesList,
    DeviceView,
}

/// A [Notification] can be one of two notification types:
/// - Error(summary, detail)
/// - Info(summary, detail)
enum Notification {
    Error(String, String),
    Info(String, String),
}

#[derive(Default)]
struct MeshChat {
    view: View,
    device_list_view: DeviceListView,
    device_view: DeviceView,
    notifications: Vec<(usize, Notification)>,
    next_id: usize,
}

/// These are the messages that MeshChat responds to
#[derive(Debug, Clone)]
pub enum Message {
    Navigation(NavigationMessage),
    WindowEvent(Event),
    Discovery(DiscoveryEvent),
    Device(DeviceViewMessage),
    Exit,
    NewConfig(Config),
    SaveConfig(Config),
    AppNotification(String, String),
    AppError(String, String),
    RemoveNotification(usize),
    None,
}

fn main() -> iced::Result {
    iced::application(MeshChat::title, MeshChat::update, MeshChat::view)
        .subscription(MeshChat::subscription)
        .exit_on_close_request(false)
        .resizable(true)
        .run_with(MeshChat::new)
}

pub fn name_from_id(id: &BleDevice) -> String {
    id.name
        .clone()
        .unwrap_or_else(|| id.mac_address.to_string())
}

impl MeshChat {
    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::batch(vec![load_config()]))
    }

    fn title(&self) -> String {
        // Can enhance with the number of unread messages or something
        "MeshChat".to_string()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Navigation(navigation_message) => self.navigate(navigation_message),
            WindowEvent(event) => self.window_handler(event),
            Discovery(discovery_event) => self
                .device_list_view
                .update(discovery_event, self.device_view.connection_state()),
            Device(device_event) => self.device_view.update(device_event),
            Exit => window::get_latest().and_then(window::close),
            NewConfig(config) => {
                if let Some(device) = &config.device {
                    self.device_view.update(DeviceViewMessage::ConnectRequest(
                        device.clone(),
                        config.channel_id,
                    ))
                } else {
                    Task::none()
                }
            }
            AppNotification(summary, detail) => {
                self.add_notification(Notification::Info(summary, detail));
                Task::none()
            }
            AppError(summary, detail) => {
                self.add_notification(Notification::Error(summary, detail));
                Task::none()
            }
            Message::None => Task::none(),
            SaveConfig(config) => save_config(config),
            RemoveNotification(id) => {
                self.remove_notification(id);
                Task::none()
            }
        }
    }

    fn add_notification(&mut self, notification: Notification) {
        self.notifications.push((self.next_id, notification));
        self.next_id += 1;
    }

    fn remove_notification(&mut self, id: usize) {
        self.notifications.retain(|item| item.0 != id);
    }

    fn view(&self) -> Element<'_, Message> {
        let state = self.device_view.connection_state();

        // Always add a button to allow navigating back to the list of devices
        let mut header = Row::new().align_y(Bottom).push(
            button("Devices")
                .style(chip_style)
                .on_press(Navigation(NavigationMessage::DevicesList)),
        );

        header = header.push(match self.view {
            View::DeviceList => self.device_list_view.header(state),
            View::Device => self.device_view.header(state),
        });

        // Build the inner view
        let (inner, mut busy) = match self.view {
            View::DeviceList => (self.device_list_view.view(state), true),
            View::Device => (self.device_view.view(), false),
        };

        // Also busy if we are connecting or disconnecting
        busy = busy
            || matches!(
                state,
                ConnectionState::Connecting(_) | ConnectionState::Disconnecting(_)
            );

        let mut outer = Column::new().push(header);
        if busy {
            outer = outer.push(
                Linear::new()
                    .easing(&easing::EMPHASIZED_ACCELERATE)
                    .cycle_duration(Duration::from_secs_f32(2.0))
                    .width(iced::Length::Fill),
            );
        }
        outer = outer.push(self.notifications());
        outer = outer.push(inner);

        outer.into()
    }

    /// Subscribe to events from Discover and from Windows and from Devices (Radios)
    fn subscription(&self) -> Subscription<Message> {
        let subscriptions = vec![
            iced::event::listen().map(WindowEvent),
            Subscription::run(ble_discovery).map(Discovery),
            self.device_view.subscription().map(Device),
        ];

        Subscription::batch(subscriptions)
    }

    fn navigate(&mut self, navigation_message: NavigationMessage) -> Task<Message> {
        match navigation_message {
            NavigationMessage::DevicesList => {
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

    fn notifications(&self) -> Element<'_, Message> {
        let mut notifications = Row::new().padding(10);

        // TODO a box with color and a cancel button that removes this error
        // larger font for summary, detail can be unfolded
        for (id, notification) in &self.notifications {
            match notification {
                Notification::Error(summary, details) => {
                    notifications =
                        notifications.push(Self::error_box(*id, summary.clone(), details.clone()));
                }
                Notification::Info(summary, details) => {
                    notifications =
                        notifications.push(Self::info_box(*id, summary.clone(), details.clone()));
                }
            }
        }

        notifications.into()
    }

    // TODO accept detail and make it in an expandable box
    fn error_box(id: usize, summary: String, _detail: String) -> Element<'static, Message> {
        let row = Row::new().push(iced::widget::text(summary));
        let row = row.push(button("OK").on_press(RemoveNotification(id)));

        Container::new(row)
            .padding([6, 12]) // adjust to taste
            .style(|_theme: &Theme| Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0xE5, 0x2D, 0x2C))), // red
                border: Border {
                    radius: Radius::from(12.0), // rounded corners
                    width: 2.0,
                    color: Color::from_rgb8(0xFF, 0xD7, 0x00), // yellow
                },
                ..Default::default()
            })
            .into()
    }

    // TODO accept detail and make it in an expandable box
    fn info_box(id: usize, summary: String, _detail: String) -> Element<'static, Message> {
        let row = Row::new().push(iced::widget::text(summary));
        let row = row.push(button("OK").on_press(RemoveNotification(id)));

        Container::new(row)
            .padding([6, 12]) // adjust to taste
            .style(|_theme: &Theme| Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0x00, 0x00, 0x00))), // black
                border: Border {
                    radius: Radius::from(12.0), // rounded corners
                    width: 2.0,
                    color: Color::WHITE,
                },
                ..Default::default()
            })
            .into()
    }
}
