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

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{self, Clipboard, Layout, Shell, Widget};
use iced::border::Radius;
use iced::mouse;
use iced::{Background, Border, Color, Element, Length, Rectangle, Size};

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
    Theme: StyleSheet,
    Renderer: advanced::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
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
            x: bounds.x + bounds.width - terminal_width,
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

        match self.state {
            BatteryState::Charging => {
                // Draw charging icon (lightning bolt)
                let icon_size = bounds.height * 0.6;
                let icon_x = bounds.x + (battery_body.width - icon_size) / 2.0;
                let icon_y = bounds.y + (bounds.height - icon_size) / 2.0;

                // Draw a simple lightning bolt shape using two quads
                // Top part of the lightning bolt
                let top_part = Rectangle {
                    x: icon_x + icon_size * 0.3,
                    y: icon_y,
                    width: icon_size * 0.2,
                    height: icon_size * 0.5,
                };

                // Bottom part of lightning bolt
                let bottom_part = Rectangle {
                    x: icon_x + icon_size * 0.2,
                    y: icon_y + icon_size * 0.4,
                    width: icon_size * 0.2,
                    height: icon_size * 0.6,
                };

                renderer.fill_quad(
                    renderer::Quad {
                        bounds: top_part,
                        ..renderer::Quad::default()
                    },
                    Background::Color(custom_style.charging_color),
                );

                renderer.fill_quad(
                    renderer::Quad {
                        bounds: bottom_part,
                        ..renderer::Quad::default()
                    },
                    Background::Color(custom_style.charging_color),
                );
            }
            BatteryState::Charged(percentage) => {
                // Clamp percentage to 0-100
                let percentage = percentage.min(100);

                // Calculate charge fill width
                let padding = border_width + 2.0;
                let fill_width = (battery_body.width - padding * 2.0) * (percentage as f32 / 100.0);

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
                        x: bounds.x + padding,
                        y: bounds.y + padding,
                        width: fill_width,
                        height: bounds.height - padding * 2.0,
                    };

                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: charge_fill,
                            border: Border {
                                radius: Radius::from(1.0),
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

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<()>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(())
    }

    fn on_event(
        &mut self,
        _tree: &mut Tree,
        _event: iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> iced::event::Status {
        iced::event::Status::Ignored
    }
}

impl<'a, Message, Theme, Renderer> From<Battery<Theme>> for Element<'a, Message, Theme, Renderer>
where
    Theme: StyleSheet + 'a,
    Renderer: advanced::Renderer + 'a,
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
        // Use dark theme colors for the battery widget
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
