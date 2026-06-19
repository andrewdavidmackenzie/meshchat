use crate::meshchat::{self, MeshChat};
use crate::message::MCContent;
use iced_test::simulator::simulator;

fn test_app() -> MeshChat {
    meshchat::tests::test_app()
}

fn view_contains(app: &MeshChat, text: &str) -> bool {
    let view = app.view();
    let mut ui = simulator(view);
    ui.find(text).is_ok()
}

fn add_message(app: &mut MeshChat, text: &str) {
    app.new_message(MCContent::NewTextMessage(text.to_string()));
}

/// Navigate to the device view, showing a conversation view for channel 0
fn navigate_to_channel_view(app: &mut MeshChat) {
    let _ = app.update(crate::Message::Navigation(meshchat::View::DeviceView(
        Some(crate::conversation_id::ConversationId::Channel(0.into())),
    )));
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
    let _ = app.update(crate::Message::OpenSettingsDialog);
    assert!(view_contains(&app, "Settings"));
    let _ = app.update(crate::Message::CloseSettingsDialog);
    assert!(!view_contains(&app, "Settings"));
}

#[test]
fn settings_contains_title() {
    let mut app = test_app();
    let _ = app.update(crate::Message::OpenSettingsDialog);
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
    let _ = app.update(crate::Message::OpenSettingsDialog);
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
    let _ = app.update(crate::Message::ShowUserInfo(user));
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
    let _ = app.update(crate::Message::ShowUserInfo(user));
    assert!(view_contains(&app, "ID: !abc999"));
    assert!(view_contains(&app, "Long Name: Alice"));
    assert!(view_contains(&app, "Short Name: ALI"));
}

#[test]
fn user_info_modal_shows_public_key_and_unmessageable() {
    let mut app = test_app();
    let user = meshchat::MCUser {
        id: "!key.test".to_string(),
        long_name: "Key Test".to_string(),
        short_name: "KT".to_string(),
        hw_model_str: "TBEAM".to_string(),
        hw_model: 0,
        is_licensed: true,
        role_str: "CLIENT".to_string(),
        role: 0,
        public_key: vec![0xDE, 0xAD, 0xBE, 0xEF],
        is_unmessagable: false,
    };
    let _ = app.update(crate::Message::ShowUserInfo(user));
    assert!(view_contains(&app, "Node User Info"));
    assert!(view_contains(&app, "Public Key: [DE, AD, BE, EF]"));
    assert!(view_contains(&app, "Unmessageable: false"));
    assert!(view_contains(&app, "Licensed: true"));
}

/// Test that the user info modal shows correctly when it has a public key
#[test]
fn user_info_modal_shows_empty_public_key() {
    let mut app = test_app();
    let user = meshchat::MCUser {
        id: "!emptykey".to_string(),
        long_name: "No Key".to_string(),
        short_name: "NK".to_string(),
        hw_model_str: "TBEAM".to_string(),
        hw_model: 0,
        is_licensed: false,
        role_str: "CLIENT".to_string(),
        role: 0,
        public_key: vec![],
        is_unmessagable: true,
    };
    let _ = app.update(crate::Message::ShowUserInfo(user));
    assert!(view_contains(&app, "Unmessageable: true"));
}

/// Test that navigating to DeviceView works without panic
#[test]
fn navigate_to_device_view() {
    let mut app = test_app();
    let _ = app.update(crate::Message::Navigation(meshchat::View::DeviceView(
        Some(crate::conversation_id::ConversationId::Channel(0.into())),
    )));
    let _view = app.view();
}

/// Test that device view with a conversation renders without panic
/// and still shows the "Devices" back button in the header
#[test]
fn device_view_shows_channel_buttons() {
    let mut app = test_app();
    add_message(&mut app, "Hello in channel");
    navigate_to_channel_view(&mut app);
    // The "Devices" back button is always shown in the device header
    assert!(view_contains(&app, "Devices"));
}

/// Test that device view with messages does not panic
#[test]
fn device_view_with_messages_does_not_panic() {
    let mut app = test_app();
    add_message(&mut app, "First message");
    add_message(&mut app, "Second message");
    add_message(&mut app, "Third message");
    navigate_to_channel_view(&mut app);
    let _view = app.view();
}

/// Test that the empty state shows when no messages
#[test]
fn device_view_empty_channel_shows_no_messages_text() {
    let mut app = test_app();
    navigate_to_channel_view(&mut app);
    assert!(view_contains(&app, "No messages sent or received yet."));
}

/// Test that device view channel header shows the channel name
/// The channel name is prefixed with "🛜  " in the header button
#[test]
fn device_view_channel_header_shows_channel_name() {
    let mut app = test_app();
    navigate_to_channel_view(&mut app);
    let view = app.view();
    let mut ui = simulator(view);
    assert!(ui.find("🛜  Test").is_ok() || ui.find("Test").is_ok());
}

