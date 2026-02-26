#[cfg(feature = "auto-update")]
use crate::Message::UpdateChecked;
use crate::Message::{
    AddDeviceAlias, AddNodeAlias, AppError, AppNotification, CloseSettingsDialog, CloseShowUser,
    ConfigLoaded, CopyToClipBoard, CriticalAppError, DeviceAndChannelConfigChange,
    DeviceListViewEvent, DeviceViewEvent, Exit, HistoryLengthSelected, Navigation,
    OpenSettingsDialog, OpenUrl, RemoveDeviceAlias, RemoveNodeAlias, RemoveNotification,
    SetWindowPosition, SetWindowSize, ShowLocation, ShowUserInfo, ToggleAutoReconnect,
    ToggleAutoUpdate, ToggleNodeFavourite, ToggleSaveWindowPosition, ToggleSaveWindowSize,
    ToggleShowPositionUpdates, ToggleShowUserUpdates,
};
use crate::config::{Config, HistoryLength, load_config};
use crate::conversation_id::{ConversationId, NodeId};
use crate::device::ConnectionState::Connected;
use crate::device::DeviceViewMessage;
use crate::device::DeviceViewMessage::DisconnectRequest;
#[cfg(any(feature = "meshtastic", feature = "meshcore"))]
use crate::device::DeviceViewMessage::SubscriptionMessage;
use crate::device::{Device, TimeStamp};
use crate::device_list::{DeviceList, DeviceListEvent, RadioType};
use crate::discovery::ble_discovery;
use crate::notification::{Notification, Notifications};
use crate::styles::{modal_style, picker_header_style, tooltip_style};
use crate::{meshc, mesht};
use iced::font::Weight;
use iced::keyboard::key;
use iced::widget::{Column, center, container, mouse_area, opaque, operation, stack, text};
use iced::{Center, Event, Font, Point, Size, Subscription, Task, clipboard, keyboard, window};
use iced::{Element, Fill, event};
#[cfg(feature = "auto-update")]
use self_update::Status;
use std::cmp::PartialEq;
use std::fmt;
use std::fmt::Formatter;
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A User as represented in the App, maybe a superset of User attributes from different meshes
#[derive(Debug, Clone, Default, PartialEq)]
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
#[derive(Clone, Debug, Default)]
pub struct MCNodeInfo {
    pub node_id: NodeId,
    pub user: Option<MCUser>,
    pub position: Option<MCPosition>,
    pub is_ignored: bool,
}

