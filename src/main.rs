//! MeshChat is an iced GUI app that uses the meshtastic "rust" crate to discover and control
//! meshtastic compatible radios connected to the host running it

use crate::Message::{
    AddDeviceAlias, AddNodeAlias, AppError, AppNotification, ConfigChange, CopyToClipBoard,
    DeviceListViewEvent, DeviceViewEvent, Exit, Navigation, NewConfig, RemoveDeviceAlias,
    RemoveNodeAlias, RemoveNotification, ShowLocation, ToggleNodeFavourite, WindowEvent,
};
use crate::View::DeviceList;
use crate::channel_view::ChannelId;
use crate::config::{Config, load_config, save_config};
use crate::device_list_view::{DeviceListEvent, DeviceListView, ble_discovery};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnecting};
use crate::device_view::DeviceViewMessage::{DisconnectRequest, SubscriptionMessage};
use crate::device_view::{DeviceView, DeviceViewMessage};
use crate::linear::Linear;
use crate::notification::{Notification, Notifications};
use btleplug::api::BDAddr;
use iced::widget::{Column, Space};
use iced::{Element, Fill};
use iced::{Event, Subscription, Task, clipboard, window};
use std::cmp::PartialEq;
use std::time::Duration;

mod battery;
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
mod notification;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum View {
    #[default]
    DeviceList,
    Device(Option<ChannelId>),
}

#[derive(Default)]
struct MeshChat {
    config: Config,
    current_view: View,
    device_list_view: DeviceListView,
    device_view: DeviceView,
    notifications: Notifications,
}

#[derive(Debug, Clone)]
pub enum ConfigChangeMessage {
    DeviceAndChannel(Option<BDAddr>, Option<ChannelId>),
}

/// These are the messages that MeshChat responds to
#[derive(Debug, Clone)]
pub enum Message {
    Navigation(View),
    WindowEvent(Event),
    DeviceListViewEvent(DeviceListEvent),
    DeviceViewEvent(DeviceViewMessage),
    Exit,
    NewConfig(Config),
    ConfigChange(ConfigChangeMessage),
    ShowLocation(i32, i32), // lat and long / 1_000_000
    AppNotification(String, String),
    AppError(String, String),
    RemoveNotification(usize),
    ToggleNodeFavourite(u32),
    CopyToClipBoard(String),
    AddNodeAlias(u32, String),
    RemoveNodeAlias(u32),
    AddDeviceAlias(BDAddr, String),
    RemoveDeviceAlias(BDAddr),
    None,
}

fn main() -> iced::Result {
    iced::application(MeshChat::new, MeshChat::update, MeshChat::view)
        .subscription(MeshChat::subscription)
        .exit_on_close_request(false)
        .resizable(true)
        .font(icons::FONT)
        .title(MeshChat::title)
        .run()
}

