use crate::Message;
use crate::channel_view::ChannelId;
use directories::ProjectDirs;
use iced::Task;
use meshtastic::utils::stream::BleDevice;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use tokio::fs::DirBuilder;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub device: Option<BleDevice>,
    pub channel_id: Option<ChannelId>,
    pub fav_nodes: HashSet<u32>,
    #[serde(default = "HashMap::new")]
    pub aliases: HashMap<u32, String>, // node name aliases
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

        Task::perform(save(config_path, config.clone()), {
            |result| match result {
                Ok(_) => Message::None,
                Err(e) => Message::AppError("Error saving config file".to_string(), e.to_string()),
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
            Task::perform(load(config_path), {
                |result| match result {
                    Ok(config) => Message::NewConfig(config),
                    Err(e) => {
                        Message::AppError("Error loading config file".to_string(), e.to_string())
                    }
                }
            })
        } else {
            // Create the config file so that it can be relied upon to always exist later on
            Task::perform(create(config_path), {
                |result| match result {
                    Ok(_) => Message::None,
                    Err(e) => {
                        Message::AppError("Error creating config file: ".to_string(), e.to_string())
                    }
                }
            })
        }
    } else {
        Task::none()
    }
}
