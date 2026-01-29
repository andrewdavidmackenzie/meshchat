//! MeshChat is an iced GUI app that uses the mesht "rust" crate to discover and control
//! mesht compatible radios connected to the host running it

#[cfg(feature = "auto-update")]
use crate::Message::UpdateChecked;
use crate::Message::{
    AddDeviceAlias, AddNodeAlias, AppError, AppNotification, CloseSettingsDialog, CloseShowUser,
    ConfigLoaded, CopyToClipBoard, DeviceAndChannelChange, DeviceListViewEvent, DeviceViewEvent,
    Exit, HistoryLengthSelected, Navigation, OpenSettingsDialog, OpenUrl, RemoveDeviceAlias,
    RemoveNodeAlias, RemoveNotification, ShowLocation, ShowUserInfo, ToggleAutoReconnect,
    ToggleAutoUpdate, ToggleNodeFavourite, ToggleShowPositionUpdates, ToggleShowUserUpdates,
    WindowEvent,
};
use crate::View::DeviceList;
use crate::channel_id::ChannelId;
use crate::channel_view_entry::MCMessage;
use crate::config::{Config, HistoryLength, load_config};
use crate::device_list_view::{DeviceListEvent, DeviceListView};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnecting};
use crate::device_view::DeviceView;
use crate::device_view::DeviceViewMessage;
use crate::device_view::DeviceViewMessage::{DisconnectRequest, SubscriptionMessage};
use crate::discovery::ble_discovery;
use crate::linear::Linear;
use crate::mesht::device_subscription;
use crate::notification::{Notification, Notifications};
use crate::styles::{picker_header_style, tooltip_style};
use iced::font::Weight;
use iced::keyboard::key;
use iced::widget::{Column, Space, center, container, mouse_area, opaque, operation, stack, text};
use iced::window::icon;
use iced::{Center, Color, Event, Font, Subscription, Task, clipboard, keyboard, window};
use iced::{Element, Fill, event};
use meshtastic::protobufs::FromRadio;
#[cfg(feature = "auto-update")]
use self_update::Status;
use std::cmp::PartialEq;
use std::fmt;
use std::fmt::Formatter;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Sender;

mod battery;
mod channel_view;
mod channel_view_entry;
mod config;
mod device_list_view;
mod device_view;
mod discovery;
mod easing;
mod linear;
mod styles;

#[rustfmt::skip]
/// Icons generated as a font using iced_fontello
mod icons;
mod channel_id;
mod emoji_picker;
mod mesht;
mod notification;
#[cfg(test)]
mod test_helper;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A User as represented in the App, maybe a superset of User attributes from different meshes
#[derive(Debug, Clone)]
pub struct MCUser {
    pub id: String,
    pub long_name: String,
    pub short_name: String,
    pub hw_model_str: String,
    pub hw_model: i32,
    pub is_licensed: bool,
    pub role_str: String,
    pub role: i32,
    pub public_key: Vec<u8>,
    pub is_unmessagable: bool,
}

#[allow(clippy::from_over_into)]
impl fmt::Display for MCUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ⓘ from '{}' ('{}'), id = '{}', with hardware '{}'",
            self.long_name, self.short_name, self.id, self.hw_model_str
        )
    }
}

/// A Node's Info as represented in the App, maybe a superset of User attributes from different meshes
#[derive(Clone, Debug)]
pub struct MCNodeInfo {
    pub num: u32,
    pub user: Option<MCUser>,
    pub position: Option<MCPosition>,
    pub channel: u32,
    pub is_ignored: bool,
}

#[derive(Clone, Debug)]
pub struct MCPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<i32>,
    pub time: u32,
    pub location_source: i32,
    pub altitude_source: i32,
    pub timestamp: u32,
    pub timestamp_millis_adjust: i32,
    pub altitude_hae: Option<i32>,
    pub altitude_geoidal_separation: Option<i32>,
    pub pdop: u32,
    pub hdop: u32,
    pub vdop: u32,
    pub gps_accuracy: u32,
    pub ground_speed: Option<u32>,
    pub ground_track: Option<u32>,
    pub fix_quality: u32,
    pub fix_type: u32,
    pub sats_in_view: u32,
    pub sensor_id: u32,
    pub next_update: u32,
    pub seq_number: u32,
    pub precision_bits: u32,
}