impl MeshChat {
    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::batch(vec![load_config()]))
    }

    /// Return the title of the app, which is used in the window title bar
    /// This could vary with state, such as number of devices or unread messages or similar
    fn title(&self) -> String {
        let unread_count = self.device_view.unread_count();
        if unread_count > 0 {
            format!("MeshChat ({} unread) ", unread_count)
        } else {
            "MeshChat".to_string()
        }
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
            DeviceListViewEvent(device_list_event) => {
                self.device_list_view.update(device_list_event)
            }
            DeviceViewEvent(device_event) => self.device_view.update(device_event),
            Exit => window::latest().and_then(window::close),
            AppNotification(summary, detail) => {
                self.notifications.add(Notification::Info(summary, detail));
                Task::none()
            }
            AppError(summary, detail) => {
                self.notifications.add(Notification::Error(summary, detail));
                Task::none()
            }
            Message::None => Task::none(),
            NewConfig(config) => {
                self.config = config;
                if let Some(mac_address) = &self.config.device_mac_address {
                    self.device_view.update(DeviceViewMessage::ConnectRequest(
                        *mac_address,
                        self.config.channel_id.clone(),
                    ))
                } else {
                    Task::none()
                }
            }
            ConfigChange(config_change) => {
                // Merge in what has changed
                match config_change {
                    ConfigChangeMessage::DeviceAndChannel(mac_address, channel) => {
                        self.config.device_mac_address = mac_address;
                        self.config.channel_id = channel;
                    }
                }
                // and save it asynchronously, so that we don't block the GUI thread
                save_config(&self.config)
            }
            RemoveNotification(id) => {
                self.notifications.remove(id);
                Task::none()
            }
            ShowLocation(lat, long) => {
                let _ = webbrowser::open(&Self::location_url(lat, long));
                Task::none()
            }
            ToggleNodeFavourite(node_id) => {
                // Toggle the favourite status of the node in the config
                if !self.config.fav_nodes.remove(&node_id) {
                    let _ = self.config.fav_nodes.insert(node_id);
                }
                // and save the config asynchronously, so that we don't block the GUI thread
                save_config(&self.config)
            }
            CopyToClipBoard(string) => clipboard::write(string),
            AddNodeAlias(node_id, alias) => {
                self.device_view.stop_editing_alias();
                if !alias.is_empty() {
                    self.config.aliases.insert(node_id, alias);
                    save_config(&self.config)
                } else {
                    Task::none()
                }
            }
            RemoveNodeAlias(node_id) => {
                self.config.aliases.remove(&node_id);
                save_config(&self.config)
            }
            AddDeviceAlias(mac_address, alias) => {
                self.device_list_view.stop_editing_alias();
                if !alias.is_empty() {
                    self.config.device_aliases.insert(mac_address, alias);
                    save_config(&self.config)
                } else {
                    Task::none()
                }
            }
            RemoveDeviceAlias(mac_address) => {
                self.config.device_aliases.remove(&mac_address);
                save_config(&self.config)
            }
        }
    }

    /// Render the main app view
    fn view(&self) -> Element<'_, Message> {
        let state = self.device_view.connection_state();

        // Build the inner view and show busy if in DeviceList which is in discovery mode
        let (inner, scanning) = match self.current_view {
            DeviceList => (self.device_list_view.view(&self.config, state), true),
            View::Device(_) => (self.device_view.view(&self.config), false),
        };

        let header = match self.current_view {
            DeviceList => self.device_list_view.header(&self.config, state),
            View::Device(_) => self
                .device_view
                .header(&self.config, state, &self.device_list_view),
        };

        // Create the stack of elements, starting with the header
        let mut stack = Column::new().push(header);

        // If busy of connecting or disconnecting, add a busy bar to the header
        if scanning || matches!(state, Connecting(_) | Disconnecting(_)) {
            stack = stack.push(Space::new().width(Fill)).push(
                Linear::new()
                    .easing(easing::emphasized_accelerate())
                    .cycle_duration(Duration::from_secs_f32(2.0))
                    .width(Fill),
            );
        }

        // add the notification area and the inner view
        stack.push(self.notifications.view()).push(inner).into()
    }

    /// Convert a location tuple to a URL that can be opened in a browser.
    fn location_url(lat: i32, long: i32) -> String {
        let latitude = 0.0000001 * lat as f64;
        let longitude = 0.0000001 * long as f64;
        format!("https://maps.google.com/?q={},{}", latitude, longitude)
    }

    /// Subscribe to events from Discover and from Windows and from Devices (Radios)
    fn subscription(&self) -> Subscription<Message> {
        let subscriptions = vec![
            iced::event::listen().map(WindowEvent),
            Subscription::run(ble_discovery).map(DeviceListViewEvent),
            Subscription::run(device_subscription::subscribe)
                .map(|m| DeviceViewEvent(SubscriptionMessage(m))),
        ];

        Subscription::batch(subscriptions)
    }

    /// Navigate to show a different view, as defined by the [View] enum
    fn navigate(&mut self, view: View) -> Task<Message> {
        self.current_view = view.clone();
        if let View::Device(Some(channel_id)) = view {
            self.device_view
                .update(DeviceViewMessage::ShowChannel(Some(channel_id)))
        } else {
            Task::none()
        }
    }

    /// Handle window events, like close button or minimize button
    fn window_handler(&mut self, event: Event) -> Task<Message> {
        if let Event::Window(window::Event::CloseRequested) = event {
            if let Connected(mac_address) = self.device_view.connection_state() {
                self.device_view
                    .update(DisconnectRequest(*mac_address, true))
            } else {
                window::latest().and_then(window::close)
            }
        } else {
            Task::none()
        }
    }
}
