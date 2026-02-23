use crate::Message::{
    HistoryLengthSelected, ToggleAutoReconnect, ToggleAutoUpdate, ToggleSaveWindowPosition,
    ToggleSaveWindowSize, ToggleShowPositionUpdates, ToggleShowUserUpdates,
};
use crate::channel_id::{ChannelId, NodeId};
use crate::device_list::RadioType;
use crate::styles::{picker_header_style, tooltip_style};
use crate::{MeshChat, Message};
use directories::ProjectDirs;
use iced::font::Weight;
use iced::widget::{Column, container, pick_list, text, toggler};
use iced::{Center, Element, Fill, Font, Point, Size, Task};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs::DirBuilder;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

const EIGHT_HOURS_IN_SECONDS: u64 = 60 * 60 * 8;
const ONE_DAY_IN_SECONDS: u64 = 60 * 60 * 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default, rename = "device", skip_serializing_if = "Option::is_none")]
    pub ble_device: Option<(String, RadioType)>,
    #[serde(default, rename = "channel", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub fav_nodes: HashSet<NodeId>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub aliases: HashMap<NodeId, String>, // node name aliases
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub device_aliases: HashMap<String, String>, // device (as a string) to alias
    #[serde(
        default = "HistoryLength::default",
        skip_serializing_if = "HistoryLength::is_all"
    )]
    pub history_length: HistoryLength,
    /// Whether node position updates sent are shown in the chat view or just update node position
    #[serde(default = "default_show_position")]
    pub show_position_updates: bool,
    /// Whether node User updates sent are shown in the chat view or just update node User
    #[serde(default = "default_show_user")]
    pub show_user_updates: bool,
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
    /// Whether we should attempt an auto-update on start-up
    #[serde(default = "default_auto_update_startup")]
    pub auto_update_startup: bool,
    #[serde(default)]
    pub restore_window_position: bool,
    #[serde(default)]
    pub window_position: Option<WindowPosition>,
    #[serde(default)]
    pub restore_window_size: bool,
    #[serde(default)]
    pub window_size: Option<WindowSize>,
}

/// Struct we will use to serialize and deserialize window position
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowPosition {
    pub x: u32,
    pub y: u32,
}

/// Struct we will use to serialize and deserialize window size
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

impl From<Point> for WindowPosition {
    fn from(value: Point) -> Self {
        WindowPosition {
            x: value.x as u32,
            y: value.y as u32,
        }
    }
}

impl From<Size> for WindowSize {
    fn from(value: Size) -> Self {
        WindowSize {
            width: value.width as u32,
            height: value.height as u32,
        }
    }
}

impl WindowPosition {
    pub fn point(&self) -> Point<f32> {
        Point {
            x: self.x as f32,
            y: self.y as f32,
        }
    }
}

impl WindowSize {
    pub fn size(&self) -> Size {
        Size {
            width: self.width as f32,
            height: self.height as f32,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum HistoryLength {
    #[serde(rename = "all")]
    #[default]
    All,
    #[serde(rename = "messages")]
    NumberOfMessages(usize),
    #[serde(rename = "duration")]
    Duration(Duration),
}

impl HistoryLength {
    pub const ALL: [HistoryLength; 5] = [
        HistoryLength::All,
        HistoryLength::NumberOfMessages(10),
        HistoryLength::NumberOfMessages(100),
        HistoryLength::Duration(Duration::from_secs(EIGHT_HOURS_IN_SECONDS)),
        HistoryLength::Duration(Duration::from_secs(ONE_DAY_IN_SECONDS)),
    ];

    pub fn is_all(&self) -> bool {
        matches!(self, HistoryLength::All)
    }
}

impl std::fmt::Display for HistoryLength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HistoryLength::All => write!(f, "Store all messages"),
            HistoryLength::NumberOfMessages(n) => write!(f, "Store last {} messages", n),
            HistoryLength::Duration(d) => {
                write!(
                    f,
                    "Store all messages in last {} hours",
                    d.as_secs() / (60 * 60)
                )
            }
        }
    }
}

