//! Show a linear progress indicator.
use iced::advanced::layout;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{self, Clipboard, Layout, Shell, Widget};
use iced::mouse;
use iced::time::Instant;
use iced::window::{self};
use iced::{Background, Color, Element, Event, Length, Rectangle, Size, Theme};

use super::easing::{self, Easing};

use crate::styles::CYAN;
use std::time::Duration;

#[allow(missing_debug_implementations)]
pub struct Linear<'a, Theme = iced::Theme>
where
    Theme: Catalog,
{
    width: Length,
    height: Length,
    easing: &'a Easing,
    cycle_duration: Duration,
    class: Theme::Class<'a>,
}

impl<'a, Theme> Linear<'a, Theme>
where
    Theme: Catalog,
{
    /// Creates a new [`Linear`] with the given content.
    pub fn new() -> Self {
        Linear {
            width: Length::Fixed(100.0),
            height: Length::Fixed(4.0),
            easing: easing::standard(),
            cycle_duration: Duration::from_millis(500),
            class: Theme::default(),
        }
    }

    /// Sets the width of the [`Linear`].
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Linear`].
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the style of the [`Linear`].
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the [`Linear`].
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }

    /// Sets the motion easing of this [`Linear`].
    pub fn easing(mut self, easing: &'a Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Sets the cycle duration of this [`Linear`].
    pub fn cycle_duration(mut self, duration: Duration) -> Self {
        self.cycle_duration = duration / 2;
        self
    }
}

impl<Theme> Default for Linear<'_, Theme>
where
    Theme: Catalog,
{
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
enum State {
    Expanding { start: Instant, progress: f32 },
    Contracting { start: Instant, progress: f32 },
}

impl Default for State {
    fn default() -> Self {
        Self::Expanding {
            start: Instant::now(),
            progress: 0.0,
        }
    }
}

impl State {
    fn next(&self, now: Instant) -> Self {
        match self {
            Self::Expanding { .. } => Self::Contracting {
                start: now,
                progress: 0.0,
            },
            Self::Contracting { .. } => Self::Expanding {
                start: now,
                progress: 0.0,
            },
        }
    }

    fn start(&self) -> Instant {
        match self {
            Self::Expanding { start, .. } | Self::Contracting { start, .. } => *start,
        }
    }

    fn timed_transition(&self, cycle_duration: Duration, now: Instant) -> Self {
        let elapsed = now.duration_since(self.start());

        match elapsed {
            elapsed if elapsed > cycle_duration => self.next(now),
            _ => self.with_elapsed(cycle_duration, elapsed),
        }
    }

    fn with_elapsed(&self, cycle_duration: Duration, elapsed: Duration) -> Self {
        let progress = elapsed.as_secs_f32() / cycle_duration.as_secs_f32();
        match self {
            Self::Expanding { start, .. } => Self::Expanding {
                start: *start,
                progress,
            },
            Self::Contracting { start, .. } => Self::Contracting {
                start: *start,
                progress,
            },
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Linear<'_, Theme>
where
    Message: Clone,
    Theme: Catalog,
    Renderer: advanced::Renderer,
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
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let custom_style = theme.style(&self.class);
        let state = tree.state.downcast_ref::<State>();

        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: bounds.x,
                    y: bounds.y,
                    width: bounds.width,
                    height: bounds.height,
                },
                ..Quad::default()
            },
            Background::Color(custom_style.track_color),
        );

        match state {
            State::Expanding { progress, .. } => renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x,
                        y: bounds.y,
                        width: self.easing.y_at_x(*progress) * bounds.width,
                        height: bounds.height,
                    },
                    ..Quad::default()
                },
                Background::Color(custom_style.bar_color),
            ),

            State::Contracting { progress, .. } => renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x + self.easing.y_at_x(*progress) * bounds.width,
                        y: bounds.y,
                        width: (1.0 - self.easing.y_at_x(*progress)) * bounds.width,
                        height: bounds.height,
                    },
                    ..Quad::default()
                },
                Background::Color(custom_style.bar_color),
            ),
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            *state = state.timed_transition(self.cycle_duration, *now);

            shell.request_redraw();
        }
    }
}

impl<'a, Message, Theme, Renderer> From<Linear<'a, Theme>> for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: Catalog + 'a,
    Renderer: advanced::Renderer + 'a,
{
    fn from(linear: Linear<'a, Theme>) -> Self {
        Self::new(linear)
    }
}

/// The style of a linear progress indicator.
///
/// If not specified with [`Linear::style`], then the theme will provide the style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// The track [`Color`] of the progress indicator.
    pub track_color: Color,
    /// The bar [`Color`] of the progress indicator.
    pub bar_color: Color,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            track_color: Color::TRANSPARENT,
            bar_color: Color::BLACK,
        }
    }
}

/// The theme catalog of a [`Linear`].
///
/// All themes that can be used with [`Linear`]
/// must implement this trait.
///
/// # Example
/// ```no_run
/// # use iced::Color;
/// # use meshchat::linear::{Catalog, Style};
/// # struct MyTheme;
/// #[derive(Debug, Default, Clone)]
/// pub enum LinearClass {
///     #[default]
///     Default,
///     Accent,
/// }
///
/// impl Catalog for MyTheme {
///     type Class<'a> = LinearClass;
///
///     fn default<'a>() -> Self::Class<'a> {
///         LinearClass::default()
///     }
///
///     fn style(&self, class: &Self::Class<'_>) -> Style {
///         match class {
///             LinearClass::Default => Style::default(),
///             LinearClass::Accent => Style {
///                 bar_color: Color::from_rgb(0.0, 0.8, 0.8),
///                 ..Style::default()
///             },
///         }
///     }
/// }
/// ```
pub trait Catalog {
    /// The item class of the [`Catalog`].
    type Class<'a>;

    /// The default class produced by the [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class.
    fn style(&self, class: &Self::Class<'_>) -> Style;
}

/// A styling function for a [`Linear`].
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

/// The default linear progress indicator style.
pub fn default(theme: &Theme) -> Style {
    let palette = theme.extended_palette();

    Style {
        track_color: palette.background.weak.color,
        bar_color: CYAN,
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    // Test State enum
    #[test]
    fn test_state_default_is_expanding() {
        let state = State::default();
        assert!(matches!(state, State::Expanding { progress, .. } if progress == 0.0));
    }

    #[test]
    fn test_state_expanding_next_is_contracting() {
        let state = State::Expanding {
            start: Instant::now(),
            progress: 0.5,
        };
        let next = state.next(Instant::now());
        assert!(matches!(next, State::Contracting { progress, .. } if progress == 0.0));
    }

    #[test]
    fn test_state_contracting_next_is_expanding() {
        let state = State::Contracting {
            start: Instant::now(),
            progress: 0.5,
        };
        let next = state.next(Instant::now());
        assert!(matches!(next, State::Expanding { progress, .. } if progress == 0.0));
    }

    #[test]
    fn test_state_start_expanding() {
        let start_time = Instant::now();
        let state = State::Expanding {
            start: start_time,
            progress: 0.3,
        };
        assert_eq!(state.start(), start_time);
    }

    #[test]
    fn test_state_start_contracting() {
        let start_time = Instant::now();
        let state = State::Contracting {
            start: start_time,
            progress: 0.7,
        };
        assert_eq!(state.start(), start_time);
    }

    #[test]
    fn test_state_with_elapsed_expanding() {
        let start_time = Instant::now();
        let state = State::Expanding {
            start: start_time,
            progress: 0.0,
        };
        let cycle_duration = Duration::from_millis(1000);
        let elapsed = Duration::from_millis(500);
        let new_state = state.with_elapsed(cycle_duration, elapsed);

        assert!(
            matches!(new_state, State::Expanding { start, progress } if start == start_time && (progress - 0.5).abs() < 0.01),
            "State should be Expanding with start={:?} and progress near 0.5, got {:?}",
            start_time,
            new_state
        );
    }

    #[test]
    fn test_state_with_elapsed_contracting() {
        let start_time = Instant::now();
        let state = State::Contracting {
            start: start_time,
            progress: 0.0,
        };
        let cycle_duration = Duration::from_millis(1000);
        let elapsed = Duration::from_millis(250);
        let new_state = state.with_elapsed(cycle_duration, elapsed);

        assert!(
            matches!(new_state, State::Contracting { start, progress } if start == start_time && (progress - 0.25).abs() < 0.01),
            "State should be Contracting with start={:?} and progress near 0.25, got {:?}",
            start_time,
            new_state
        );
    }

    #[test]
    fn test_state_timed_transition_no_transition() {
        let start_time = Instant::now();
        let state = State::Expanding {
            start: start_time,
            progress: 0.0,
        };
        let cycle_duration = Duration::from_millis(1000);
        // Immediately check - should stay expanding with small progress
        let new_state = state.timed_transition(cycle_duration, start_time);
        assert!(matches!(new_state, State::Expanding { .. }));
    }

    #[test]
    fn test_state_timed_transition_triggers_transition() {
        let start_time = Instant::now();
        let state = State::Expanding {
            start: start_time,
            progress: 0.0,
        };
        let cycle_duration = Duration::from_millis(100);
        // Check after cycle duration has passed
        let later = start_time + Duration::from_millis(150);
        let new_state = state.timed_transition(cycle_duration, later);
        assert!(matches!(new_state, State::Contracting { progress, .. } if progress == 0.0));
    }

    #[test]
    fn test_state_clone() {
        let state = State::Expanding {
            start: Instant::now(),
            progress: 0.5,
        };
        let cloned = state;
        assert!(matches!(cloned, State::Expanding { progress, .. } if progress == 0.5));
    }

    #[test]
    fn test_state_copy() {
        let state = State::Contracting {
            start: Instant::now(),
            progress: 0.75,
        };
        let copied: State = state;
        assert!(matches!(copied, State::Contracting { progress, .. } if progress == 0.75));
        // Original still valid (Copy trait)
        assert!(matches!(state, State::Contracting { progress, .. } if progress == 0.75));
    }

    // Test Style
    #[test]
    fn test_style_default() {
        let style = Style::default();
        assert_eq!(style.track_color, Color::TRANSPARENT);
        assert_eq!(style.bar_color, Color::BLACK);
    }

    #[test]
    fn test_style_custom() {
        let style = Style {
            track_color: Color::WHITE,
            bar_color: Color::from_rgb(1.0, 0.0, 0.0),
        };
        assert_eq!(style.track_color, Color::WHITE);
        assert_eq!(style.bar_color.r, 1.0);
        assert_eq!(style.bar_color.g, 0.0);
        assert_eq!(style.bar_color.b, 0.0);
    }

    #[test]
    fn test_style_clone() {
        let style = Style {
            track_color: Color::WHITE,
            bar_color: Color::BLACK,
        };
        let cloned = style;
        assert_eq!(cloned.track_color, Color::WHITE);
        assert_eq!(cloned.bar_color, Color::BLACK);
    }

    #[test]
    fn test_style_debug() {
        let style = Style::default();
        let debug_str = format!("{:?}", style);
        assert!(debug_str.contains("Style"));
    }

    // Test Catalog implementation for iced::Theme
    #[test]
    fn test_theme_catalog_style() {
        let theme = Theme::Dark;
        let class = <Theme as Catalog>::default();
        let style = theme.style(&class);
        // bar_color should be CYAN
        assert_eq!(style.bar_color, CYAN);
        // track_color should be from the theme palette (not transparent or default)
        assert_ne!(style.track_color, Color::TRANSPARENT);
    }

    #[test]
    fn test_theme_catalog_light() {
        let theme = Theme::Light;
        let class = <Theme as Catalog>::default();
        let style = theme.style(&class);
        assert_eq!(style.bar_color, CYAN);
    }

    // Test style functions
    #[test]
    fn test_default_style_fn() {
        let theme = Theme::Dark;
        let style = default(&theme);
        assert_eq!(style.bar_color, CYAN);
        assert_ne!(style.track_color, Color::TRANSPARENT);
    }

    // Test Linear struct and builder methods
    #[test]
    fn test_linear_new() {
        let linear: Linear<Theme> = Linear::new();
        assert_eq!(linear.width, Length::Fixed(100.0));
        assert_eq!(linear.height, Length::Fixed(4.0));
        assert_eq!(linear.cycle_duration, Duration::from_millis(500));
    }

    #[test]
    fn test_linear_default() {
        let linear: Linear<Theme> = Linear::default();
        assert_eq!(linear.width, Length::Fixed(100.0));
        assert_eq!(linear.height, Length::Fixed(4.0));
    }

    #[test]
    fn test_linear_width() {
        let linear: Linear<Theme> = Linear::new().width(Length::Fixed(200.0));
        assert_eq!(linear.width, Length::Fixed(200.0));
    }

    #[test]
    fn test_linear_width_fill() {
        let linear: Linear<Theme> = Linear::new().width(Length::Fill);
        assert_eq!(linear.width, Length::Fill);
    }

    #[test]
    fn test_linear_height() {
        let linear: Linear<Theme> = Linear::new().height(Length::Fixed(10.0));
        assert_eq!(linear.height, Length::Fixed(10.0));
    }

    #[test]
    fn test_linear_height_shrink() {
        let linear: Linear<Theme> = Linear::new().height(Length::Shrink);
        assert_eq!(linear.height, Length::Shrink);
    }

    #[test]
    fn test_linear_style() {
        let linear: Linear<Theme> = Linear::new().style(|_theme| Style::default());
        // Just verify it compiles and runs - can't easily compare closures
        assert_eq!(linear.width, Length::Fixed(100.0));
    }

    #[test]
    fn test_linear_easing() {
        let custom_easing = easing::emphasized_accelerate();
        let linear: Linear<Theme> = Linear::new().easing(custom_easing);
        // Verify easing was set by checking it's not the default
        assert!(std::ptr::eq(linear.easing, custom_easing));
    }

    #[test]
    fn test_linear_cycle_duration() {
        let linear: Linear<Theme> = Linear::new().cycle_duration(Duration::from_millis(1000));
        // cycle_duration is halved internally
        assert_eq!(linear.cycle_duration, Duration::from_millis(500));
    }

    #[test]
    fn test_linear_cycle_duration_custom() {
        let linear: Linear<Theme> = Linear::new().cycle_duration(Duration::from_secs(2));
        assert_eq!(linear.cycle_duration, Duration::from_secs(1));
    }

    #[test]
    fn test_linear_builder_chain() {
        let linear: Linear<Theme> = Linear::new()
            .width(Length::Fixed(300.0))
            .height(Length::Fixed(8.0))
            .cycle_duration(Duration::from_millis(800));

        assert_eq!(linear.width, Length::Fixed(300.0));
        assert_eq!(linear.height, Length::Fixed(8.0));
        assert_eq!(linear.cycle_duration, Duration::from_millis(400));
    }

    // Test Widget trait methods that don't require renderer
    #[test]
    fn test_linear_size() {
        use iced::advanced::Widget;

        let linear: Linear<Theme> = Linear::new()
            .width(Length::Fixed(150.0))
            .height(Length::Fixed(6.0));

        let size = <Linear<Theme> as Widget<(), Theme, iced::Renderer>>::size(&linear);
        assert_eq!(size.width, Length::Fixed(150.0));
        assert_eq!(size.height, Length::Fixed(6.0));
    }

    #[test]
    fn test_linear_tag() {
        use iced::advanced::Widget;
        use iced::advanced::widget::tree;

        let linear: Linear<Theme> = Linear::new();
        let tag = <Linear<Theme> as Widget<(), Theme, iced::Renderer>>::tag(&linear);
        assert_eq!(tag, tree::Tag::of::<State>());
    }

    #[test]
    fn test_linear_state() {
        use iced::advanced::Widget;

        let linear: Linear<Theme> = Linear::new();
        let state = <Linear<Theme> as Widget<(), Theme, iced::Renderer>>::state(&linear);
        // State should be created (we can't easily inspect it, but it shouldn't panic)
        let _ = state;
    }
}
