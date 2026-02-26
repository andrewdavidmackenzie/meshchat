//! MeshChat is an iced GUI app that uses the mesht "rust" crate to discover and control
//! mesht compatible radios connected to the host running it

#[cfg(not(any(feature = "meshtastic", feature = "meshcore")))]
compile_error!("At least one of 'meshtastic' or 'meshcore' features must be enabled");

use crate::meshchat::MeshChat;
use iced::window;

mod meshchat;

mod channel_view;
mod config;
mod device;
mod device_list;
mod discovery;
mod message;
mod styles;
mod widgets;

mod conversation_id;
mod notification;
#[cfg(test)]
mod test_helper;

#[cfg(feature = "meshcore")]
mod meshc;
#[cfg(feature = "meshtastic")]
mod mesht;

#[rustfmt::skip]
/// Icons generated as a font using iced_fontello
mod icons;

pub use crate::meshchat::Message;

fn main() -> iced::Result {
    #[cfg(feature = "debug")]
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    #[allow(unused_mut)]
    let mut window_settings = window::Settings {
        exit_on_close_request: false,
        resizable: true,
        ..Default::default()
    };

    // Try and add an icon to the window::Settings
    #[cfg(not(target_os = "macos"))]
    let icon_bytes = include_bytes!("../assets/images/icon.ico");
    #[cfg(not(target_os = "macos"))]
    if let Ok(app_icon) = iced::window::icon::from_file_data(icon_bytes, None) {
        window_settings.icon = Some(app_icon);
    }

    iced::application(MeshChat::new, MeshChat::update, MeshChat::view)
        .subscription(MeshChat::subscription)
        .font(icons::FONT)
        .title(MeshChat::title)
        .window(window_settings)
        .run()
}
