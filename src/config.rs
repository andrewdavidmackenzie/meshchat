use crate::Message;
use crate::channel_view::ChannelId;
use btleplug::api::BDAddr;
use directories::ProjectDirs;
use iced::Task;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use tokio::fs::DirBuilder;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub device_mac_address: Option<BDAddr>,
    pub channel_id: Option<ChannelId>,
    pub fav_nodes: HashSet<u32>,
    #[serde(default = "HashMap::new")]
    pub aliases: HashMap<u32, String>, // node name aliases
    #[serde(default = "HashMap::new")]
    pub device_aliases: HashMap<BDAddr, String>, // node name aliases
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

/// Use `save_config` to save the config to disk from the UI
pub fn save_config(config: &Config) -> Task<Message> {
    if let Some(proj_dirs) = ProjectDirs::from("net", "Mackenzie Serres", "meshchat") {
        let config_path = proj_dirs.config_dir().join("config.toml");

        Task::perform(save(config_path.clone(), config.clone()), {
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

/// Use `load_config` to load the config from disk from the UI
pub fn load_config() -> Task<Message> {
    if let Some(proj_dirs) = ProjectDirs::from("net", "Mackenzie Serres", "meshchat") {
        let config_path = proj_dirs.config_dir().join("config.toml");
        if config_path.exists() {
            Task::perform(load(config_path.clone()), {
                move |result| match result {
                    Ok(config) => Message::NewConfig(config),
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
    use crate::config::{Config, load, save};
    use btleplug::api::BDAddr;

    fn assert_default(config: Config) {
        assert!(config.device_mac_address.is_none());
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
        let mut config = Config::default();
        config.device_mac_address = Some(BDAddr::from([0, 1, 2, 3, 4, 6]));

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
}
