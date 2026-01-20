//! MeshChat is an iced GUI app that uses the meshtastic "rust" crate to discover and control
//! meshtastic compatible radios connected to the host running it

use crate::Message::{
    AddDeviceAlias, AddNodeAlias, AppError, AppNotification, CloseSettingsDialog, CloseShowUser,
    ConfigChange, ConfigLoaded, CopyToClipBoard, DeviceListViewEvent, DeviceViewEvent, Exit,
    HistoryLengthSelected, Navigation, OpenSettingsDialog, OpenUrl, RemoveDeviceAlias,
    RemoveNodeAlias, RemoveNotification, ShowLocation, ShowUserInfo, ToggleAutoReconnect,
    ToggleAutoUpdate, ToggleNodeFavourite, ToggleShowPositionUpdates, ToggleShowUserUpdates,
    WindowEvent,
};
use crate::View::DeviceList;
use crate::channel_id::ChannelId;
use crate::config::{Config, HistoryLength, load_config};
use crate::device_list_view::{DeviceListEvent, DeviceListView};
use crate::device_view::ConnectionState::{Connected, Connecting, Disconnecting};
use crate::device_view::DeviceView;
use crate::device_view::DeviceViewMessage;
use crate::device_view::DeviceViewMessage::{DisconnectRequest, SubscriptionMessage};
use crate::discovery::ble_discovery;
use crate::linear::Linear;
use crate::notification::{Notification, Notifications};
use crate::styles::{button_chip_style, picker_header_style, tooltip_style};
use iced::font::Weight;
use iced::keyboard::key;
use iced::widget::{
    Column, Space, button, center, container, mouse_area, opaque, operation, stack, text, tooltip,
};
use iced::window::icon;
use iced::{Center, Color, Event, Font, Subscription, Task, clipboard, keyboard, window};
use iced::{Element, Fill, event};
use meshtastic::protobufs::User;
use meshtastic::utils::stream::BleDevice;
use self_update::Status;
use std::cmp::PartialEq;
use std::time::Duration;

mod battery;
mod channel_view;
mod channel_view_entry;
mod config;
mod device_list_view;
mod device_subscription;
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
mod notification;
#[cfg(test)]
mod test_helper;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    show_user: Option<User>,
}

#[derive(Debug, Clone)]
pub enum ConfigChangeMessage {
    DeviceAndChannel(Option<BleDevice>, Option<ChannelId>),
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
    ConfigChange(ConfigChangeMessage),
    ShowLocation(i32, i32), // lat and long / 1_000_000
    ShowUserInfo(User),
    CloseShowUser,
    OpenUrl(String),
    AppNotification(String, String),
    AppError(String, String),
    RemoveNotification(usize),
    ToggleNodeFavourite(u32),
    CopyToClipBoard(String),
    AddNodeAlias(u32, String),
    RemoveNodeAlias(u32),
    AddDeviceAlias(BleDevice, String),
    RemoveDeviceAlias(BleDevice),
    Event(Event),
    OpenSettingsDialog,
    CloseSettingsDialog,
    ToggleShowPositionUpdates,
    ToggleShowUserUpdates,
    ToggleAutoReconnect,
    ToggleAutoUpdate,
    HistoryLengthSelected(HistoryLength),
    None,
}

