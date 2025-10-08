use crate::Message;
use directories::ProjectDirs;
use futures_lite::io::AsyncWriteExt;
use iced::Task;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub device_id: Option<String>,
    pub channel_name: Option<String>,
}

async fn load(config_path: PathBuf) -> Result<Config, anyhow::Error> {
    let config_str = smol::fs::read_to_string(config_path).await?;
    Ok(toml::from_str(&config_str)?)
}

async fn save(config_path: PathBuf, config: Config) -> Result<(), anyhow::Error> {
    let mut config_file = smol::fs::File::open(&config_path).await?;
    let config_str = toml::to_string(&config)?;
    Ok(config_file.write_all(config_str.as_bytes()).await?)
}

async fn create(config_path: PathBuf) -> Result<(), anyhow::Error> {
    smol::fs::create_dir_all(config_path.parent().unwrap()).await?;
    let _ = smol::fs::File::create(&config_path).await?;
    Ok(())
}

pub fn save_config(config: Config) -> Task<Message> {
    if let Some(proj_dirs) = ProjectDirs::from("net", "Mackenzie Serres", "meshchat") {
        let config_path = proj_dirs.config_dir().join("config.toml");

        Task::perform(save(config_path, config), {
            |result| match result {
                Ok(_) => {
                    println!("Saved config file");
                    Message::None
                }
                Err(e) => Message::AppError(format!("Error saving config file: {e}")),
            }
        })
    } else {
        Task::none()
    }
}

pub fn load_config() -> Task<Message> {
    if let Some(proj_dirs) = ProjectDirs::from("net", "Mackenzie Serres", "meshchat") {
        let config_path = proj_dirs.config_dir().join("config.toml");
        if config_path.exists() {
            Task::perform(load(config_path), {
                |result| match result {
                    Ok(config) => Message::NewConfig(config),
                    Err(e) => Message::AppError(format!("Error loading config file: {e}")),
                }
            })
        } else {
            // Create the config file so that it can be relied upon to always exist later on
            Task::perform(create(config_path), {
                |result| match result {
                    Ok(_) => Message::None,
                    Err(e) => Message::AppError(format!("Error creating config file: {e}")),
                }
            })
        }
    } else {
        Task::none()
    }
}
