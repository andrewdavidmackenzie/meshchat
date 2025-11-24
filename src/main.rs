//! MeshChat is an iced GUI app that uses the meshtastic "rust" crate to discover and control
//! meshtastic compatible radios connected to the host running it

use crate::Message::{
    AppError, AppNotification, Device, DeviceListEvent, Exit, Navigation, NewConfig,
    RemoveNotification, SaveConfig, WindowEvent,
};
use crate::View::DeviceList;
use crate::config::{Config, load_config, save_config};
use crate::device_list_view::{DeviceListView, DiscoveryEvent, ble_discovery};
use crate::device_view::ConnectionState::Connected;
use crate::device_view::DeviceViewMessage::{DisconnectRequest, SubscriptionMessage};
use crate::device_view::{ConnectionState, DeviceView, DeviceViewMessage};
use crate::linear::Linear;
use crate::styles::button_chip_style;
use iced::border::Radius;
use iced::widget::container::Style;
use iced::widget::{Column, Container, Row, Space, button};
use iced::{Background, Element};
use iced::{Border, Bottom, Color, Event, Subscription, Task, Theme, window};
use meshtastic::utils::stream::BleDevice;
use std::cmp::PartialEq;
use std::time::Duration;

mod channel_view;
mod channel_view_entry;
mod config;
mod device_list_view;
mod device_subscription;
mod device_view;
mod easing;
mod linear;
mod styles;

#[rustfmt::skip]
/// Icons generated as a font using iced_fontello
mod icons;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum View {
    #[default]
    DeviceList,
    Device,
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
    current_view: View,
    device_list_view: DeviceListView,
    device_view: DeviceView,
    notifications: Vec<(usize, Notification)>,
    next_id: usize,
}

/// These are the messages that MeshChat responds to
#[derive(Debug, Clone)]
pub enum Message {
    Navigation(View),
    WindowEvent(Event),
    DeviceListEvent(DiscoveryEvent),
    Device(DeviceViewMessage),
    Exit,
    NewConfig(Config),
    SaveConfig(Config),
    ShowLocation(f64, f64), // lat and long
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
        .font(icons::FONT)
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

    /// Return the title of the app, which is used in the window title bar
    /// This could vary with state, such as number of devices or unread messages or similar
    fn title(&self) -> String {
        // Can enhance with the number of unread messages or something
        "MeshChat".to_string()
    }

    /// Update the app state based on a message received from the GUI.
    /// This is the main function of the app, and it drives the GUI.
    /// It is called every time a message is received from the GUI, and it updates the app state
    /// accordingly.
    /// It also handles window events like "minimize" and close buttons, and it handles navigation
    /// between views.
    /// It also handles the discovery of devices and the connection to them.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Navigation(view) => self.navigate(view),
            WindowEvent(event) => self.window_handler(event),
            DeviceListEvent(discovery_event) => self.device_list_view.update(discovery_event),
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
            Message::ShowLocation(lat, long) => {
                let maps_url = format!("https://maps.google.com/?q={},{}", lat, long);
                let _ = webbrowser::open(&maps_url);
                Task::none()
            }
        }
    }

    /// Create a header view for the top of the screen depending on the current state of the app
    /// and whether we are in discovery mode or not.
    fn header<'a>(&'a self, state: &'a ConnectionState, scanning: bool) -> Element<'a, Message> {
        let mut header = Column::new().padding(4);

        // Always add a button to allow navigating back to the list of devices
        let mut device_list_button = button("Devices").style(button_chip_style);

        // Activate it if we are not on the device list view
        if self.current_view != DeviceList {
            device_list_button = device_list_button.on_press(Navigation(DeviceList));
        }

        // Create the navigation bar and add it to the header
        let nav_bar =
            Row::new()
                .align_y(Bottom)
                .push(device_list_button)
                .push(match self.current_view {
                    DeviceList => self.device_list_view.header(state),
                    View::Device => self.device_view.header(state),
                });
        header = header.push(nav_bar);

        // If busy of connecting or disconnecting, add a busy bar to the header
        if scanning
            || matches!(
                state,
                ConnectionState::Connecting(_) | ConnectionState::Disconnecting(_)
            )
        {
            header = header.push(Space::new(iced::Length::Fill, 2)).push(
                Linear::new()
                    .easing(easing::emphasized_accelerate())
                    .cycle_duration(Duration::from_secs_f32(2.0))
                    .width(iced::Length::Fill),
            );
        }

        header.into()
    }

    /// Render the app view
    fn view(&self) -> Element<'_, Message> {
        let state = self.device_view.connection_state();

        // Build the inner view and show busy if in DeviceList which is in discovery mode
        let (inner, scanning) = match self.current_view {
            DeviceList => (self.device_list_view.view(state), true),
            View::Device => (self.device_view.view(), false),
        };

        let header = self.header(state, scanning);

        let mut outer = Column::new().push(header);
        outer = outer.push(self.notifications());
        outer = outer.push(inner);

        outer.into()
    }

    /// Subscribe to events from Discover and from Windows and from Devices (Radios)
    fn subscription(&self) -> Subscription<Message> {
        // TODO keyboard navigation
        //         iced::event::listen().map(ModalKeyEvent)
        let subscriptions = vec![
            iced::event::listen().map(WindowEvent),
            Subscription::run(ble_discovery).map(DeviceListEvent),
            Subscription::run(device_subscription::subscribe)
                .map(|m| Device(SubscriptionMessage(m))),
        ];

        Subscription::batch(subscriptions)
    }

    /// Navigate to show a different view, as defined by the [View] enum
    fn navigate(&mut self, view: View) -> Task<Message> {
        self.current_view = view;
        Task::none()
    }

    async fn empty() {}

    /// Handle window events, like close button or minimize button
    fn window_handler(&mut self, event: Event) -> Task<Message> {
        if let Event::Window(window::Event::CloseRequested) = event {
            if let Connected(device) = self.device_view.connection_state().clone() {
                Task::perform(Self::empty(), move |_| {
                    Device(DisconnectRequest(device.clone(), true))
                })
            } else {
                window::get_latest().and_then(window::close)
            }
        } else {
            Task::none()
        }
    }

    /// Add a notification to the list of notifications to display at the top of the screen.
    /// Notifications are displayed in a list, with the most recent at the top.
    /// Each notification has a unique id, which is used to remove it from the list.
    fn add_notification(&mut self, notification: Notification) {
        self.notifications.push((self.next_id, notification));
        self.next_id += 1;
    }

    /// Remove a notification from the list of notifications displayed at the top of the screen.
    /// Use the unique id to identify it
    fn remove_notification(&mut self, id: usize) {
        self.notifications.retain(|item| item.0 != id);
    }

    /// Render any notifications that are active into an element to display at the top of the screen
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
