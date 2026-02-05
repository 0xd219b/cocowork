//! Theme system for CocoWork
//!
//! Provides color definitions and styling based on the design specification.

mod colors;

pub use colors::*;

/// Theme configuration
#[derive(Debug, Clone)]
pub struct Theme {
    pub colors: ThemeColors,
    pub spacing: Spacing,
    pub typography: Typography,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Create the default dark theme
    pub fn dark() -> Self {
        Self {
            colors: ThemeColors::dark(),
            spacing: Spacing::default(),
            typography: Typography::default(),
        }
    }
}

/// Spacing constants
#[derive(Debug, Clone)]
pub struct Spacing {
    /// Extra small spacing (4px)
    pub xs: f32,
    /// Small spacing (8px)
    pub sm: f32,
    /// Medium spacing (12px)
    pub md: f32,
    /// Large spacing (16px)
    pub lg: f32,
    /// Extra large spacing (24px)
    pub xl: f32,
    /// Extra extra large spacing (32px)
    pub xxl: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
            xl: 24.0,
            xxl: 32.0,
        }
    }
}

/// Typography settings
#[derive(Debug, Clone)]
pub struct Typography {
    /// Base font size
    pub base_size: f32,
    /// Small font size
    pub small_size: f32,
    /// Large font size
    pub large_size: f32,
    /// Header font size
    pub header_size: f32,
    /// Default line height multiplier
    pub line_height: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            base_size: 14.0,
            small_size: 12.0,
            large_size: 16.0,
            header_size: 18.0,
            line_height: 1.5,
        }
    }
}

/// Layout constants
pub mod layout {
    /// Sidebar width in pixels
    pub const SIDEBAR_WIDTH: f32 = 220.0;
    /// Context panel width in pixels
    pub const CONTEXT_PANEL_WIDTH: f32 = 280.0;
    /// Header height in pixels
    pub const HEADER_HEIGHT: f32 = 48.0;
    /// Input bar height in pixels
    pub const INPUT_BAR_HEIGHT: f32 = 56.0;
    /// Tree item height in pixels
    pub const TREE_ITEM_HEIGHT: f32 = 28.0;
    /// Border radius
    pub const BORDER_RADIUS: f32 = 6.0;
    /// Border radius small
    pub const BORDER_RADIUS_SM: f32 = 4.0;
}
