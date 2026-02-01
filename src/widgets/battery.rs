//! A custom battery widget for Iced that displays the battery charge level or charging status.
//!
//! # Example
//!
//! ```no_run
//! use iced::widget::Column;
//! use meshchat::battery::{Battery, BatteryState};
//!
//! // Create a battery widget showing 75% charge
//! let battery_75 = Battery::new()
//!     .state(BatteryState::Charged(75));
//!
//! // Create a battery widget showing charging status
//! let battery_charging = Battery::new()
//!     .state(BatteryState::Charging);
//!
//! // Create a battery widget showing unknown state
//! let battery_unknown = Battery::new()
//!     .state(BatteryState::Unknown);
//!
//! // Use in a column
//! Column::new()
//!     .push(battery_75)
//!     .push(battery_charging)
//!     .push(battery_unknown);
//! ```

use iced::advanced::graphics::color;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::border::Radius;
use iced::{Background, Border, Color, advanced, mouse};

use iced::{Element, Length, Rectangle, Size, Transformation, Vector};

use iced::advanced::graphics::mesh::{self};

/// The state of the battery widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    /// Battery is charging (show charging icon)
    Charging,
    /// Battery is charged/discharging with the given percentage (0-100)
    Charged(u32),
    /// Battery state is unknown (show question mark)
    Unknown,
}

impl Default for BatteryState {
    fn default() -> Self {
        BatteryState::Charged(100)
    }
}

/// A battery widget that displays the charge level or charging status.
#[allow(missing_debug_implementations)]
pub struct Battery<Theme>
where
    Theme: StyleSheet,
{
    width: Length,
    height: Length,
    style: Theme::Style,
    state: BatteryState,
}

impl<Theme> Battery<Theme>
where
    Theme: StyleSheet,
{
    /// Creates a new [`Battery`] widget.
    pub fn new() -> Self {
        Battery {
            width: Length::Fixed(40.0),
            height: Length::Fixed(20.0),
            style: Theme::Style::default(),
            state: BatteryState::default(),
        }
    }

    /// Sets the width of the [`Battery`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Battery`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the style variant of this [`Battery`].
    pub fn style(mut self, style: impl Into<Theme::Style>) -> Self {
        self.style = style.into();
        self
    }

    /// Sets the battery state (charging or charged with percentage).
    pub fn state(mut self, state: BatteryState) -> Self {
        self.state = state;
        self
    }

    /// Sets the battery state (charging or charged with percentage).
    /// This is a mutable version for updating existing widgets.
    pub fn set_state(&mut self, state: BatteryState) {
        self.state = state;
    }
}

impl<Theme> Default for Battery<Theme>
where
    Theme: StyleSheet,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Battery<Theme>
