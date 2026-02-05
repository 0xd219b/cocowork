//! Color definitions for CocoWork theme
//!
//! Colors extracted from design specification and cocowork-index.png

/// RGBA color representation
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    /// Create a new RGBA color from 0-255 values
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Create from hex value (0xRRGGBBAA)
    pub const fn from_hex(hex: u32) -> Self {
        Self::new(
            ((hex >> 24) & 0xFF) as u8,
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    }

    /// Create opaque color from RGB hex (0xRRGGBB)
    pub const fn rgb(hex: u32) -> Self {
        Self::new(
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
            255,
        )
    }

    /// Create with alpha
    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: alpha,
        }
    }

    /// Convert to array [r, g, b, a]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Theme color palette
#[derive(Debug, Clone)]
pub struct ThemeColors {
    // === Backgrounds ===
    /// Deep sidebar background
    pub sidebar_bg: Rgba,
    /// Panel background
    pub panel_bg: Rgba,
    /// Surface/card background
    pub surface: Rgba,
    /// Elevated surface
    pub surface_elevated: Rgba,
    /// Input field background
    pub input_bg: Rgba,

    // === Brand Colors ===
    /// Primary brand color (coconut tree green)
    pub primary: Rgba,
    /// Primary hover state
    pub primary_hover: Rgba,
    /// Accent color (coconut orange)
    pub accent: Rgba,
    /// Accent hover state
    pub accent_hover: Rgba,

    // === Text Colors ===
    /// Primary text
    pub text_primary: Rgba,
    /// Secondary/muted text
    pub text_secondary: Rgba,
    /// Disabled text
    pub text_disabled: Rgba,
    /// Link text
    pub text_link: Rgba,

    // === Status Colors ===
    /// Success state
    pub success: Rgba,
    /// Warning state
    pub warning: Rgba,
    /// Error state
    pub error: Rgba,
    /// Info state
    pub info: Rgba,

    // === UI Elements ===
    /// Border color
    pub border: Rgba,
    /// Border subtle
    pub border_subtle: Rgba,
    /// Divider color
    pub divider: Rgba,
    /// Selection/highlight
    pub selection: Rgba,
    /// Hover state
    pub hover: Rgba,
    /// Focus ring
    pub focus_ring: Rgba,

    // === Syntax/Code ===
    /// Code background
    pub code_bg: Rgba,
    /// Code text
    pub code_text: Rgba,
}

impl ThemeColors {
    /// Create the dark theme color palette
    pub fn dark() -> Self {
        Self {
            // Backgrounds (from design)
            sidebar_bg: Rgba::rgb(0x1a1e2a),      // Deep blue-gray sidebar
            panel_bg: Rgba::rgb(0x282c34),        // Panel background
            surface: Rgba::rgb(0x21252b),         // Surface color
            surface_elevated: Rgba::rgb(0x2c313a), // Elevated surface
            input_bg: Rgba::rgb(0x1e2228),        // Input background

            // Brand colors
            primary: Rgba::rgb(0x2d8f6f),         // Coconut tree green
            primary_hover: Rgba::rgb(0x3aa882),   // Primary hover
            accent: Rgba::rgb(0xe8845c),          // Coconut orange
            accent_hover: Rgba::rgb(0xf09570),    // Accent hover

            // Text colors
            text_primary: Rgba::rgb(0xeceff4),    // Primary text
            text_secondary: Rgba::rgb(0x8b949e),  // Secondary text
            text_disabled: Rgba::rgb(0x6b7b76),   // Disabled text
            text_link: Rgba::rgb(0x58a6ff),       // Link color

            // Status colors
            success: Rgba::rgb(0x3fb950),         // Green
            warning: Rgba::rgb(0xd29922),         // Yellow/orange
            error: Rgba::rgb(0xf85149),           // Red
            info: Rgba::rgb(0x58a6ff),            // Blue

            // UI Elements
            border: Rgba::rgb(0x4a5260),          // Border color (increased contrast)
            border_subtle: Rgba::rgb(0x3b4048),   // Subtle border
            divider: Rgba::rgb(0x30363d),         // Divider
            selection: Rgba::from_hex(0x388bfd33), // Selection with transparency
            hover: Rgba::from_hex(0xb1bac420),    // Hover state
            focus_ring: Rgba::rgb(0x58a6ff),      // Focus ring

            // Code
            code_bg: Rgba::rgb(0x161b22),         // Code background
            code_text: Rgba::rgb(0xe6edf3),       // Code text
        }
    }
}

// === Predefined Colors ===

/// Transparent color
pub const TRANSPARENT: Rgba = Rgba::new(0, 0, 0, 0);

/// White color
pub const WHITE: Rgba = Rgba::new(255, 255, 255, 255);

/// Black color
pub const BLACK: Rgba = Rgba::new(0, 0, 0, 255);