fn update() -> self_update::errors::Result<Status> {
    let mut update_builder = self_update::backends::github::Update::configure();

    let release_update = update_builder
        .repo_owner("andrewdavidmackenzie")
        .repo_name("meshchat")
        .bin_name("meshchat")
        .show_download_progress(true)
        .show_output(false)
        //.current_version(env!("CARGO_PKG_VERSION"))
        .current_version("0.2.0")
        .build()?;

    release_update.update()
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

    /// Return the title of the app, which is used in the window title bar.
    /// Include the version number of the app and the number of unread messages, if any
    fn title(&self) -> String {
        let unread_count = self.device_view.unread_count();
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
            AppNotification(summary, detail) => {
                self.notifications.add(Notification::Info(summary, detail))
            }
            AppError(summary, detail) => {
                self.notifications.add(Notification::Error(summary, detail))
            }
            Message::None => Task::none(),
            ConfigLoaded(config) => {
                self.device_view
                    .set_history_length(config.history_length.clone());
                self.device_view
                    .set_show_position_updates(config.show_position_updates);
                self.config = config;

                // TODO put into a task
                if self.config.auto_update_startup {
                    match update() {
                        Ok(Status::UpToDate(version)) => {
                            println!("Already up to date: `{}`", version)
                        }
                        Ok(Status::Updated(version)) => {
                            println!("Updated to version: `{}`", version)
                        }
                        Err(e) => eprintln!("Error updating: {:?}", e),
                    }
                }

                // If the config requests to re-connect to a device, ask the device view to do so
                // optionally on a specific Node/Channel also
                if let Some(ble_device) = &self.config.ble_device
                    && !self.config.disable_auto_reconnect
                {
                    self.device_view.update(DeviceViewMessage::ConnectRequest(
                        ble_device.clone(),
                        self.config.channel_id.clone(),
                    ))
                } else {
                    Task::none()
                }
            }
            ConfigChange(config_change) => {
                // Merge in what has changed
                match config_change {
                    ConfigChangeMessage::DeviceAndChannel(ble_device, channel) => {
                        self.config.ble_device = ble_device;
                        self.config.channel_id = channel;
                    }
                }
                // and save it asynchronously, so that we don't block the GUI thread
                self.config.save_config()
            }
            RemoveNotification(id) => self.notifications.remove(id),
            ShowLocation(lat, long) => {
                let _ = webbrowser::open(&Self::location_url(lat, long));
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
                    let device_string = ble_device
                        .name
                        .unwrap_or(ble_device.mac_address.to_string());
                    self.config.device_aliases.insert(device_string, alias);
                    self.config.save_config()
                } else {
                    Task::none()
                }
            }
            RemoveDeviceAlias(ble_device) => {
                let device_string = ble_device
                    .name
                    .unwrap_or(ble_device.mac_address.to_string());
                self.config.device_aliases.remove(&device_string);
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
            // TODO unify these with ConfigChange event above
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
                self.config.disable_auto_reconnect = !self.config.disable_auto_reconnect;
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
    fn user<'a>(&self, user: &User) -> Element<'a, Message> {
        let settings_column = Column::new()
            .padding(8)
            .push(text(format!("ID: {}", user.id)))
            .push(text(format!("Long Name: {}", user.long_name)))
            .push(text(format!("Short Name: {}", user.short_name)))
            .push(text(format!(
                "Hardware Model: {}",
                user.hw_model().as_str_name()
            )))
            .push(text(format!("Licensed: {}", user.is_licensed)))
            .push(text(format!("Role: {}", user.role().as_str_name())))
            .push(text(format!("Public Key: {:X?}", user.public_key)))
            .push(text(format!("Unmessageable: {}", user.is_unmessagable())));

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

    /// Generate a Settings button
    pub fn settings_button<'a>() -> Element<'a, Message> {
        tooltip(
            button(icons::cog())
                .style(button_chip_style)
                .on_press(OpenSettingsDialog),
            "Open Settings Dialog",
            tooltip::Position::Bottom,
        )
        .style(tooltip_style)
        .into()
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
    fn location_url(lat: i32, long: i32) -> String {
        let latitude = 0.0000001 * (lat as f64);
        let longitude = 0.0000001 * (long as f64);
        format!(
            "https://maps.google.com/?q={:.7},{:.7}",
            latitude, longitude
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
    use crate::channel_view_entry::Payload::NewTextMessage;
    use iced::keyboard::key::NativeCode::MacOS;
    use iced::keyboard::{Key, Location};

    #[test]
    fn test_location_url() {
        assert_eq!(
            MeshChat::location_url(50, 1),
            "https://maps.google.com/?q=0.0000050,0.0000001"
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