where
    Message: Clone,
    Theme: StyleSheet,
    Renderer: advanced::Renderer + mesh::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let custom_style = theme.appearance(&self.style);

        // Battery dimensions
        let border_width = 2.0;
        let terminal_width = bounds.height * 0.3; // Small protrusion on the right
        let terminal_height = bounds.height * 0.5;
        let terminal_offset = (bounds.height - terminal_height) / 2.0;

        // Main battery body (without terminal)
        let battery_body = Rectangle {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width - terminal_width - border_width,
            height: bounds.height,
        };

        // Battery terminal (right side protrusion)
        let battery_terminal = Rectangle {
            x: bounds.x + battery_body.width - (border_width / 2.0),
            y: bounds.y + terminal_offset,
            width: terminal_width,
            height: terminal_height,
        };

        // Create border for battery body
        let battery_border = Border {
            radius: Radius::from(2.0),
            width: border_width,
            color: custom_style.border_color,
        };

        // Create border for battery terminal
        let terminal_border = Border {
            radius: Radius::from(1.0),
            width: border_width / 2.0,
            color: custom_style.border_color,
        };

        // Draw battery outline (main body)
        renderer.fill_quad(
            renderer::Quad {
                bounds: battery_body,
                border: battery_border,
                ..renderer::Quad::default()
            },
            Background::Color(custom_style.background_color),
        );

        // Draw battery terminal
        renderer.fill_quad(
            renderer::Quad {
                bounds: battery_terminal,
                border: terminal_border,
                ..renderer::Quad::default()
            },
            Background::Color(custom_style.background_color),
        );

        // Dimensions of the inner area of the battery body
        let inner_width = battery_body.width - border_width * 2.0;
        let inner_height = battery_body.height - border_width * 2.0;

        match self.state {
            BatteryState::Charging => {
                // Draw charging icon (lightning bolt)
                use iced::advanced::graphics::mesh::{self, Mesh, SolidVertex2D};

                let triangle1 = Mesh::Solid {
                    buffers: mesh::Indexed {
                        vertices: vec![
                            SolidVertex2D {
                                // Point 0
                                position: [inner_width * 0.9, inner_height * 0.66],
                                color: color::pack(custom_style.charging_color),
                            },
                            SolidVertex2D {
                                // Point 1
                                position: [inner_width * 0.45, inner_height * 0.1],
                                color: color::pack(custom_style.charging_color),
                            },
                            SolidVertex2D {
                                // Point 2
                                position: [inner_width * 0.45, inner_height * 0.5],
                                color: color::pack(custom_style.charging_color),
                            },
                            SolidVertex2D {
                                // Point 3
                                position: [inner_width * 0.55, inner_height * 0.5],
                                color: color::pack(custom_style.charging_color),
                            },
                            SolidVertex2D {
                                // Point 4
                                position: [inner_width * 0.1, inner_height * 0.33],
                                color: color::pack(custom_style.charging_color),
                            },
                            SolidVertex2D {
                                // Point 5
                                position: [inner_width * 0.55, inner_height * 0.9],
                                color: color::pack(custom_style.charging_color),
                            },
                        ],
                        indices: vec![0, 1, 2, 3, 4, 5],
                    },
                    transformation: Transformation::IDENTITY,
                    clip_bounds: Rectangle::INFINITE,
                };

                renderer.with_translation(
                    Vector::new(bounds.x + border_width, bounds.y + border_width),
                    |renderer| {
                        renderer.draw_mesh(triangle1);
                    },
                );
            }
            BatteryState::Charged(percentage) => {
                // Clamp percentage to 0-100
                let percentage = percentage.min(100);

                // Calculate charge fill width
                let fill_width = inner_width * (percentage as f32 / 100.0);

                if fill_width > 0.0 {
                    // Determine charge color based on percentage
                    let charge_color = if percentage > 50 {
                        custom_style.charge_high_color
                    } else if percentage > 20 {
                        custom_style.charge_medium_color
                    } else {
                        custom_style.charge_low_color
                    };

                    // Draw charge fill
                    let charge_fill = Rectangle {
                        x: bounds.x + border_width,
                        y: bounds.y + border_width,
                        width: fill_width,
                        height: inner_height,
                    };

                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: charge_fill,
                            border: Border {
                                radius: Radius::from(0.0),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..renderer::Quad::default()
                        },
                        Background::Color(charge_color),
                    );
                }
            }
            BatteryState::Unknown => {}
        }
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        _event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
    }
}

impl<'a, Message, Theme, Renderer> From<Battery<Theme>> for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: StyleSheet + 'a,
    Renderer: advanced::Renderer + 'a + mesh::Renderer,
{
    fn from(battery: Battery<Theme>) -> Self {
        Self::new(battery)
    }
}

/// The appearance of a battery widget.
#[derive(Debug, Clone, Copy)]
pub struct Appearance {
    /// The background color of the battery.
    pub background_color: Color,
    /// The border color of the battery.
    pub border_color: Color,
    /// The color when charging.
    pub charging_color: Color,
    /// The color when charge is high (>50%).
    pub charge_high_color: Color,
    /// The color when charge is medium (20-50%).
    pub charge_medium_color: Color,
    /// The color when charge is low (<20%).
    pub charge_low_color: Color,
    /// The color when battery state is unknown.
    pub unknown_color: Color,
}