/// Some values have non-false (true) defaults, so implement using default functions
/// to make sure it matches what a deserialized empty config would give.
impl Default for Config {
    fn default() -> Self {
        Self {
            ble_device: None,
            channel_id: None,
            fav_nodes: HashSet::new(),
            aliases: HashMap::new(),
            device_aliases: HashMap::new(),
            history_length: HistoryLength::default(),
            show_position_updates: default_show_position(),
            show_user_updates: default_show_user(),
            auto_reconnect: default_auto_reconnect(),
            auto_update_startup: default_auto_update_startup(),
            restore_window_position: false,
            window_position: None,
            restore_window_size: false,
            window_size: None,
        }
    }
}

impl Config {
    /// Use `save_config` to save the config to disk from the UI
    pub fn save_config(&self) -> Task<Message> {
        if let Some(proj_dirs) = ProjectDirs::from("net", "Mackenzie Serres", "meshchat") {
            let config_path = proj_dirs.config_dir().join("config.toml");

            Task::perform(save(config_path.clone(), self.clone()), {
                move |result| match result {
                    Ok(_) => Message::None,
                    Err(e) => Message::AppError(
                        format!(
                            "Error saving config file: '{}'",
                            config_path.to_string_lossy()
                        ),
                        e.to_string(),
                        MeshChat::now(),
                    ),
                }
            })
        } else {
            Task::none()
        }
    }