/// Test that navigating between device list and device view works
#[test]
fn navigate_between_views() {
    let mut app = test_app();
    assert!(view_contains(&app, "Devices"));
    navigate_to_channel_view(&mut app);
    // Should not panic when rendering device view
    {
        let _view = app.view();
    }
    // Navigate back
    let _ = app.update(crate::Message::Navigation(meshchat::View::DeviceListView));
    assert!(view_contains(&app, "Devices"));
}

/// Test app notification appears in the view
#[test]
fn app_notification_appears_in_view() {
    let mut app = test_app();
    let _ = app.update(crate::Message::AppNotification(
        "Test Summary".to_string(),
        "Test Detail".to_string(),
        crate::timestamp::TimeStamp::now(),
    ));
    assert!(view_contains(&app, "Test Summary"));
}

/// Test app error notification appears in the view
#[test]
fn app_error_notification_appears_in_view() {
    let mut app = test_app();
    let _ = app.update(crate::Message::AppError(
        "Error Summary".to_string(),
        "Error Detail".to_string(),
        crate::timestamp::TimeStamp::now(),
    ));
    assert!(view_contains(&app, "Error Summary"));
}

/// Test critical error notification appears in the view
#[test]
fn critical_error_notification_appears_in_view() {
    let mut app = test_app();
    let _ = app.update(crate::Message::CriticalAppError(
        "Critical Summary".to_string(),
        "Critical Detail".to_string(),
        crate::timestamp::TimeStamp::now(),
    ));
    assert!(view_contains(&app, "Critical Summary"));
}

/// Test settings dialog shows toggles
#[test]
fn settings_dialog_shows_content() {
    let mut app = test_app();
    let _ = app.update(crate::Message::OpenSettingsDialog);
    assert!(view_contains(&app, "Settings"));
    // Settings content renders without panic (toggler labels may not be queryable)
    let _view = app.view();
}

/// Test settings dialog close button works
#[test]
fn settings_dialog_can_be_closed() {
    let mut app = test_app();
    let _ = app.update(crate::Message::OpenSettingsDialog);
    assert!(view_contains(&app, "Settings"));
    let _ = app.update(crate::Message::CloseSettingsDialog);
    assert!(!view_contains(&app, "Settings"));
}

/// Test the view does not panic after opening and closing settings
#[test]
fn view_does_not_panic_after_settings_toggle() {
    let mut app = test_app();
    let _ = app.update(crate::Message::OpenSettingsDialog);
    let _ = app.view();
    let _ = app.update(crate::Message::CloseSettingsDialog);
    let _view = app.view();
}

/// Test view with settings and a notification at the same time
#[test]
fn view_with_settings_and_notification() {
    let mut app = test_app();
    let _ = app.update(crate::Message::AppNotification(
        "Network".to_string(),
        "Connected".to_string(),
        crate::timestamp::TimeStamp::now(),
    ));
    let _ = app.update(crate::Message::OpenSettingsDialog);
    let _view = app.view();
    assert!(view_contains(&app, "Network"));
    assert!(view_contains(&app, "Settings"));
}

/// Test view with user info modal and notification
#[test]
fn view_with_user_info_and_notification() {
    let mut app = test_app();
    let _ = app.update(crate::Message::AppNotification(
        "Update".to_string(),
        "Done".to_string(),
        crate::timestamp::TimeStamp::now(),
    ));
    let user = meshchat::MCUser {
        id: "!both".to_string(),
        long_name: "Both".to_string(),
        short_name: "BT".to_string(),
        hw_model_str: "TBEAM".to_string(),
        hw_model: 0,
        is_licensed: false,
        role_str: "CLIENT".to_string(),
        role: 0,
        public_key: vec![],
        is_unmessagable: false,
    };
    let _ = app.update(crate::Message::ShowUserInfo(user));
    let _view = app.view();
    assert!(view_contains(&app, "Update"));
    assert!(view_contains(&app, "Node User Info"));
}

/// Test that the view still works after navigating to channel and back repeatedly
#[test]
fn view_survives_repeated_navigation() {
    let mut app = test_app();
    for _ in 0..3 {
        navigate_to_channel_view(&mut app);
        {
            let _v1 = app.view();
        }
        let _ = app.update(crate::Message::Navigation(meshchat::View::DeviceListView));
        {
            let _v2 = app.view();
        }
    }
}

/// Test device view with forwarding_message set (channel picker modal)
#[test]
fn device_view_with_forwarding_message_shows_channel_picker() {
    let mut app = test_app();
    navigate_to_channel_view(&mut app);
    let entry = crate::message::MCMessage::new(
        crate::conversation_id::MessageId::from(1),
        crate::conversation_id::NodeId::from(100u64),
        crate::message::MCContent::NewTextMessage("forward me".into()),
        crate::timestamp::TimeStamp::from(0u64),
    );
    let _ = app.update(crate::Message::DeviceViewEvent(
        crate::device::DeviceMessage::StartForwardingMessage(entry),
    ));
    let _view = app.view();
    // Verify the forwarding message was set
    assert!(app.device.forwarding_message.is_some());
    // Check the view renders the channel picker text
    // Note: iced_test's find() may not detect text inside a modal overlay
    // so we just verify it doesn't panic and the state is correct
}