/*
pub fn battery_style(_theme: &Theme) -> crate::battery::Appearance {
    crate::battery::Appearance {
        background_color: COLOR_GRAY_10,   // Very dark background
        border_color: COLOR_GRAY_80,       // Light gray border
        charging_color: COLOR_GREEN,       // Green for charging
        charge_high_color: COLOR_GREEN,    // Dark green for high charge (>50%)
        charge_medium_color: COLOR_ORANGE, // Orange for medium charge (20-50%)
        charge_low_color: COLOR_RED,       // Dark red for low charge (<20%)
        unknown_color: COLOR_GRAY_40,      // Medium gray for unknown state
    }
}

/// Returns the dark style appearance for the battery widget
pub fn battery_style_dark(_theme: &Theme) -> crate::battery::Appearance {
    crate::battery::Appearance {
        background_color: COLOR_GRAY_10,
        border_color: COLOR_GRAY_80,
        charging_color: COLOR_GREEN,
        charge_high_color: COLOR_GREEN,
        charge_medium_color: COLOR_ORANGE,
        charge_low_color: COLOR_RED,
        unknown_color: COLOR_GRAY_40,
    }
}
*/

impl Default for Appearance {
    fn default() -> Self {
        Self {
            background_color: Color::from_rgba(0.1, 0.1, 0.1, 1.0),
            border_color: Color::WHITE,
            charging_color: Color::from_rgb(0.2, 0.8, 0.2), // Green
            charge_high_color: Color::from_rgb(0.2, 0.8, 0.2), // Green
            charge_medium_color: Color::from_rgb(0.95, 0.9, 0.2), // Yellow
            charge_low_color: Color::from_rgb(0.9, 0.2, 0.2), // Red
            unknown_color: Color::from_rgb(0.7, 0.7, 0.7),  // Gray
        }
    }
}

/// A set of rules that dictate the style of a battery widget.
pub trait StyleSheet {
    /// The supported style of the [`StyleSheet`].
    type Style: Default;

    /// Produces the active [`Appearance`] of a battery widget.
    fn appearance(&self, style: &Self::Style) -> Appearance;
}

impl StyleSheet for iced::Theme {
    type Style = ();

