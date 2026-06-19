use crate::Message::{CloseSettingsDialog, OpenSettingsDialog, ShowUserInfo};
use crate::meshchat::{self, MeshChat};
use iced_test::simulator::simulator;

fn test_app() -> MeshChat {
    meshchat::tests::test_app()
}

fn view_contains(app: &MeshChat, text: &str) -> bool {
    let view = app.view();
    let mut ui = simulator(view);
    ui.find(text).is_ok()
}

#[test]
fn view_renders_device_list_header() {
    let app = test_app();
    assert!(view_contains(&app, "Devices"));
}

#[test]
fn view_renders_empty_state_message() {
    let app = test_app();
    assert!(view_contains(&app, "No compatible radios found"));
}

#[test]
fn open_settings_then_close_via_message() {
    let mut app = test_app();
    let _ = app.update(OpenSettingsDialog);
    assert!(view_contains(&app, "Settings"));
    let _ = app.update(CloseSettingsDialog);
    assert!(!view_contains(&app, "Settings"));
}

#[test]
fn settings_contains_title() {
    let mut app = test_app();
    let _ = app.update(OpenSettingsDialog);
    assert!(view_contains(&app, "Settings"));
}

#[test]
fn view_does_not_panic_in_device_list() {
    let app = test_app();
    let _view = app.view();
}

#[test]
fn view_does_not_panic_in_settings() {
    let mut app = test_app();
    let _ = app.update(OpenSettingsDialog);
    let _view = app.view();
}

#[test]
fn view_does_not_panic_with_user_info_modal() {
    let mut app = test_app();
    let user = meshchat::MCUser {
        id: "!test123".to_string(),
        long_name: "Test User".to_string(),
        short_name: "TU".to_string(),
        hw_model_str: "TBEAM".to_string(),
        hw_model: 0,
        is_licensed: false,
        role_str: "CLIENT".to_string(),
        role: 0,
        public_key: vec![],
        is_unmessagable: false,
    };
    let _ = app.update(ShowUserInfo(user));
    let _view = app.view();
    assert!(view_contains(&app, "Node User Info"));
}

#[test]
fn user_info_modal_shows_user_details() {
    let mut app = test_app();
    let user = meshchat::MCUser {
        id: "!abc999".to_string(),
        long_name: "Alice".to_string(),
        short_name: "ALI".to_string(),
        hw_model_str: "TBEAM".to_string(),
        hw_model: 0,
        is_licensed: false,
        role_str: "CLIENT".to_string(),
        role: 0,
        public_key: vec![],
        is_unmessagable: false,
    };
    let _ = app.update(ShowUserInfo(user));
    assert!(view_contains(&app, "ID: !abc999"));
    assert!(view_contains(&app, "Long Name: Alice"));
    assert!(view_contains(&app, "Short Name: ALI"));
}