#[derive(Clone, Debug, Default)]
pub struct MCPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<i32>,
    pub time: u32,
    pub location_source: i32,
    pub altitude_source: i32,
    pub timestamp: TimeStamp,
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
    DeviceListView,
    DeviceView(Option<ConversationId>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MCChannel {
    pub index: i32,
    pub name: String,
}

/// These are the messages that MeshChat responds to
#[derive(Debug, Clone)]
pub enum Message {
    Navigation(View),
    DeviceListViewEvent(DeviceListEvent),
    DeviceViewEvent(DeviceViewMessage),
    Exit,
    ConfigLoaded(Config),
    DeviceAndChannelConfigChange(Option<(String, RadioType)>, Option<ConversationId>),
    ShowLocation(MCPosition),
    ShowUserInfo(MCUser),
    CloseShowUser,
    OpenUrl(String),
    AppNotification(String, String, TimeStamp), // Message, detail, TimeStamp
    AppError(String, String, TimeStamp),        // Message, detail, TimeStamp
    CriticalAppError(String, String, TimeStamp), // Message, detail, TimeStamp
    RemoveNotification(usize),
    ToggleNodeFavourite(NodeId),
    CopyToClipBoard(String),
    AddNodeAlias(NodeId, String),
    RemoveNodeAlias(NodeId),
    AddDeviceAlias(String, String),
    RemoveDeviceAlias(String),
    Event(Event),
    OpenSettingsDialog,
    CloseSettingsDialog,
    ToggleShowPositionUpdates,
    ToggleShowUserUpdates,
    ToggleAutoReconnect,
    ToggleAutoUpdate,
    ToggleSaveWindowSize,
    SetWindowSize(Size),
    SetWindowPosition(Option<Point>),
    ToggleSaveWindowPosition,
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

#[derive(Default)]
pub struct MeshChat {
    config: Config,
    current_view: View,
    device_list: DeviceList,
    pub(crate) device: Device,
    notifications: Notifications,
    showing_settings: bool,
    show_user: Option<MCUser>,
}

impl MeshChat {
    /// Create a new instance of the app and load the config asynchronously
    pub fn new() -> (Self, Task<Message>) {
        (Self::default(), load_config())
    }

    /// Get the current time in epoch as u32
    pub fn now() -> TimeStamp {
        TimeStamp::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as u32,
        )
    }

    /// Return the title of the app, which is used in the window title bar.
    /// Include the version number of the app and the number of unread messages, if any
    pub(crate) fn title(&self) -> String {
        let unread_count = self.device.unread_count(
            self.config.show_position_updates,
            self.config.show_user_updates,
        );
        if unread_count > 0 {
            format!("MeshChat {} ({} unread)", VERSION, unread_count)
        } else {
            format!("MeshChat {}", VERSION)
        }
    }

    fn window_size_change_request(size: Size) -> Task<Message> {
        window::latest().then(move |latest| {
            if let Some(id) = latest {
                window::resize(id, size)
            } else {
                Task::none()
            }
        })
    }

    fn window_position_change_request(position: Point) -> Task<Message> {
        window::latest().then(move |latest| {
            if let Some(id) = latest {
                window::move_to(id, position)
            } else {
                Task::none()
            }
        })
    }

    /// Update the app state based on a message received from the GUI.
    /// This is the main function of the app, and it drives the GUI.
    /// It is called every time a message is received from the GUI, and it updates the app state
    /// accordingly.
    /// It also handles window events like "minimise" and close buttons, and it handles navigation
    /// between views.
    /// It also handles the discovery of devices and the connection to them.
    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Navigation(view) => self.navigate(view),
            DeviceListViewEvent(device_list_event) => self.device_list.update(device_list_event),
            DeviceViewEvent(device_event) => self.device.update(device_event),
            Exit => window::latest().and_then(window::close),
            AppNotification(summary, detail, timestamp) => self
                .notifications
                .add(Notification::Info(summary, detail, timestamp)),
            AppError(summary, detail, timestamp) => self
                .notifications
                .add(Notification::Error(summary, detail, timestamp)),
            CriticalAppError(summary, detail, timestamp) => self
                .notifications
                .add(Notification::Critical(summary, detail, timestamp)),
            Message::None => Task::none(),
            ConfigLoaded(config) => {
                self.device.set_history_length(config.history_length);
                self.device
                    .set_show_position_updates(config.show_position_updates);
                self.device.set_show_user_updates(config.show_user_updates);

                self.config = config;

                let mut tasks = vec![];
                #[cfg(feature = "auto-update")]
                if self.config.auto_update_startup {
                    tasks.push(Task::perform(check_for_update(), UpdateChecked));
                }

                // If the config requests to re-connect to a device, ask the device view to do so
                // optionally on a specific Node/Channel also
                if let Some((ble_device_name, radio_type)) = &self.config.ble_device
                    && self.config.auto_reconnect
                {
                    tasks.push(self.device.update(DeviceViewMessage::ConnectRequest(
                        ble_device_name.clone(),
                        *radio_type,
                        self.config.conversation_id,
                    )))
                }

                if self.config.restore_window_size
                    && let Some(window_size) = &self.config.window_size
                {
                    tasks.push(Self::window_size_change_request(window_size.size()));
                }

                if self.config.restore_window_position
                    && let Some(window_position) = &self.config.window_position
                {
                    tasks.push(Self::window_position_change_request(
                        window_position.point(),
                    ));
                }

                Task::batch(tasks)
            }
            DeviceAndChannelConfigChange(ble_device, channel) => {
                self.config.ble_device = ble_device;
                self.config.conversation_id = channel;
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
                self.device.stop_editing_alias();
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
                self.device_list.stop_editing_alias();
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
            Message::Event(event) => self.process_event(event),
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
                self.device
                    .set_show_position_updates(self.config.show_position_updates);
                self.config.save_config()
            }
            ToggleShowUserUpdates => {
                self.config.show_user_updates = !self.config.show_user_updates;
                self.device
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
            ToggleSaveWindowSize => {
                self.config.restore_window_size = !self.config.restore_window_size;
                if self.config.restore_window_size {
                    window::latest().and_then(window::size).map(SetWindowSize)
                } else {
                    self.config.window_size = None;
                    self.config.save_config()
                }
            }
            ToggleSaveWindowPosition => {
                self.config.restore_window_position = !self.config.restore_window_position;
                if self.config.restore_window_position {
                    window::latest()
                        .and_then(window::position)
                        .map(SetWindowPosition)
                } else {
                    self.config.window_position = None;
                    self.config.save_config()
                }
            }
            HistoryLengthSelected(length) => {
                self.config.history_length = length;
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
            SetWindowSize(size) => {
                if self.config.restore_window_size {
                    self.config.window_size = Some(size.into());
                }
                Task::none()
            }
            SetWindowPosition(position) => {
                if self.config.restore_window_position {
                    self.config.window_position = position.map(Into::into);
                }
                Task::none()
            }
        }
    }

    fn process_event(&mut self, event: Event) -> Task<Message> {
        match event {
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
                self.device.cancel_interactive();
                self.device_list.stop_editing_alias();
                Task::none()
            }
            Event::Window(window::Event::Moved(point)) => {
                if self.config.restore_window_position {
                    self.config.window_position = Some(point.into());
                    self.config.save_config()
                } else {
                    Task::none()
                }
            }
            Event::Window(window::Event::Resized(size)) => {
                if self.config.restore_window_size {
                    self.config.window_size = Some(size.into());
                    self.config.save_config()
                } else {
                    Task::none()
                }
            }
            Event::Window(window::Event::CloseRequested) => {
                if let Connected(_, _) = self.device.connection_state() {
                    self.device.update(DisconnectRequest(true))
                } else {
                    window::latest().and_then(window::close)
                }
            }

            _ => Task::none(),
        }
    }

    /// Render the main app view
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let state = self.device.connection_state();

        let header_view = match self.current_view {
            View::DeviceListView => self.device_list.header(&self.config, state),
            View::DeviceView(_) => self.device.header(&self.config, state, &self.device_list),
        };

        // Build the inner view and show busy if in DeviceList which is in discovery mode
        let inner_view = match &self.current_view {
            View::DeviceListView => self.device_list.view(&self.config, state),
            View::DeviceView(_) => self.device.view(&self.config),
        };

        // Create the stack of elements, starting with the header
        let main_content_column = Column::new()
            .push(header_view)
            .push(self.notifications.view())
            .push(inner_view);

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

    /// Function to create a modal dialogue in the middle of the screen
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
            opaque(mouse_area(center(opaque(content)).style(modal_style)).on_press(on_blur))
        ]
        .into()
    }

    /// Convert a location tuple to a URL that can be opened in a browser.
    pub fn location_url(position: &MCPosition) -> String {
        format!(
            "https://maps.google.com/?q={:.7},{:.7}",
            position.latitude, position.longitude
        )
    }

    /// Subscribe to events from Discover and from Windows and from Devices (Radios)
    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let subscriptions = vec![
            Subscription::run(ble_discovery).map(DeviceListViewEvent),
            #[cfg(feature = "meshtastic")]
            Subscription::run(mesht::subscription::subscribe)
                .map(|m| DeviceViewEvent(SubscriptionMessage(m))),
            #[cfg(feature = "meshcore")]
            Subscription::run(meshc::subscription::subscribe)
                .map(|m| DeviceViewEvent(SubscriptionMessage(m))),
            event::listen().map(Message::Event),
        ];

        Subscription::batch(subscriptions)
    }

    /// Navigate to show a different view, as defined by the [View] enum
    fn navigate(&mut self, view: View) -> Task<Message> {
        self.current_view = view;
        match &self.current_view {
            View::DeviceListView => Task::none(),
            View::DeviceView(conversation_id) => self
                .device
                .update(DeviceViewMessage::ShowChannel(*conversation_id)),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::conversation_id::MessageId;
    use crate::device::SubscriptionEvent::MyNodeNum;
    use crate::meshchat::View::DeviceListView;
    use crate::message;
    use crate::message::MCContent;
    use crate::message::MCContent::NewTextMessage;
    use iced::keyboard::key::NativeCode::MacOS;
    use iced::keyboard::{Key, Location};
    use std::sync::atomic::{AtomicU32, Ordering};

    static MESSAGE_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

    impl MeshChat {
        pub fn new_message(&mut self, msg: MCContent) {
            let message_id = MESSAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
            let mcmessage = message::MCMessage::new(
                MessageId::from(message_id),
                NodeId::from(1u64),
                msg,
                MeshChat::now(),
            );
            let _ = self
                .device
                .new_message(&ConversationId::Channel(0.into()), mcmessage);
        }
    }

    pub fn test_app() -> MeshChat {
        let mut meshchat = MeshChat::default();
        let mut device = Device::default();
        let _ = device.update(SubscriptionMessage(MyNodeNum(NodeId::from(999u64))));
        device.add_channel(MCChannel {
            index: 0,
            name: "Test".to_string(),
        });

        meshchat.device = device;

        meshchat
    }

    #[test]
    fn test_location_url() {
        let position = MCPosition {
            latitude: 50.0,
            longitude: 1.0,
            altitude: None,
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0),
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
        let meshchat = test_app();
        assert_eq!(meshchat.current_view, DeviceListView);
    }

    #[test]
    fn navigate_to_device_view() {
        let mut meshchat = test_app();
        let _ = meshchat.update(Navigation(View::DeviceView(None)));
        assert_eq!(meshchat.current_view, View::DeviceView(None));
    }

    #[test]
    fn show_settings() {
        let mut meshchat = test_app();
        let _ = meshchat.update(OpenSettingsDialog);
        assert!(meshchat.showing_settings);
    }

    #[test]
    fn hide_settings() {
        let mut meshchat = test_app();
        let _ = meshchat.update(OpenSettingsDialog);
        assert!(meshchat.showing_settings);
        let _ = meshchat.update(CloseSettingsDialog);
        assert!(!meshchat.showing_settings);
    }

    #[test]
    fn escape_hide_settings() {
        let mut meshchat = test_app();
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
        let meshchat = test_app();
        assert_eq!(
            meshchat.title(),
            format!("MeshChat {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn test_title_unreads() {
        let mut meshchat = test_app();
        meshchat.new_message(NewTextMessage("Hello World".into()));
        assert_eq!(meshchat.title(), format!("MeshChat {} (1 unread)", VERSION));
    }

    #[test]
    fn test_title_multiple_unreads() {
        let mut meshchat = test_app();
        meshchat.new_message(NewTextMessage("Message 1".into()));
        meshchat.new_message(NewTextMessage("Message 2".into()));
        meshchat.new_message(NewTextMessage("Message 3".into()));
        assert_eq!(meshchat.title(), format!("MeshChat {} (3 unread)", VERSION));
    }

    #[test]
    fn test_view_default() {
        assert_eq!(View::default(), DeviceListView);
    }

    #[test]
    fn test_view_equality() {
        assert_eq!(View::DeviceView(None), View::DeviceView(None));
        assert_eq!(
            View::DeviceView(Some(ConversationId::Channel(0.into()))),
            View::DeviceView(Some(ConversationId::Channel(0.into())))
        );
        assert_ne!(DeviceListView, View::DeviceView(None));
    }

    #[test]
    fn test_mc_user_display() {
        let user = MCUser {
            id: "!abc123".to_string(),
            long_name: "Test User".to_string(),
            short_name: "TEST".to_string(),
            hw_model_str: "TBEAM".to_string(),
            hw_model: 0,
            is_licensed: false,
            role_str: "CLIENT".to_string(),
            role: 0,
            public_key: vec![],
            is_unmessagable: false,
        };

        let display = format!("{}", user);
        assert!(display.contains("Test User"));
        assert!(display.contains("TEST"));
        assert!(display.contains("!abc123"));
        assert!(display.contains("TBEAM"));
    }

    #[test]
    fn test_mc_position_display() {
        let position = MCPosition {
            latitude: 51.5074,
            longitude: -0.1278,
            altitude: Some(11),
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0),
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

        let display = format!("{}", position);
        assert!(display.contains("51.51"));
        assert!(display.contains("-0.13"));
        assert!(display.contains("📌"));
    }

    #[test]
    fn test_location_url_negative_coordinates() {
        let position = MCPosition {
            latitude: -33.8688,
            longitude: 151.2093,
            altitude: None,
            time: 0,
            location_source: 0,
            altitude_source: 0,
            timestamp: TimeStamp::from(0),
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

        let url = MeshChat::location_url(&position);
        assert!(url.contains("-33.8688"));
        assert!(url.contains("151.2093"));
        assert!(url.starts_with("https://maps.google.com"));
    }

    #[test]
    fn test_now_returns_valid_timestamp() {
        let now = MeshChat::now();
        // Should be a reasonable recent timestamp (after 2020)
        assert!(now > TimeStamp::from(1577836800)); // Jan 1, 2020
    }

    #[test]
    fn test_toggle_node_favourite() {
        let mut meshchat = test_app();
        assert!(!meshchat.config.fav_nodes.contains(&NodeId::from(12345u64)));

        // Toggle on
        let _ = meshchat.update(ToggleNodeFavourite(NodeId::from(12345u64)));
        assert!(meshchat.config.fav_nodes.contains(&NodeId::from(12345u64)));

        // Toggle off
        let _ = meshchat.update(ToggleNodeFavourite(NodeId::from(12345u64)));
        assert!(!meshchat.config.fav_nodes.contains(&NodeId::from(12345u64)));
    }

    #[test]
    fn test_add_node_alias() {
        let mut meshchat = test_app();
        assert!(meshchat.config.aliases.is_empty());

        let _ = meshchat.update(AddNodeAlias(NodeId::from(123u64), "My Friend".to_string()));
        assert_eq!(
            meshchat.config.aliases.get(&NodeId::from(123_u64)),
            Some(&"My Friend".to_string())
        );
    }

    #[test]
    fn test_add_empty_node_alias() {
        let mut meshchat = test_app();
        let _ = meshchat.update(AddNodeAlias(NodeId::from(123u64), "".to_string()));
        // Empty alias should not be added
        assert!(!meshchat.config.aliases.contains_key(&NodeId::from(123u64)));
    }

    #[test]
    fn test_remove_node_alias() {
        let mut meshchat = test_app();
        meshchat
            .config
            .aliases
            .insert(NodeId::from(123u64), "Friend".to_string());

        let _ = meshchat.update(RemoveNodeAlias(NodeId::from(123u64)));
        assert!(!meshchat.config.aliases.contains_key(&NodeId::from(123u64)));
    }

    #[test]
    fn test_add_device_alias() {
        let mut meshchat = test_app();
        assert!(meshchat.config.device_aliases.is_empty());

        let _ = meshchat.update(AddDeviceAlias(
            "AA:BB:CC:DD:EE:FF".to_string(),
            "My Radio".to_string(),
        ));
        assert_eq!(
            meshchat.config.device_aliases.get("AA:BB:CC:DD:EE:FF"),
            Some(&"My Radio".to_string())
        );
    }

    #[test]
    fn test_remove_device_alias() {
        let mut meshchat = test_app();
        meshchat
            .config
            .device_aliases
            .insert("AA:BB:CC:DD:EE:FF".to_string(), "Radio".to_string());

        let _ = meshchat.update(RemoveDeviceAlias("AA:BB:CC:DD:EE:FF".to_string()));
        assert!(
            !meshchat
                .config
                .device_aliases
                .contains_key("AA:BB:CC:DD:EE:FF")
        );
    }

    #[test]
    fn test_toggle_show_position_updates() {
        let mut meshchat = test_app();
        let initial = meshchat.config.show_position_updates;

        let _ = meshchat.update(ToggleShowPositionUpdates);
        assert_eq!(meshchat.config.show_position_updates, !initial);

        let _ = meshchat.update(ToggleShowPositionUpdates);
        assert_eq!(meshchat.config.show_position_updates, initial);
    }

    #[test]
    fn test_toggle_show_user_updates() {
        let mut meshchat = test_app();
        let initial = meshchat.config.show_user_updates;

        let _ = meshchat.update(ToggleShowUserUpdates);
        assert_eq!(meshchat.config.show_user_updates, !initial);
    }

    #[test]
    fn test_toggle_auto_reconnect() {
        let mut meshchat = test_app();
        let initial = meshchat.config.auto_reconnect;

        let _ = meshchat.update(ToggleAutoReconnect);
        assert_eq!(meshchat.config.auto_reconnect, !initial);
    }

    #[test]
    fn test_toggle_auto_update() {
        let mut meshchat = test_app();
        let initial = meshchat.config.auto_update_startup;

        let _ = meshchat.update(ToggleAutoUpdate);
        assert_eq!(meshchat.config.auto_update_startup, !initial);
    }

    #[test]
    fn test_history_length_selected() {
        use crate::config::HistoryLength;

        let mut meshchat = test_app();
        let _ = meshchat.update(HistoryLengthSelected(HistoryLength::NumberOfMessages(50)));
        assert_eq!(
            meshchat.config.history_length,
            HistoryLength::NumberOfMessages(50)
        );
    }

    #[test]
    fn test_show_user_info() {
        let mut meshchat = test_app();
        assert!(meshchat.show_user.is_none());

        let user = MCUser {
            id: "test".to_string(),
            long_name: "Test".to_string(),
            short_name: "T".to_string(),
            hw_model_str: "TBEAM".to_string(),
            hw_model: 0,
            is_licensed: false,
            role_str: "CLIENT".to_string(),
            role: 0,
            public_key: vec![],
            is_unmessagable: false,
        };

        let _ = meshchat.update(ShowUserInfo(user));
        assert!(meshchat.show_user.is_some());
    }

    #[test]
    fn test_close_show_user() {
        let mut meshchat = test_app();
        meshchat.show_user = Some(MCUser {
            id: "test".to_string(),
            long_name: "Test".to_string(),
            short_name: "T".to_string(),
            hw_model_str: "TBEAM".to_string(),
            hw_model: 0,
            is_licensed: false,
            role_str: "CLIENT".to_string(),
            role: 0,
            public_key: vec![],
            is_unmessagable: false,
        });

        let _ = meshchat.update(CloseShowUser);
        assert!(meshchat.show_user.is_none());
    }

    #[test]
    fn test_none_message_does_nothing() {
        let mut meshchat = test_app();
        let initial_view = meshchat.current_view.clone();
        let _ = meshchat.update(Message::None);
        assert_eq!(meshchat.current_view, initial_view);
    }
}