    fn appearance(&self, _style: &Self::Style) -> Appearance {
        Appearance {
            background_color: Color::from_rgb(0.05, 0.05, 0.05), // Very dark background
            border_color: Color::from_rgb(0.4, 0.4, 0.4),        // Light gray border
            charging_color: Color::from_rgb(0.0, 0.7, 0.0),      // Green for charging
            charge_high_color: Color::from_rgb(0.0, 0.7, 0.0), // Dark green for high charge (>50%)
            charge_medium_color: Color::from_rgb(0.8, 0.7, 0.0), // Orange/yellow for medium charge (20-50%)
            charge_low_color: Color::from_rgb(0.8, 0.0, 0.0),    // Dark red for low charge (<20%)
            unknown_color: Color::from_rgb(0.5, 0.5, 0.5),       // Medium gray for unknown state
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test BatteryState enum
    #[test]
    fn test_battery_state_default() {
        let state = BatteryState::default();
        assert_eq!(state, BatteryState::Charged(100));
    }

    #[test]
    fn test_battery_state_charging() {
        let state = BatteryState::Charging;
        assert_eq!(state, BatteryState::Charging);
    }

    #[test]
    fn test_battery_state_charged() {
        let state = BatteryState::Charged(75);
        assert_eq!(state, BatteryState::Charged(75));
    }

    #[test]
    fn test_battery_state_charged_zero() {
        let state = BatteryState::Charged(0);
        assert_eq!(state, BatteryState::Charged(0));
    }

    #[test]
    fn test_battery_state_unknown() {
        let state = BatteryState::Unknown;
        assert_eq!(state, BatteryState::Unknown);
    }

    #[test]
    fn test_battery_state_clone() {
        let state = BatteryState::Charged(50);
        let cloned = state;
        assert_eq!(cloned, BatteryState::Charged(50));
    }

    #[test]
    fn test_battery_state_copy() {
        let state = BatteryState::Charging;
        let copied: BatteryState = state;
        assert_eq!(copied, BatteryState::Charging);
        // Original still valid (Copy trait)
        assert_eq!(state, BatteryState::Charging);
    }

    #[test]
    fn test_battery_state_debug() {
        let state = BatteryState::Charged(42);
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("Charged"));
        assert!(debug_str.contains("42"));
    }

    #[test]
    fn test_battery_state_equality() {
        assert_eq!(BatteryState::Charging, BatteryState::Charging);
        assert_eq!(BatteryState::Unknown, BatteryState::Unknown);
        assert_eq!(BatteryState::Charged(50), BatteryState::Charged(50));
        assert_ne!(BatteryState::Charged(50), BatteryState::Charged(51));
        assert_ne!(BatteryState::Charging, BatteryState::Unknown);
        assert_ne!(BatteryState::Charged(100), BatteryState::Charging);
    }

    // Test Battery struct and builder methods
    #[test]
    fn test_battery_new() {
        let battery: Battery<iced::Theme> = Battery::new();
        assert_eq!(battery.width, Length::Fixed(40.0));
        assert_eq!(battery.height, Length::Fixed(20.0));
        assert_eq!(battery.state, BatteryState::Charged(100));
    }

    #[test]
    fn test_battery_default() {
        let battery: Battery<iced::Theme> = Battery::default();
        assert_eq!(battery.width, Length::Fixed(40.0));
        assert_eq!(battery.height, Length::Fixed(20.0));
        assert_eq!(battery.state, BatteryState::Charged(100));
    }

    #[test]
    fn test_battery_width() {
        let battery: Battery<iced::Theme> = Battery::new().width(Length::Fixed(60.0));
        assert_eq!(battery.width, Length::Fixed(60.0));
    }

    #[test]
    fn test_battery_width_fill() {
        let battery: Battery<iced::Theme> = Battery::new().width(Length::Fill);
        assert_eq!(battery.width, Length::Fill);
    }

    #[test]
    fn test_battery_height() {
        let battery: Battery<iced::Theme> = Battery::new().height(Length::Fixed(30.0));
        assert_eq!(battery.height, Length::Fixed(30.0));
    }

    #[test]
    fn test_battery_height_shrink() {
        let battery: Battery<iced::Theme> = Battery::new().height(Length::Shrink);
        assert_eq!(battery.height, Length::Shrink);
    }

    #[test]
    fn test_battery_style() {
        let battery: Battery<iced::Theme> = Battery::new().style(());
        assert_eq!(battery.style, ());
    }

    #[test]
    fn test_battery_state_builder() {
        let battery: Battery<iced::Theme> = Battery::new().state(BatteryState::Charging);
        assert_eq!(battery.state, BatteryState::Charging);
    }

    #[test]
    fn test_battery_state_charged_builder() {
        let battery: Battery<iced::Theme> = Battery::new().state(BatteryState::Charged(25));
        assert_eq!(battery.state, BatteryState::Charged(25));
    }

    #[test]
    fn test_battery_state_unknown_builder() {
        let battery: Battery<iced::Theme> = Battery::new().state(BatteryState::Unknown);
        assert_eq!(battery.state, BatteryState::Unknown);
    }

    #[test]
    fn test_battery_set_state() {
        let mut battery: Battery<iced::Theme> = Battery::new();
        assert_eq!(battery.state, BatteryState::Charged(100));

        battery.set_state(BatteryState::Charging);
        assert_eq!(battery.state, BatteryState::Charging);

        battery.set_state(BatteryState::Charged(50));
        assert_eq!(battery.state, BatteryState::Charged(50));

        battery.set_state(BatteryState::Unknown);
        assert_eq!(battery.state, BatteryState::Unknown);
    }

    #[test]
    fn test_battery_builder_chain() {
        let battery: Battery<iced::Theme> = Battery::new()
            .width(Length::Fixed(80.0))
            .height(Length::Fixed(40.0))
            .state(BatteryState::Charged(75));

        assert_eq!(battery.width, Length::Fixed(80.0));
        assert_eq!(battery.height, Length::Fixed(40.0));
        assert_eq!(battery.state, BatteryState::Charged(75));
    }

    // Test Appearance struct
    #[test]
    fn test_appearance_default() {
        let appearance = Appearance::default();
        assert_eq!(
            appearance.background_color,
            Color::from_rgba(0.1, 0.1, 0.1, 1.0)
        );
        assert_eq!(appearance.border_color, Color::WHITE);
    }

    #[test]
    fn test_appearance_default_charging_color() {
        let appearance = Appearance::default();
        assert_eq!(appearance.charging_color, Color::from_rgb(0.2, 0.8, 0.2));
    }

    #[test]
    fn test_appearance_default_charge_colors() {
        let appearance = Appearance::default();
        // High color is green
        assert_eq!(appearance.charge_high_color, Color::from_rgb(0.2, 0.8, 0.2));
        // Medium color is yellow
        assert_eq!(
            appearance.charge_medium_color,
            Color::from_rgb(0.95, 0.9, 0.2)
        );
        // Low color is red
        assert_eq!(appearance.charge_low_color, Color::from_rgb(0.9, 0.2, 0.2));
    }

    #[test]
    fn test_appearance_default_unknown_color() {
        let appearance = Appearance::default();
        assert_eq!(appearance.unknown_color, Color::from_rgb(0.7, 0.7, 0.7));
    }

    #[test]
    fn test_appearance_custom() {
        let appearance = Appearance {
            background_color: Color::BLACK,
            border_color: Color::WHITE,
            charging_color: Color::from_rgb(0.0, 1.0, 0.0),
            charge_high_color: Color::from_rgb(0.0, 1.0, 0.0),
            charge_medium_color: Color::from_rgb(1.0, 1.0, 0.0),
            charge_low_color: Color::from_rgb(1.0, 0.0, 0.0),
            unknown_color: Color::from_rgb(0.5, 0.5, 0.5),
        };
        assert_eq!(appearance.background_color, Color::BLACK);
        assert_eq!(appearance.border_color, Color::WHITE);
    }

    #[test]
    fn test_appearance_clone() {
        let appearance = Appearance::default();
        let cloned = appearance;
        assert_eq!(cloned.background_color, appearance.background_color);
        assert_eq!(cloned.border_color, appearance.border_color);
    }

    #[test]
    fn test_appearance_debug() {
        let appearance = Appearance::default();
        let debug_str = format!("{:?}", appearance);
        assert!(debug_str.contains("Appearance"));
        assert!(debug_str.contains("background_color"));
        assert!(debug_str.contains("border_color"));
    }

    // Test StyleSheet implementation for iced::Theme
    #[test]
    fn test_theme_stylesheet_appearance() {
        let theme = iced::Theme::Dark;
        let appearance = theme.appearance(&());
        // Verify colors are set
        assert_eq!(
            appearance.background_color,
            Color::from_rgb(0.05, 0.05, 0.05)
        );
        assert_eq!(appearance.border_color, Color::from_rgb(0.4, 0.4, 0.4));
    }

    #[test]
    fn test_theme_stylesheet_charging_color() {
        let theme = iced::Theme::Dark;
        let appearance = theme.appearance(&());
        assert_eq!(appearance.charging_color, Color::from_rgb(0.0, 0.7, 0.0));
    }

    #[test]
    fn test_theme_stylesheet_charge_colors() {
        let theme = iced::Theme::Dark;
        let appearance = theme.appearance(&());
        assert_eq!(appearance.charge_high_color, Color::from_rgb(0.0, 0.7, 0.0));
        assert_eq!(
            appearance.charge_medium_color,
            Color::from_rgb(0.8, 0.7, 0.0)
        );
        assert_eq!(appearance.charge_low_color, Color::from_rgb(0.8, 0.0, 0.0));
    }

    #[test]
    fn test_theme_stylesheet_unknown_color() {
        let theme = iced::Theme::Dark;
        let appearance = theme.appearance(&());
        assert_eq!(appearance.unknown_color, Color::from_rgb(0.5, 0.5, 0.5));
    }

    #[test]
    fn test_theme_stylesheet_light() {
        let theme = iced::Theme::Light;
        let appearance = theme.appearance(&());
        // Light theme should also return valid colors
        assert_eq!(
            appearance.background_color,
            Color::from_rgb(0.05, 0.05, 0.05)
        );
    }

    // Test Widget trait methods
    #[test]
    fn test_battery_size() {
        use iced::advanced::Widget;

        let battery: Battery<iced::Theme> = Battery::new()
            .width(Length::Fixed(50.0))
            .height(Length::Fixed(25.0));

        let size =
            <Battery<iced::Theme> as Widget<(), iced::Theme, iced::Renderer>>::size(&battery);
        assert_eq!(size.width, Length::Fixed(50.0));
        assert_eq!(size.height, Length::Fixed(25.0));
    }

    #[test]
    fn test_battery_size_default() {
        use iced::advanced::Widget;

        let battery: Battery<iced::Theme> = Battery::new();

        let size =
            <Battery<iced::Theme> as Widget<(), iced::Theme, iced::Renderer>>::size(&battery);
        assert_eq!(size.width, Length::Fixed(40.0));
        assert_eq!(size.height, Length::Fixed(20.0));
    }
}
