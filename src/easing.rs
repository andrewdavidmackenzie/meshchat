use iced::Point;
use std::sync::OnceLock;

use lyon_algorithms::measure::PathMeasurements;
use lyon_algorithms::path::{Path, builder::NoAttributes, path::BuilderImpl};

pub static STANDARD: OnceLock<Easing> = OnceLock::new();

pub fn standard() -> &'static Easing {
    STANDARD.get_or_init(|| {
        Easing::builder()
            .cubic_bezier_to([0.2, 0.0], [0.0, 1.0], [1.0, 1.0])
            .build()
    })
}

pub static EMPHASIZED_ACCELERATE: OnceLock<Easing> = OnceLock::new();

pub fn emphasized_accelerate() -> &'static Easing {
    EMPHASIZED_ACCELERATE.get_or_init(|| {
        Easing::builder()
            .cubic_bezier_to([0.3, 0.0], [0.8, 0.15], [1.0, 1.0])
            .build()
    })
}

pub struct Easing {
    path: Path,
    measurements: PathMeasurements,
}

impl Easing {
    pub fn builder() -> Builder {
        Builder::new()
    }

    pub fn y_at_x(&self, x: f32) -> f32 {
        let mut sampler = self
            .measurements
            .create_sampler(&self.path, lyon_algorithms::measure::SampleType::Normalized);
        let sample = sampler.sample(x);

        sample.position().y
    }
}

pub struct Builder(NoAttributes<BuilderImpl>);

impl Builder {
    pub fn new() -> Self {
        let mut builder = Path::builder();
        builder.begin(lyon_algorithms::geom::point(0.0, 0.0));

        Self(builder)
    }

    /// Adds a line segment. Points must be between 0,0 and 1,1
    pub fn line_to(mut self, to: impl Into<Point>) -> Self {
        self.0.line_to(Self::point(to));

        self
    }

    /// Adds a quadratic bézier curve. Points must be between 0,0 and 1,1
    pub fn quadratic_bezier_to(mut self, ctrl: impl Into<Point>, to: impl Into<Point>) -> Self {
        self.0
            .quadratic_bezier_to(Self::point(ctrl), Self::point(to));

        self
    }

    /// Adds a cubic bézier curve. Points must be between 0,0 and 1,1
    pub fn cubic_bezier_to(
        mut self,
        ctrl1: impl Into<Point>,
        ctrl2: impl Into<Point>,
        to: impl Into<Point>,
    ) -> Self {
        self.0
            .cubic_bezier_to(Self::point(ctrl1), Self::point(ctrl2), Self::point(to));

        self
    }

    pub fn build(mut self) -> Easing {
        self.0.line_to(lyon_algorithms::geom::point(1.0, 1.0));
        self.0.end(false);

        let path = self.0.build();
        let measurements = PathMeasurements::from_path(&path, 0.0);

        Easing { path, measurements }
    }

    fn point(p: impl Into<Point>) -> lyon_algorithms::geom::Point<f32> {
        let p: Point = p.into();
        lyon_algorithms::geom::point(p.x.clamp(0.0, 1.0), p.y.clamp(0.0, 1.0))
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_default() {
        let builder = Builder::default();
        let easing = builder.build();
        // Should be able to query the easing curve
        let y = easing.y_at_x(0.5);
        assert!(y >= 0.0 && y <= 1.0);
    }

    #[test]
    fn test_linear_easing() {
        // Build a linear easing (straight line from 0,0 to 1,1)
        let easing = Easing::builder().line_to([1.0, 1.0]).build();

        // At x=0, y should be close to 0
        let y_start = easing.y_at_x(0.0);
        assert!(y_start < 0.1, "y at x=0 should be near 0, got {}", y_start);

        // At x=1, y should be close to 1
        let y_end = easing.y_at_x(1.0);
        assert!(y_end > 0.9, "y at x=1 should be near 1, got {}", y_end);

        // At x=0.5, y should be around 0.5 for a linear curve
        let y_mid = easing.y_at_x(0.5);
        assert!(
            y_mid > 0.4 && y_mid < 0.6,
            "y at x=0.5 should be near 0.5, got {}",
            y_mid
        );
    }

    #[test]
    fn test_standard_easing() {
        let easing = standard();
        // Standard easing should return values in valid range
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let y = easing.y_at_x(x);
            assert!(
                y >= 0.0 && y <= 1.0,
                "y should be between 0 and 1 at x={}, got {}",
                x,
                y
            );
        }
    }

    #[test]
    fn test_emphasized_accelerate_easing() {
        let easing = emphasized_accelerate();
        // Should return values in valid range
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let y = easing.y_at_x(x);
            assert!(
                y >= 0.0 && y <= 1.0,
                "y should be between 0 and 1 at x={}, got {}",
                x,
                y
            );
        }
    }

    #[test]
    fn test_quadratic_bezier() {
        let easing = Easing::builder()
            .quadratic_bezier_to([0.5, 0.5], [1.0, 1.0])
            .build();

        let y = easing.y_at_x(0.5);
        assert!(y >= 0.0 && y <= 1.0);
    }

    #[test]
    fn test_cubic_bezier() {
        let easing = Easing::builder()
            .cubic_bezier_to([0.25, 0.1], [0.25, 1.0], [1.0, 1.0])
            .build();

        let y = easing.y_at_x(0.5);
        assert!(y >= 0.0 && y <= 1.0);
    }

    #[test]
    fn test_points_clamped_to_unit_square() {
        // Points outside 0-1 range should be clamped
        let easing = Easing::builder()
            .line_to([2.0, 2.0]) // Should be clamped to 1.0, 1.0
            .build();

        let y = easing.y_at_x(0.5);
        assert!(y >= 0.0 && y <= 1.0);
    }

    #[test]
    fn test_static_once_lock_standard() {
        // Test that the OnceLock works correctly - calling twice should return same reference
        let first = standard();
        let second = standard();
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn test_static_once_lock_emphasized() {
        let first = emphasized_accelerate();
        let second = emphasized_accelerate();
        assert!(std::ptr::eq(first, second));
    }
}