#[allow(clippy::from_over_into)]
impl fmt::Display for MCPosition {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "📌 {:.2}, {:.2}", self.latitude, self.longitude)
    }
}

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
    showing_settings: bool,
    show_user: Option<MCUser>,
}

#[derive(Clone, Debug)]
pub struct MCChannel {
    pub index: i32,
    pub name: String,
}

/// Events are Messages sent from the subscription to the GUI
#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    /// The subscription is ready to receive [SubscriberMessage] from the GUI
    Ready(Sender<SubscriberMessage>),
    ConnectedEvent(String),
    ConnectingEvent(String),
    DisconnectingEvent(String),
    DisconnectedEvent(String),
    ConnectionError(String, String, String),
    NotReady,
    MyNodeNum(u32),
    NewChannel(MCChannel),
    NewNode(MCNodeInfo),
    RadioNotification(String, u32), // Message, rx_time
    MessageACK(ChannelId, u32),
    MCMessageReceived(ChannelId, u32, u32, MCMessage, u32), // channel, id, from, MCMessage, rx_time
    NewNodeInfo(ChannelId, u32, u32, MCUser, u32),          // channel_id, id, from, MCUser, rx_time
    NewNodePosition(ChannelId, u32, u32, MCPosition, u32), // channel_id, id, from, MCPosition, rx_time
    DeviceBatteryLevel(Option<u32>),
}

/// Messages sent from the GUI to the subscription
pub enum SubscriberMessage {
    Connect(String),
    Disconnect,
    SendText(String, ChannelId, Option<u32>), // Optional reply to message id
    SendEmojiReply(String, ChannelId, u32),
    SendPosition(ChannelId, MCPosition),
    SendUser(ChannelId, MCUser),
    MeshTasticRadioPacket(Box<FromRadio>), // Sent from the radio to the subscription, not GUI
}

/// These are the messages that MeshChat responds to
#[derive(Debug, Clone)]
pub enum Message {
    Navigation(View),
    WindowEvent(Event),
    DeviceListViewEvent(DeviceListEvent),
    DeviceViewEvent(DeviceViewMessage),
    Exit,
    ConfigLoaded(Config),
    DeviceAndChannelChange(Option<String>, Option<ChannelId>),
    ShowLocation(MCPosition),
    ShowUserInfo(MCUser),
    CloseShowUser,
    OpenUrl(String),
    AppNotification(String, String, u32), // Message, detail, rx_time
    AppError(String, String, u32),        // Message, detail, rx_time
    RemoveNotification(usize),
    ToggleNodeFavourite(u32),
    CopyToClipBoard(String),
    AddNodeAlias(u32, String),
    RemoveNodeAlias(u32),
    AddDeviceAlias(String, String),
    RemoveDeviceAlias(String),
    Event(Event),
    OpenSettingsDialog,
    CloseSettingsDialog,
    ToggleShowPositionUpdates,
    ToggleShowUserUpdates,
    ToggleAutoReconnect,
    ToggleAutoUpdate,
    HistoryLengthSelected(HistoryLength),
    #[cfg(feature = "auto-update")]
    UpdateChecked(Result<Status, String>),
    None,
}

/// Check for a new release of MeshChat and update if available
#[cfg(feature = "auto-update")]
async fn check_for_update() -> Result<Status, String> {
    let mut update_builder = self_update::backends::github::Update::configure();

    let release_update = update_builder
        .repo_owner("andrewdavidmackenzie")
        .repo_name("meshchat")
        .bin_name("meshchat")
        .show_download_progress(true)
        .show_output(false)
        //.current_version(env!("CARGO_PKG_VERSION"))
        .current_version("0.2.0")
        .build()
        .map_err(|e| e.to_string())?;

    release_update.update().map_err(|e| e.to_string())
}

