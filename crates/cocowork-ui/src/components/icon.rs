//! SVG Icon component for consistent icon rendering
//!
//! Uses Zed's icon system with proper SVG rendering via GPUI.
//! Icons are stored in assets/icons/ as SVG files.

use gpui::*;

/// Icon names corresponding to SVG files in assets/icons/
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    // Chevrons for expand/collapse
    ChevronDown,
    ChevronRight,
    ChevronUp,
    ChevronLeft,

    // Arrows
    ArrowUp,

    // Status indicators
    Check,
    Close,
    Circle,
    CircleCheck,

    // Actions
    Settings,
    Pencil,

    // Files
    File,
    Folder,
    Plus,

    // Tools
    Terminal,
    Search,
    Web,
    Play,

    // Agents
    AiClaude,
    AiGemini,
    Agent,

    // Communication
    Chat,

    // CocoWork brand
    Coconut,
}

impl IconName {
    /// Get the path to the SVG file
    pub fn path(&self) -> &'static str {
        match self {
            IconName::ChevronDown => "icons/chevron_down.svg",
            IconName::ChevronRight => "icons/chevron_right.svg",
            IconName::ChevronUp => "icons/chevron_up.svg",
            IconName::ChevronLeft => "icons/chevron_left.svg",
            IconName::ArrowUp => "icons/arrow_up.svg",
            IconName::Check => "icons/check.svg",
            IconName::Close => "icons/close.svg",
            IconName::Circle => "icons/circle.svg",
            IconName::CircleCheck => "icons/circle_check.svg",
            IconName::Settings => "icons/settings.svg",
            IconName::Pencil => "icons/pencil.svg",
            IconName::File => "icons/file.svg",
            IconName::Folder => "icons/folder.svg",
            IconName::Plus => "icons/plus.svg",
            IconName::Terminal => "icons/terminal.svg",
            IconName::Search => "icons/magnifying_glass.svg",
            IconName::Web => "icons/tool_web.svg",
            IconName::Play => "icons/play_outlined.svg",
            IconName::AiClaude => "icons/ai_claude.svg",
            IconName::AiGemini => "icons/ai_gemini.svg",
            IconName::Agent => "icons/zed_agent.svg",
            IconName::Chat => "icons/chat.svg",
            IconName::Coconut => "icons/coconut.svg",
        }
    }
}

/// Standard icon sizes
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum IconSize {
    /// Extra small: 12px
    XSmall,
    /// Small: 14px
    Small,
    /// Medium: 16px (default)
    #[default]
    Medium,
    /// Large: 20px
    Large,
}

impl IconSize {
    /// Get the size in pixels
    pub fn px(&self) -> f32 {
        match self {
            IconSize::XSmall => 12.0,
            IconSize::Small => 14.0,
            IconSize::Medium => 16.0,
            IconSize::Large => 20.0,
        }
    }

    /// Get the size in rems for GPUI
    pub fn rems(&self) -> Rems {
        rems(self.px() / 16.0)
    }
}

/// Create an SVG icon element
///
/// # Example
/// ```ignore
/// svg_icon(IconName::ChevronDown, IconSize::Small)
///     .text_color(rgb(colors.text_secondary))
/// ```
pub fn svg_icon(name: IconName, size: IconSize) -> Svg {
    let px_size = size.px();
    // Load SVG via AssetSource (path relative to assets directory)
    svg()
        .path(name.path())
        .size(px(px_size))
        .flex_shrink_0()
}

/// Convenience function for status indicators
pub mod status {
    use super::*;

    pub fn pending(size: IconSize) -> Svg {
        svg_icon(IconName::Circle, size)
    }

    pub fn in_progress(size: IconSize) -> Svg {
        // Use circle with different styling for in-progress
        svg_icon(IconName::Circle, size)
    }

    pub fn completed(size: IconSize) -> Svg {
        svg_icon(IconName::Check, size)
    }

    pub fn failed(size: IconSize) -> Svg {
        svg_icon(IconName::Close, size)
    }

    pub fn cancelled(size: IconSize) -> Svg {
        svg_icon(IconName::Close, size)
    }
}