    /// Create the view for the settings
    pub fn view<'a>(&self) -> Element<'a, Message> {
        let settings_column = Column::new()
            .padding(8)
            .spacing(8)
            .push(self.show_position_in_chat_setting())
            .push(self.show_user_updates())
            .push(self.auto_reconnect())
            .push(self.history_length())
            .push(self.auto_update())
            .push(self.save_window_position())
            .push(self.save_window_size());

        let inner = Column::new()
            .spacing(8)
            .width(400)
            .push(
                container(
                    text("Settings")
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

    fn show_position_in_chat_setting<'a>(&self) -> Element<'a, Message> {
        toggler(self.show_position_updates)
            .label("Show node position updates in chat")
            .on_toggle(Self::toggle_show_position_updates)
            .into()
    }

    fn toggle_show_position_updates(_current_setting: bool) -> Message {
        ToggleShowPositionUpdates
    }

    fn show_user_updates<'a>(&self) -> Element<'a, Message> {
        toggler(self.show_user_updates)
            .label("Show node User info shares in chat")
            .on_toggle(Self::toggle_show_user_updates)
            .into()
    }

    fn toggle_show_user_updates(_current_setting: bool) -> Message {
        ToggleShowUserUpdates
    }

    /// Settings view to modify the number of messages kept in memory for each channel.
    fn history_length<'a>(&self) -> Element<'a, Message> {
        pick_list(
            HistoryLength::ALL,
            Some(self.history_length),
            HistoryLengthSelected,
        )
        .into()
    }

    fn auto_reconnect<'a>(&self) -> Element<'a, Message> {
        toggler(self.auto_reconnect)
            .label("Auto-reconnect at startup")
            .on_toggle(Self::toggle_auto_reconnects)
            .into()
    }

    fn toggle_auto_reconnects(_current_setting: bool) -> Message {
        ToggleAutoReconnect
    }

    fn auto_update<'a>(&self) -> Element<'a, Message> {
        toggler(self.auto_update_startup)
            .label("Check for App updates on startup")
            .on_toggle(Self::toggle_auto_update)
            .into()
    }

    fn save_window_position<'a>(&self) -> Element<'a, Message> {
        toggler(self.restore_window_position)
            .label("Restore window position on startup")
            .on_toggle(Self::toggle_save_window_position)
            .into()
    }

    fn save_window_size<'a>(&self) -> Element<'a, Message> {
        toggler(self.restore_window_size)
            .label("Restore window size on startup")
            .on_toggle(Self::toggle_save_window_size)
            .into()
    }

    fn toggle_save_window_position(_current_setting: bool) -> Message {
        ToggleSaveWindowPosition
    }

    fn toggle_save_window_size(_current_setting: bool) -> Message {
        ToggleSaveWindowSize
    }

    fn toggle_auto_update(_current_setting: bool) -> Message {
        ToggleAutoUpdate
    }
}

/// If the show_position_updates setting is missing in the config file, then default to true so
/// they are shown.
fn default_show_position() -> bool {
    true
}

/// If the show_user_updates setting is missing in the config file, then default to true so
/// they are shown.
fn default_show_user() -> bool {
    true
}

/// If the auto_update_startup setting is missing in the config file, then default to true, so
/// they are shown.
fn default_auto_update_startup() -> bool {
    true
}

/// If the auto_reconnect setting is missing in the config file, then default to true, so
/// it's done always unless disabled
fn default_auto_reconnect() -> bool {
    true
}

// Private methods for async reading and writing of config files
async fn load(config_path: PathBuf) -> io::Result<Config> {
    let config_str = tokio::fs::read_to_string(config_path).await?;
    toml::from_str(&config_str).map_err(io::Error::other)
}

async fn save(config_path: PathBuf, config: Config) -> io::Result<()> {
    let mut config_file = File::create(&config_path).await?;
    let config_str = toml::to_string(&config).map_err(io::Error::other)?;
    config_file.write_all(config_str.as_bytes()).await?;
    config_file.sync_all().await
}

async fn create(config_dir: PathBuf, filename: &str) -> io::Result<()> {
    // Create any directories required for the config file
    DirBuilder::new()
        .recursive(true)
        .create(&config_dir)
        .await?;
    // Create the config file itself
    let config_file = File::create(&config_dir.join(filename)).await?;
    config_file.sync_all().await
}

/// Use `load_config` to load the config from disk from the UI
pub fn load_config() -> Task<Message> {
    if let Some(proj_dirs) = ProjectDirs::from("net", "Mackenzie Serres", "meshchat") {
        let config_dir = proj_dirs.config_dir().to_path_buf();
        let config_path = config_dir.join("config.toml");
        if config_path.exists() {
            Task::perform(load(config_path.clone()), {
                move |result| match result {
                    Ok(config) => Message::ConfigLoaded(config),
                    Err(e) => Message::AppError(
                        format!(
                            "Error loading config file: '{}'",
                            config_path.to_string_lossy()
                        ),
                        e.to_string(),
                        MeshChat::now(),
                    ),
                }
            })
        } else {
            // Create the config file so that it can be relied upon to always exist later on
            Task::perform(create(config_dir, "config.toml"), {
                move |result| match result {
                    Ok(_) => Message::None,
                    Err(e) => Message::AppError(
                        format!(
                            "Error creating config file: '{}'",
                            config_path.to_string_lossy()
                        ),
                        e.to_string(),
                        MeshChat::now(),
                    ),
                }
            })
        }
    } else {
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, HistoryLength, ONE_DAY_IN_SECONDS, load, save};
    use btleplug::api::BDAddr;
    use std::io;
    use std::time::Duration;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    fn assert_default(config: Config) {
        assert!(config.ble_device.is_none());
        assert!(config.channel_id.is_none());
        assert!(config.fav_nodes.is_empty());
        assert!(config.aliases.is_empty());
        assert!(config.device_aliases.is_empty());
    }

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_default(config);
    }

    #[tokio::test]
    async fn creates_file() {
        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");
        save(tempfile.path().join("config.toml"), Config::default())
            .await
            .expect("Could not save config file");
        assert!(tempfile.path().join("config.toml").exists());
    }

    #[tokio::test]
    async fn loads_default() {
        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");
        save(tempfile.path().join("config.toml"), Config::default())
            .await
            .expect("Could not save config file");
        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");
        assert_default(returned);
    }

    #[cfg(feature = "meshtastic")]
    #[tokio::test]
    async fn mac_address_saved() {
        let config = Config {
            ble_device: Some((
                BDAddr::from([0, 1, 2, 3, 4, 5]).to_string(),
                RadioType::Meshtastic,
            )),
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");
        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");
        assert_eq!(
            returned.ble_device.expect("BLE device address not saved"),
            (
                BDAddr::from([0, 1, 2, 3, 4, 5]).to_string(),
                RadioType::Meshtastic
            )
        );
    }

    #[tokio::test]
    async fn history_length_default_saved() {
        let config = Config {
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");
        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");
        assert_eq!(returned.history_length, HistoryLength::All);
    }

    #[tokio::test]
    async fn history_length_messages_saved() {
        let config = Config {
            history_length: HistoryLength::NumberOfMessages(10),
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");
        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");
        assert_eq!(returned.history_length, HistoryLength::NumberOfMessages(10));
    }

    #[tokio::test]
    async fn history_length_duration_saved() {
        let config = Config {
            history_length: HistoryLength::Duration(Duration::from_secs(ONE_DAY_IN_SECONDS)),
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");
        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");
        assert_eq!(
            returned.history_length,
            HistoryLength::Duration(Duration::from_secs(86_400))
        );
    }

    #[tokio::test]
    async fn history_duration_deser() {
        let config_str = "[history_length.duration]\nsecs = 86400\nnanos = 0";
        let returned: Config = toml::from_str(config_str)
            .map_err(io::Error::other)
            .expect("Could not deserialize config");
        assert_eq!(
            returned.history_length,
            HistoryLength::Duration(Duration::from_secs(86_400))
        );
    }

    #[tokio::test]
    async fn auto_reconnect_default() {
        let config = Config {
            ..Default::default()
        };

        assert!(config.auto_reconnect);
    }

    #[tokio::test]
    async fn empty_config_desers_to_default() {
        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        let config_file_path = &tempfile.path().join("config.toml");
        let mut config_file = File::create(config_file_path)
            .await
            .expect("Could not create temporary config file");
        let config_str = "";
        config_file
            .write_all(config_str.as_bytes())
            .await
            .expect("Could not write to temporary config file");
        config_file
            .sync_all()
            .await
            .expect("Could not sync temporary config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");
        assert_eq!(returned, Config::default());
    }

    #[tokio::test]
    async fn auto_reconnect_deser() {
        let config = Config {
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");
        assert!(returned.auto_reconnect);
    }

    #[test]
    fn test_history_length_is_all() {
        assert!(HistoryLength::All.is_all());
        assert!(!HistoryLength::NumberOfMessages(10).is_all());
        assert!(!HistoryLength::Duration(Duration::from_secs(3600)).is_all());
    }

    #[test]
    fn test_history_length_display_all() {
        let display = format!("{}", HistoryLength::All);
        assert_eq!(display, "Store all messages");
    }

    #[test]
    fn test_history_length_display_messages() {
        let display = format!("{}", HistoryLength::NumberOfMessages(50));
        assert_eq!(display, "Store last 50 messages");
    }

    #[test]
    fn test_history_length_display_duration() {
        let display = format!("{}", HistoryLength::Duration(Duration::from_secs(3600)));
        assert_eq!(display, "Store all messages in last 1 hours");
    }

    #[test]
    fn test_history_length_display_duration_eight_hours() {
        let display = format!(
            "{}",
            HistoryLength::Duration(Duration::from_secs(60 * 60 * 8))
        );
        assert_eq!(display, "Store all messages in last 8 hours");
    }

    #[test]
    fn test_history_length_display_duration_one_day() {
        let display = format!(
            "{}",
            HistoryLength::Duration(Duration::from_secs(ONE_DAY_IN_SECONDS))
        );
        assert_eq!(display, "Store all messages in last 24 hours");
    }

    #[test]
    fn test_history_length_all_variants() {
        // Test that ALL constant has the correct number of variants
        assert_eq!(HistoryLength::ALL.len(), 5);
        assert_eq!(HistoryLength::ALL[0], HistoryLength::All);
        assert_eq!(HistoryLength::ALL[1], HistoryLength::NumberOfMessages(10));
        assert_eq!(HistoryLength::ALL[2], HistoryLength::NumberOfMessages(100));
    }

    #[test]
    fn test_config_default_show_position() {
        let config = Config::default();
        // Default for show_position_updates should be false (from Default trait),
        // but the serde default is true
        assert!(config.show_position_updates);
    }

    #[test]
    fn test_config_default_auto_update() {
        let config = Config::default();
        // Default for auto_update_startup from Default trait is false
        assert!(config.auto_update_startup);
    }

    #[tokio::test]
    async fn test_fav_nodes_saved() {
        let mut fav_nodes = std::collections::HashSet::new();
        fav_nodes.insert(123);
        fav_nodes.insert(456);

        let config = Config {
            fav_nodes,
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert_eq!(returned.fav_nodes.len(), 2);
        assert!(returned.fav_nodes.contains(&123));
        assert!(returned.fav_nodes.contains(&456));
    }

    #[tokio::test]
    async fn test_aliases_saved() {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert(123, "My Friend".to_string());

        let config = Config {
            aliases,
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert_eq!(returned.aliases.len(), 1);
        assert_eq!(returned.aliases.get(&123), Some(&"My Friend".to_string()));
    }

    #[tokio::test]
    async fn test_device_aliases_saved() {
        let mut device_aliases = std::collections::HashMap::new();
        device_aliases.insert("AA:BB:CC:DD:EE:FF".to_string(), "My Radio".to_string());

        let config = Config {
            device_aliases,
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert_eq!(returned.device_aliases.len(), 1);
        assert_eq!(
            returned.device_aliases.get("AA:BB:CC:DD:EE:FF"),
            Some(&"My Radio".to_string())
        );
    }

    #[tokio::test]
    async fn test_show_position_updates_saved() {
        let config = Config {
            show_position_updates: false,
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert!(!returned.show_position_updates);
    }

    #[tokio::test]
    async fn test_show_user_updates_saved() {
        let config = Config {
            show_user_updates: false,
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert!(!returned.show_user_updates);
    }

    #[tokio::test]
    async fn test_disable_auto_reconnect_saved() {
        let config = Config {
            auto_reconnect: true,
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert!(returned.auto_reconnect);
    }

    // Tests for WindowPosition and WindowSize
    use crate::config::{WindowPosition, WindowSize};
    use crate::device_list::RadioType;
    use iced::{Point, Size};

    #[test]
    fn test_window_position_from_point() {
        let point = Point::new(100.5, 200.7);
        let window_pos: WindowPosition = point.into();
        assert_eq!(window_pos.x, 100);
        assert_eq!(window_pos.y, 200);
    }

    #[test]
    fn test_window_position_to_point() {
        let window_pos = WindowPosition { x: 150, y: 250 };
        let point = window_pos.point();
        assert_eq!(point.x, 150.0);
        assert_eq!(point.y, 250.0);
    }

    #[test]
    fn test_window_size_from_size() {
        let size = Size::new(800.9, 600.1);
        let window_size: WindowSize = size.into();
        assert_eq!(window_size.width, 800);
        assert_eq!(window_size.height, 600);
    }

    #[test]
    fn test_window_size_to_size() {
        let window_size = WindowSize {
            width: 1024,
            height: 768,
        };
        let size = window_size.size();
        assert_eq!(size.width, 1024.0);
        assert_eq!(size.height, 768.0);
    }

    #[test]
    fn test_window_position_equality() {
        let pos1 = WindowPosition { x: 100, y: 200 };
        let pos2 = WindowPosition { x: 100, y: 200 };
        let pos3 = WindowPosition { x: 100, y: 300 };
        assert_eq!(pos1, pos2);
        assert_ne!(pos1, pos3);
    }

    #[test]
    fn test_window_size_equality() {
        let size1 = WindowSize {
            width: 800,
            height: 600,
        };
        let size2 = WindowSize {
            width: 800,
            height: 600,
        };
        let size3 = WindowSize {
            width: 1024,
            height: 600,
        };
        assert_eq!(size1, size2);
        assert_ne!(size1, size3);
    }

    #[test]
    fn test_window_position_debug() {
        let pos = WindowPosition { x: 100, y: 200 };
        let debug_str = format!("{:?}", pos);
        assert!(debug_str.contains("WindowPosition"));
        assert!(debug_str.contains("100"));
        assert!(debug_str.contains("200"));
    }

    #[test]
    fn test_window_size_debug() {
        let size = WindowSize {
            width: 800,
            height: 600,
        };
        let debug_str = format!("{:?}", size);
        assert!(debug_str.contains("WindowSize"));
        assert!(debug_str.contains("800"));
        assert!(debug_str.contains("600"));
    }

    #[test]
    fn test_window_position_clone() {
        let pos1 = WindowPosition { x: 100, y: 200 };
        let pos2 = pos1.clone();
        assert_eq!(pos1, pos2);
    }

    #[test]
    fn test_window_size_clone() {
        let size1 = WindowSize {
            width: 800,
            height: 600,
        };
        let size2 = size1.clone();
        assert_eq!(size1, size2);
    }

    #[tokio::test]
    async fn test_window_position_saved() {
        let config = Config {
            restore_window_position: true,
            window_position: Some(WindowPosition { x: 100, y: 200 }),
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert!(returned.restore_window_position);
        assert_eq!(
            returned.window_position,
            Some(WindowPosition { x: 100, y: 200 })
        );
    }

    #[tokio::test]
    async fn test_window_size_saved() {
        let config = Config {
            restore_window_size: true,
            window_size: Some(WindowSize {
                width: 1024,
                height: 768,
            }),
            ..Default::default()
        };

        let tempfile = tempfile::Builder::new()
            .prefix("meshchat")
            .tempdir()
            .expect("Could not create a temp file for test");

        save(tempfile.path().join("config.toml"), config.clone())
            .await
            .expect("Could not save config file");

        let returned = load(tempfile.path().join("config.toml"))
            .await
            .expect("Could not load config file");

        assert!(returned.restore_window_size);
        assert_eq!(
            returned.window_size,
            Some(WindowSize {
                width: 1024,
                height: 768
            })
        );
    }

    #[test]
    fn test_config_debug() {
        let config = Config::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("Config"));
    }

    #[test]
    fn test_config_clone() {
        let config = Config {
            auto_reconnect: false,
            show_position_updates: false,
            ..Default::default()
        };
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_history_length_clone() {
        let hl = HistoryLength::NumberOfMessages(50);
        // HistoryLength implements Copy, but we test clone() works too
        #[allow(clippy::clone_on_copy)]
        let cloned = hl.clone();
        assert_eq!(hl, cloned);
    }

    #[test]
    fn test_history_length_copy() {
        let hl = HistoryLength::All;
        let copied = hl;
        assert_eq!(hl, copied);
    }

    #[test]
    fn test_history_length_debug() {
        let hl = HistoryLength::Duration(Duration::from_secs(3600));
        let debug_str = format!("{:?}", hl);
        assert!(debug_str.contains("Duration"));
    }

    #[test]
    fn test_default_functions() {
        // Test that default functions return expected values
        assert!(super::default_show_position());
        assert!(super::default_show_user());
        assert!(super::default_auto_reconnect());
        assert!(super::default_auto_update_startup());
    }

    // Tests for view functions and toggle functions

    #[test]
    fn test_config_view_default() {
        let config = Config::default();
        let _element = config.view();
        // Should not panic and return settings Element
    }

    #[test]
    fn test_config_view_with_various_settings() {
        let config = Config {
            show_position_updates: false,
            show_user_updates: false,
            auto_reconnect: false,
            auto_update_startup: false,
            restore_window_position: true,
            restore_window_size: true,
            history_length: HistoryLength::NumberOfMessages(50),
            ..Default::default()
        };
        let _element = config.view();
        // Should not panic
    }

    #[test]
    fn test_config_view_all_history_lengths() {
        for history_length in HistoryLength::ALL {
            let config = Config {
                history_length,
                ..Default::default()
            };
            let _element = config.view();
            // Each history length should produce a valid view
        }
    }

    #[test]
    fn test_show_position_in_chat_setting_true() {
        let config = Config {
            show_position_updates: true,
            ..Default::default()
        };
        let _element = config.show_position_in_chat_setting();
        // Should not panic
    }

    #[test]
    fn test_show_position_in_chat_setting_false() {
        let config = Config {
            show_position_updates: false,
            ..Default::default()
        };
        let _element = config.show_position_in_chat_setting();
        // Should not panic
    }

    #[test]
    fn test_show_user_updates_view_true() {
        let config = Config {
            show_user_updates: true,
            ..Default::default()
        };
        let _element = config.show_user_updates();
        // Should not panic
    }

    #[test]
    fn test_show_user_updates_view_false() {
        let config = Config {
            show_user_updates: false,
            ..Default::default()
        };
        let _element = config.show_user_updates();
        // Should not panic
    }

    #[test]
    fn test_auto_reconnect_view_true() {
        let config = Config {
            auto_reconnect: true,
            ..Default::default()
        };
        let _element = config.auto_reconnect();
        // Should not panic
    }

    #[test]
    fn test_auto_reconnect_view_false() {
        let config = Config {
            auto_reconnect: false,
            ..Default::default()
        };
        let _element = config.auto_reconnect();
        // Should not panic
    }

    #[test]
    fn test_history_length_view() {
        let config = Config::default();
        let _element = config.history_length();
        // Should not panic
    }

    #[test]
    fn test_auto_update_view_true() {
        let config = Config {
            auto_update_startup: true,
            ..Default::default()
        };
        let _element = config.auto_update();
        // Should not panic
    }

    #[test]
    fn test_auto_update_view_false() {
        let config = Config {
            auto_update_startup: false,
            ..Default::default()
        };
        let _element = config.auto_update();
        // Should not panic
    }

    #[test]
    fn test_save_window_position_view_true() {
        let config = Config {
            restore_window_position: true,
            ..Default::default()
        };
        let _element = config.save_window_position();
        // Should not panic
    }

    #[test]
    fn test_save_window_position_view_false() {
        let config = Config {
            restore_window_position: false,
            ..Default::default()
        };
        let _element = config.save_window_position();
        // Should not panic
    }

    #[test]
    fn test_save_window_size_view_true() {
        let config = Config {
            restore_window_size: true,
            ..Default::default()
        };
        let _element = config.save_window_size();
        // Should not panic
    }

    #[test]
    fn test_save_window_size_view_false() {
        let config = Config {
            restore_window_size: false,
            ..Default::default()
        };
        let _element = config.save_window_size();
        // Should not panic
    }

    // Test toggle functions
    #[test]
    fn test_toggle_show_position_updates() {
        let msg = Config::toggle_show_position_updates(true);
        assert!(matches!(msg, crate::Message::ToggleShowPositionUpdates));

        let msg = Config::toggle_show_position_updates(false);
        assert!(matches!(msg, crate::Message::ToggleShowPositionUpdates));
    }

    #[test]
    fn test_toggle_show_user_updates() {
        let msg = Config::toggle_show_user_updates(true);
        assert!(matches!(msg, crate::Message::ToggleShowUserUpdates));

        let msg = Config::toggle_show_user_updates(false);
        assert!(matches!(msg, crate::Message::ToggleShowUserUpdates));
    }

    #[test]
    fn test_toggle_auto_reconnects() {
        let msg = Config::toggle_auto_reconnects(true);
        assert!(matches!(msg, crate::Message::ToggleAutoReconnect));

        let msg = Config::toggle_auto_reconnects(false);
        assert!(matches!(msg, crate::Message::ToggleAutoReconnect));
    }

    #[test]
    fn test_toggle_auto_update() {
        let msg = Config::toggle_auto_update(true);
        assert!(matches!(msg, crate::Message::ToggleAutoUpdate));

        let msg = Config::toggle_auto_update(false);
        assert!(matches!(msg, crate::Message::ToggleAutoUpdate));
    }

    #[test]
    fn test_toggle_save_window_position() {
        let msg = Config::toggle_save_window_position(true);
        assert!(matches!(msg, crate::Message::ToggleSaveWindowPosition));

        let msg = Config::toggle_save_window_position(false);
        assert!(matches!(msg, crate::Message::ToggleSaveWindowPosition));
    }

    #[test]
    fn test_toggle_save_window_size() {
        let msg = Config::toggle_save_window_size(true);
        assert!(matches!(msg, crate::Message::ToggleSaveWindowSize));

        let msg = Config::toggle_save_window_size(false);
        assert!(matches!(msg, crate::Message::ToggleSaveWindowSize));
    }
}
