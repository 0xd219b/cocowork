//! CocoWork Desktop Application
//!
//! GPUI-based desktop client for interacting with AI coding agents via ACP.

use cocowork_ui::components::register_text_input_bindings;
use cocowork_ui::Theme;
use gpui::*;
use std::borrow::Cow;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod window;

use window::CocoWorkWindow;

/// Asset source that loads from the filesystem relative to the executable or current directory
struct FileAssetSource {
    base_path: PathBuf,
}

impl FileAssetSource {
    fn new() -> Self {
        // Try to find assets directory relative to executable or current directory
        let base_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        // Check common locations for assets
        let candidates = [
            base_path.join("assets"),
            PathBuf::from("assets"),
            base_path.join("../assets"),
            base_path.join("../../assets"),
        ];

        let base_path = candidates
            .into_iter()
            .find(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from("assets"));

        info!("Asset base path: {:?}", base_path);
        Self { base_path }
    }
}

impl AssetSource for FileAssetSource {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let full_path = self.base_path.join(path);
        match std::fs::read(&full_path) {
            Ok(bytes) => Ok(Some(Cow::Owned(bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!("Asset not found: {:?}", full_path);
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let full_path = self.base_path.join(path);
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(&full_path) {
            for entry in dir.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    entries.push(name.to_string().into());
                }
            }
        }
        Ok(entries)
    }
}

fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("CocoWork v{}", env!("CARGO_PKG_VERSION"));

    // Start GPUI application with asset loading
    App::new()
        .with_assets(FileAssetSource::new())
        .run(|cx: &mut AppContext| {
        // Register key bindings for text input
        register_text_input_bindings(cx);

        // Initialize theme
        let theme = Theme::dark();
        info!("Theme initialized: dark mode");

        // Open main window
        let window_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("CocoWork".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.0), px(9.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(100.0), px(100.0)),
                size: size(px(1200.0), px(800.0)),
            })),
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            is_movable: true,
            window_background: WindowBackgroundAppearance::Opaque,
            app_id: Some("com.cocowork.app".to_string()),
            ..Default::default()
        };

        cx.open_window(window_options, |cx| {
            cx.new_view(|cx| CocoWorkWindow::new(cx, theme))
        })
        .unwrap();
    });
}