/// Convenience function for chevron icons
pub mod chevron {
    use super::*;

    pub fn down(size: IconSize) -> Svg {
        svg_icon(IconName::ChevronDown, size)
    }

    pub fn right(size: IconSize) -> Svg {
        svg_icon(IconName::ChevronRight, size)
    }

    pub fn up(size: IconSize) -> Svg {
        svg_icon(IconName::ChevronUp, size)
    }

    pub fn left(size: IconSize) -> Svg {
        svg_icon(IconName::ChevronLeft, size)
    }
}

/// Convenience function for agent icons
pub mod agent {
    use super::*;

    pub fn claude(size: IconSize) -> Svg {
        svg_icon(IconName::AiClaude, size)
    }

    pub fn gemini(size: IconSize) -> Svg {
        svg_icon(IconName::AiGemini, size)
    }

    pub fn default(size: IconSize) -> Svg {
        svg_icon(IconName::Agent, size)
    }

    pub fn chat(size: IconSize) -> Svg {
        svg_icon(IconName::Chat, size)
    }
}

/// Convenience function for tool icons
pub mod tool {
    use super::*;

    pub fn file(size: IconSize) -> Svg {
        svg_icon(IconName::File, size)
    }

    pub fn edit(size: IconSize) -> Svg {
        svg_icon(IconName::Pencil, size)
    }

    pub fn terminal(size: IconSize) -> Svg {
        svg_icon(IconName::Terminal, size)
    }

    pub fn search(size: IconSize) -> Svg {
        svg_icon(IconName::Search, size)
    }

    pub fn web(size: IconSize) -> Svg {
        svg_icon(IconName::Web, size)
    }

    pub fn play(size: IconSize) -> Svg {
        svg_icon(IconName::Play, size)
    }
}

// Keep the old icon() function for backward compatibility during migration
// This can be removed once all usages are migrated to svg_icon()

/// Old Unicode icons module (deprecated, use svg_icon instead)
#[deprecated(note = "Use svg_icon() with IconName instead")]
pub mod icons {
    // Status indicators
    pub const PENDING: &str = "○";
    pub const IN_PROGRESS: &str = "◐";
    pub const COMPLETED: &str = "●";
    pub const FAILED: &str = "✕";
    pub const CANCELLED: &str = "⊘";

    // Arrows and chevrons
    pub const ARROW_UP: &str = "↑";
    pub const CHEVRON_DOWN: &str = "▾";
    pub const CHEVRON_RIGHT: &str = "▸";

    // Actions
    pub const CHECK: &str = "✓";
    pub const CLOSE: &str = "✕";
    pub const SETTINGS: &str = "⚙";

    // Tool kinds
    pub const FILE_READ: &str = "◇";
    pub const FILE_WRITE: &str = "◆";
    pub const FILE_EDIT: &str = "◈";
    pub const EXECUTE: &str = "▶";
    pub const SEARCH: &str = "◎";
    pub const FETCH: &str = "⇄";
    pub const TASK: &str = "☐";
    pub const PLAN: &str = "☰";
    pub const THINK: &str = "◌";
    pub const TOOL: &str = "⚒";

    // Agents
    pub const AGENT_CLAUDE: &str = "◉";
    pub const AGENT_GEMINI: &str = "◈";
    pub const AGENT_CODEX: &str = "◇";
    pub const CHAT: &str = "◬";
}

/// Old fixed-width icon function (deprecated)
#[deprecated(note = "Use svg_icon() instead")]
#[allow(deprecated)]
pub fn icon(symbol: &str, size: crate::components::icon::OldIconSize) -> Div {
    let container_w = size.container_width();
    div()
        .w(px(container_w))
        .h(px(container_w))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .child(symbol.to_string())
}

/// Old icon size enum (deprecated)
#[deprecated(note = "Use IconSize instead")]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum OldIconSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
}

#[allow(deprecated)]
impl OldIconSize {
    pub fn container_width(&self) -> f32 {
        match self {
            OldIconSize::Xs => 14.0,
            OldIconSize::Sm => 16.0,
            OldIconSize::Md => 18.0,
            OldIconSize::Lg => 22.0,
        }
    }
}
