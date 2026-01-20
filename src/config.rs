use crate::Message;
use crate::Message::{
    HistoryLengthSelected, ToggleAutoReconnect, ToggleShowPositionUpdates, ToggleShowUserUpdates,
};
use crate::channel_id::ChannelId;
use crate::styles::{picker_header_style, tooltip_style};
use directories::ProjectDirs;
use iced::font::Weight;
use iced::widget::{Column, container, pick_list, text, toggler};
use iced::{Center, Element, Fill, Font, Task};
use meshtastic::utils::stream::BleDevice;
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, rename = "device", skip_serializing_if = "Option::is_none")]
    pub ble_device: Option<BleDevice>,
    #[serde(default, rename = "channel", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(default = "HashSet::new", skip_serializing_if = "HashSet::is_empty")]
    pub fav_nodes: HashSet<u32>,
    #[serde(default = "HashMap::new", skip_serializing_if = "HashMap::is_empty")]
    pub aliases: HashMap<u32, String>, // node name aliases
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
    #[serde(default)]
    pub disable_auto_reconnect: bool,
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
            .push(self.disable_auto_reconnect())
            .push(self.show_user_updates())
            .push(self.history_length());

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
            Some(self.history_length.clone()),
            HistoryLengthSelected,
        )
        .into()
    }

    fn disable_auto_reconnect<'a>(&self) -> Element<'a, Message> {
        toggler(self.disable_auto_reconnect)
            .label("Disable auto-reconnect at startup")
            .on_toggle(Self::toggle_auto_reconnects)
            .into()
    }

    fn toggle_auto_reconnects(_current_setting: bool) -> Message {
        ToggleAutoReconnect
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

async fn create(config_path: PathBuf) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .create(config_path.parent().unwrap())
        .await?;
    let config_file = File::create(&config_path).await?;
    config_file.sync_all().await
}

/// Use `load_config` to load the config from disk from the UI
pub fn load_config() -> Task<Message> {
    if let Some(proj_dirs) = ProjectDirs::from("net", "Mackenzie Serres", "meshchat") {
        let config_path = proj_dirs.config_dir().join("config.toml");
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
                    ),
                }
            })
        } else {
            // Create the config file so that it can be relied upon to always exist later on
            Task::perform(create(config_path.clone()), {
                move |result| match result {
                    Ok(_) => Message::None,
                    Err(e) => Message::AppError(
                        format!(
                            "Error creating config file: '{}'",
                            config_path.to_string_lossy()
                        ),
                        e.to_string(),
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
    use meshtastic::utils::stream::BleDevice;
    use std::io;
    use std::time::Duration;

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

    #[tokio::test]
    async fn mac_address_saved() {
        let config = Config {
            ble_device: Some(BleDevice {
                name: None,
                mac_address: BDAddr::from([0, 1, 2, 3, 4, 6]),
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
        assert_eq!(
            returned.ble_device.unwrap().mac_address,
            BDAddr::from([0, 1, 2, 3, 4, 6])
        );
    }

    /*
    #[tokio::test]
    async fn name_saved_when_default_mac() {
        let config = Config {
            device_mac_address: Some(BDAddr {
                ::
            }from([0, 1, 2, 3, 4, 6])),
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
            returned.device_mac_address,
            Some(BDAddr::from([0, 1, 2, 3, 4, 6]))
        );
    }


     */
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

        assert!(!config.disable_auto_reconnect);
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
        assert!(!returned.disable_auto_reconnect);
    }
}