fn main() -> iced::Result {
    let mut window_settings = window::Settings::default();

    // Try and add an icon to the window::Settings
    let icon_bytes = include_bytes!("../assets/images/icon.ico");
    if let Ok(app_icon) = icon::from_file_data(icon_bytes, None) {
        window_settings.icon = Some(app_icon);
    }

    iced::application(MeshChat::new, MeshChat::update, MeshChat::view)
        .subscription(MeshChat::subscription)
        .exit_on_close_request(false)
        .resizable(true)
        .font(icons::FONT)
        .title(MeshChat::title)
        .window(window_settings)
        .run()
}

impl MeshChat {
    /// Create a new instance of the app and load the config asynchronously
    fn new() -> (Self, Task<Message>) {
        (Self::default(), load_config())
    }

    /// Get the current time in epoch as u32
    pub fn now() -> u32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32
    }

    /// Return the title of the app, which is used in the window title bar.
    /// Include the version number of the app and the number of unread messages, if any
    fn title(&self) -> String {
        let unread_count = self.device_view.unread_count(
            self.config.show_position_updates,
            self.config.show_user_updates,
        );
        if unread_count > 0 {
            format!("MeshChat {} ({} unread)", VERSION, unread_count)
        } else {
            format!("MeshChat {}", VERSION)
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
            AppNotification(summary, detail, rx_time) => self
                .notifications
                .add(Notification::Info(summary, detail, rx_time)),
            AppError(summary, detail, rx_time) => self
                .notifications
                .add(Notification::Error(summary, detail, rx_time)),
            Message::None => Task::none(),
            ConfigLoaded(config) => {
                self.device_view
                    .set_history_length(config.history_length.clone());
                self.device_view
                    .set_show_position_updates(config.show_position_updates);
                self.device_view
                    .set_show_user_updates(config.show_user_updates);

                self.config = config;

                let update_task = {
                    #[cfg(feature = "auto-update")]
                    if self.config.auto_update_startup {
                        Task::perform(check_for_update(), UpdateChecked)
                    } else {
                        Task::none()
                    }
                    #[cfg(not(feature = "auto-update"))]
                    Task::none()
                };

                // If the config requests to re-connect to a device, ask the device view to do so
                // optionally on a specific Node/Channel also
                let connect_task = {
                    if let Some(ble_device) = &self.config.ble_device
                        && self.config.auto_reconnect
                    {
                        self.device_view.update(DeviceViewMessage::ConnectRequest(
                            ble_device.clone(),
                            self.config.channel_id.clone(),
                        ))
                    } else {
                        Task::none()
                    }
                };

                Task::batch(vec![update_task, connect_task])
            }
            DeviceAndChannelChange(ble_device, channel) => {
                self.config.ble_device = ble_device;
                self.config.channel_id = channel;
                // and save it asynchronously, so that we don't block the GUI thread
                self.config.save_config()
            }
            RemoveNotification(id) => self.notifications.remove(id),
            ShowLocation(position) => {
                let _ = webbrowser::open(&Self::location_url(&position));
                Task::none()
            }
            OpenUrl(url) => {
                let _ = webbrowser::open(&url);
                Task::none()
            }
            ToggleNodeFavourite(node_id) => {
                // Toggle the favourite status of the node in the config
                if !self.config.fav_nodes.remove(&node_id) {
                    let _ = self.config.fav_nodes.insert(node_id);
                }
                // and save the config asynchronously, so that we don't block the GUI thread
                self.config.save_config()
            }
            CopyToClipBoard(string) => clipboard::write(string),
            AddNodeAlias(node_id, alias) => {
                self.device_view.stop_editing_alias();
                if !alias.is_empty() {
                    self.config.aliases.insert(node_id, alias);
                    self.config.save_config()
                } else {
                    Task::none()
                }
            }
            RemoveNodeAlias(node_id) => {
                self.config.aliases.remove(&node_id);
                self.config.save_config()
            }
            AddDeviceAlias(ble_device, alias) => {
                self.device_list_view.stop_editing_alias();
                if !alias.is_empty() {
                    self.config.device_aliases.insert(ble_device, alias);
                    self.config.save_config()
                } else {
                    Task::none()
                }
            }
            RemoveDeviceAlias(ble_device) => {
                self.config.device_aliases.remove(&ble_device);
                self.config.save_config()
            }
            Message::Event(event) => match event {
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Tab),
                    modifiers,
                    ..
                }) => {
                    if modifiers.shift() {
                        operation::focus_previous()
                    } else {
                        operation::focus_next()
                    }
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Escape),
                    ..
                }) => {
                    // Exit any interactive modes underway
                    self.showing_settings = false;
                    self.show_user = None;
                    self.device_view.cancel_interactive();
                    self.device_list_view.stop_editing_alias();
                    Task::none()
                }
                _ => Task::none(),
            },
            OpenSettingsDialog => {
                self.showing_settings = true;
                Task::none()
            }
            CloseSettingsDialog => {
                self.showing_settings = false;
                Task::none()
            }
            ToggleShowPositionUpdates => {
                self.config.show_position_updates = !self.config.show_position_updates;
                self.device_view
                    .set_show_position_updates(self.config.show_position_updates);
                self.config.save_config()
            }
            ToggleShowUserUpdates => {
                self.config.show_user_updates = !self.config.show_user_updates;
                self.device_view
                    .set_show_user_updates(self.config.show_user_updates);
                self.config.save_config()
            }
            ToggleAutoReconnect => {
                self.config.auto_reconnect = !self.config.auto_reconnect;
                self.config.save_config()
            }
            ToggleAutoUpdate => {
                self.config.auto_update_startup = !self.config.auto_update_startup;
                self.config.save_config()
            }
            HistoryLengthSelected(length) => {
                self.config.history_length = length.clone();
                self.config.save_config()
            }
            ShowUserInfo(user) => {
                self.show_user = Some(user);
                Task::none()
            }
            CloseShowUser => {
                self.show_user = None;
                Task::none()
            }
            #[cfg(feature = "auto-update")]
            UpdateChecked(result) => {
                match result {
                    Ok(Status::UpToDate(version)) => {
                        println!("Already up to date: `{}`", version)
                    }
                    Ok(Status::Updated(version)) => {
                        println!("Updated to version: `{}`", version)
                    }
                    Err(e) => eprintln!("Error updating: {:?}", e),
                }
                Task::none()
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
        let mut main_content_column = Column::new().push(header);

        // If busy of connecting or disconnecting, add a busy bar to the header
        if scanning || matches!(state, Connecting(_) | Disconnecting(_)) {
            main_content_column = main_content_column.push(Space::new().width(Fill)).push(
                Linear::new()
                    .easing(easing::emphasized_accelerate())
                    .cycle_duration(Duration::from_secs_f32(2.0))
                    .width(Fill),
            );
        }

        main_content_column = main_content_column
            .push(self.notifications.view())
            .push(inner);

        // add the notification area and the inner view
        if self.showing_settings {
            return Self::modal(main_content_column, self.config.view(), CloseSettingsDialog);
        }

        if let Some(user) = &self.show_user {
            return Self::modal(main_content_column, self.user(user), CloseShowUser);
        }

        main_content_column.into()
    }

    /// Create a view for a User
    fn user<'a>(&self, user: &MCUser) -> Element<'a, Message> {
        let settings_column = Column::new()
            .padding(8)
            .push(text(format!("ID: {}", user.id)))
            .push(text(format!("Long Name: {}", user.long_name)))
            .push(text(format!("Short Name: {}", user.short_name)))
            .push(text(format!("Hardware Model: {}", user.hw_model_str)))
            .push(text(format!("Licensed: {}", user.is_licensed)))
            .push(text(format!("Role: {}", user.role_str)))
            .push(text(format!("Public Key: {:X?}", user.public_key)))
            .push(text(format!("Unmessageable: {}", user.is_unmessagable)));

        let inner = Column::new()
            .spacing(8)
            .width(420)
            .push(
                container(
                    text("Node User Info")
                        .size(18)
                        .width(Fill)
                        .font(Font {
                            weight: Weight::Bold,
                            ..Default::default()
                        })
                        .align_x(Center),
                )
                .padding(12)
                .style(picker_header_style)
                .padding(4),
            )
            .push(settings_column);
        container(inner).style(tooltip_style).into()
    }

    /// Function to create a modal dialog in the middle of the screen
    pub fn modal<'a, Message>(
        base: impl Into<Element<'a, Message>>,
        content: impl Into<Element<'a, Message>>,
        on_blur: Message,
    ) -> Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        stack![
            base.into(),
            opaque(
                mouse_area(center(opaque(content)).style(|_theme| {
                    container::Style {
                        background: Some(
                            Color {
                                a: 0.8,
                                ..Color::BLACK
                            }
                            .into(),
                        ),
                        ..container::Style::default()
                    }
                }))
                .on_press(on_blur)
            )
        ]
        .into()
    }

    /// Convert a location tuple to a URL that can be opened in a browser.
    fn location_url(position: &MCPosition) -> String {
        format!(
            "https://maps.google.com/?q={:.7},{:.7}",
            position.latitude, position.longitude
        )
    }

    /// Subscribe to events from Discover and from Windows and from Devices (Radios)
    fn subscription(&self) -> Subscription<Message> {
        let subscriptions = vec![
            event::listen().map(WindowEvent),
            Subscription::run(ble_discovery).map(DeviceListViewEvent),
            Subscription::run(device_subscription::subscribe)
                .map(|m| DeviceViewEvent(SubscriptionMessage(m))),
            event::listen().map(Message::Event),
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
            if let Connected(_) = self.device_view.connection_state() {
                self.device_view.update(DisconnectRequest(true))
            } else {
                window::latest().and_then(window::close)
            }
        } else {
            Task::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_view_entry::MCMessage::NewTextMessage;
    use iced::keyboard::key::NativeCode::MacOS;
    use iced::keyboard::{Key, Location};

    #[test]
    fn test_location_url() {
        let position = MCPosition {
            latitude: 50.0,
            longitude: 1.0,
            altitude: None,
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: 0,
            timestamp_millis_adjust: 0,
            altitude_hae: None,
            altitude_geoidal_separation: None,
            pdop: 0,
            hdop: 0,
            vdop: 0,
            gps_accuracy: 0,
            ground_speed: None,
            ground_track: None,
            fix_quality: 0,
            fix_type: 0,
            sats_in_view: 0,
            sensor_id: 0,
            next_update: 0,
            seq_number: 0,
            precision_bits: 0,
        };

        assert_eq!(
            MeshChat::location_url(&position),
            "https://maps.google.com/?q=50.0000000,1.0000000"
        );
    }

    #[test]
    fn test_default_view() {
        let meshchat = test_helper::test_app();
        assert_eq!(meshchat.current_view, DeviceList);
    }

    #[test]
    fn navigate_to_device_view() {
        let mut meshchat = test_helper::test_app();
        let _ = meshchat.update(Navigation(View::Device(None)));
        assert_eq!(meshchat.current_view, View::Device(None));
    }

    #[test]
    fn show_settings() {
        let mut meshchat = test_helper::test_app();
        let _ = meshchat.update(OpenSettingsDialog);
        assert!(meshchat.showing_settings);
    }

    #[test]
    fn hide_settings() {
        let mut meshchat = test_helper::test_app();
        let _ = meshchat.update(OpenSettingsDialog);
        assert!(meshchat.showing_settings);
        let _ = meshchat.update(CloseSettingsDialog);
        assert!(!meshchat.showing_settings);
    }

    #[test]
    fn escape_hide_settings() {
        let mut meshchat = test_helper::test_app();
        let _ = meshchat.update(OpenSettingsDialog);
        assert!(meshchat.showing_settings);

        let key_event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::Escape),
            modified_key: Key::Unidentified,
            physical_key: key::Physical::Unidentified(MacOS(0)),
            location: Location::Standard,
            modifiers: Default::default(),
            text: None,
            repeat: false,
        });
        let _ = meshchat.update(Message::Event(key_event));
        assert!(!meshchat.showing_settings);
    }

    #[test]
    fn test_title_no_unreads() {
        let meshchat = test_helper::test_app();
        assert_eq!(
            meshchat.title(),
            format!("MeshChat {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn test_title_unreads() {
        let mut meshchat = test_helper::test_app();
        meshchat.new_message(NewTextMessage("Hello World".into()));
        assert_eq!(meshchat.title(), format!("MeshChat {} (1 unread)", VERSION));
    }
}
