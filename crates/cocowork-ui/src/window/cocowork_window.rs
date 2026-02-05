//! Main CocoWork window with three-panel layout
//!
//! Layout based on design: cocowork-index.png
//! - Sidebar (220px): Search + Session list
//! - MainPanel (flex-1): Header + Messages + Input
//! - ContextPanel (280px): State/Artifacts/Context

use cocowork_core::{ContentBlock, MessageBlock, PlanEntry, PlanStatus, ToolCallKind, ToolCallState, ToolCallStatus};
use cocowork_ui::{
    components::{svg_icon, IconName, IconSize, TextInput},
    layout, AcpModel, Rgba as ThemeRgba, Spacing, Theme,
};
use gpui::prelude::FluentBuilder;
use gpui::*;
use markdown::{Markdown, MarkdownStyle};

/// A thread entry in the sidebar
#[derive(Clone, Debug)]
pub struct ThreadEntry {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub message_count: usize,
    pub is_active: bool,
}

impl ThreadEntry {
    pub fn new(id: &str, name: &str, agent_id: &str, message_count: usize) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            agent_id: agent_id.to_string(),
            message_count,
            is_active: false,
        }
    }
}

// ============================================================================
// Window State
// ============================================================================

pub struct CocoWorkWindow {
    theme: Theme,
    acp: AcpModel,
    /// Message input component
    message_input: View<TextInput>,
    /// Search input for filtering threads
    search_input: View<TextInput>,
    /// Thread list for sidebar
    threads: Vec<ThreadEntry>,
    /// Active thread index
    active_thread_idx: Option<usize>,
    /// Expanded sections in context panel
    expanded_sections: Vec<String>,
    /// Focus handle
    focus_handle: FocusHandle,
    /// Current left sidebar width (resizable)
    sidebar_width: f32,
    /// Left sidebar resize drag state
    resizing_sidebar: bool,
    sidebar_resize_start_x: f32,
    sidebar_resize_start_width: f32,
    /// Current right sidebar (context panel) width
    context_panel_width: f32,
    /// Right sidebar resize drag state
    resizing_context_panel: bool,
    context_panel_resize_start_x: f32,
    context_panel_resize_start_width: f32,
    /// Search text
    search_text: String,
    /// Show agent selector dropdown
    show_agent_menu: bool,
    /// Show mode selector dropdown
    show_mode_menu: bool,
    /// Agent workspace path
    workspace_path: Option<String>,
    /// Attached files (uploaded via + button)
    attached_files: Vec<String>,
    /// Show MCP config panel
    show_mcp_panel: bool,
    /// Configured MCP servers
    mcp_servers: Vec<McpServerConfig>,
    /// Collapsed thinking blocks (by message index)
    collapsed_thinking: std::collections::HashSet<usize>,
    /// Scroll handle for message list (auto-scroll)
    message_scroll_handle: ScrollHandle,
    /// Track whether we should keep auto-scrolling to the latest output
    stick_to_bottom: bool,
    /// Cached timeline length for detecting new content
    last_timeline_len: usize,
    /// Cached markdown views for messages
    message_markdown_cache: std::collections::HashMap<String, View<Markdown>>,
    /// Show new thread dialog (with agent selection)
    show_new_thread_dialog: bool,
    /// Show user menu dropdown
    show_user_menu: bool,
}

/// MCP Server configuration
#[derive(Clone, Debug)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub enabled: bool,
}

impl CocoWorkWindow {
    pub fn new(cx: &mut ViewContext<Self>, theme: Theme) -> Self {
        let acp = AcpModel::new();

        // Initialize with empty threads - user will create on demand
        let threads = vec![];

        let focus_handle = cx.focus_handle();

        // Create message input
        let message_input = cx.new_view(|cx| {
            let mut input = TextInput::new(cx);
            input.set_placeholder("Message CocoWork's Agent...");
            input
        });

        // Re-render when message input changes (e.g. enable/disable send button)
        cx.observe(&message_input, |_, _, cx| cx.notify()).detach();

        // Create thread search input
        let search_input = cx.new_view(|cx| {
            let mut input = TextInput::new(cx);
            input.set_placeholder("Search Threads");
            input
        });

        // Keep thread filtering state in sync with the search input.
        cx.observe(&search_input, |this, search_input, cx| {
            this.search_text = search_input.read(cx).content().to_string();
            cx.notify();
        })
        .detach();

        // Spawn a timer to poll for ACP updates
        cx.spawn(|view, mut cx| async move {
            loop {
                // Wait 100ms between polls
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;

                // Poll and process updates
                let _ = view.update(&mut cx, |this, cx| {
                    let current_len = this.timeline_len();
                    let near_bottom = this.is_near_bottom(current_len);
                    this.stick_to_bottom = near_bottom;

                    this.acp.poll_and_process_updates();
                    // Sync thread list in case async operations completed
                    this.sync_thread_list();

                    let new_len = this.timeline_len();
                    let has_new_content = new_len > current_len;
                    let streaming = this.acp.is_loading();
                    if this.stick_to_bottom && new_len > 0 && (has_new_content || streaming) {
                        this.scroll_to_bottom_if_needed(new_len);
                    }
                    this.last_timeline_len = new_len;
                    cx.notify();
                });
            }
        })
        .detach();

        Self {
            theme,
            acp,
            message_input,
            search_input,
            threads,
            active_thread_idx: None,
            expanded_sections: vec!["Progress".to_string()],
            focus_handle,
            sidebar_width: layout::SIDEBAR_WIDTH,
            resizing_sidebar: false,
            sidebar_resize_start_x: 0.0,
            sidebar_resize_start_width: layout::SIDEBAR_WIDTH,
            context_panel_width: layout::CONTEXT_PANEL_WIDTH,
            resizing_context_panel: false,
            context_panel_resize_start_x: 0.0,
            context_panel_resize_start_width: layout::CONTEXT_PANEL_WIDTH,
            search_text: String::new(),
            show_agent_menu: false,
            show_mode_menu: false,
            workspace_path: None,
            attached_files: Vec::new(),
            show_mcp_panel: false,
            mcp_servers: vec![
                McpServerConfig {
                    name: "filesystem".to_string(),
                    command: "npx @modelcontextprotocol/server-filesystem".to_string(),
                    enabled: true,
                },
                McpServerConfig {
                    name: "github".to_string(),
                    command: "npx @modelcontextprotocol/server-github".to_string(),
                    enabled: false,
                },
            ],
            collapsed_thinking: std::collections::HashSet::new(),
            message_scroll_handle: ScrollHandle::new(),
            stick_to_bottom: true,
            last_timeline_len: 0,
            message_markdown_cache: std::collections::HashMap::new(),
            show_new_thread_dialog: false,
            show_user_menu: false,
        }
    }

    // ========================================================================
    // Event Handlers
    // ========================================================================

    fn handle_send_message(&mut self, cx: &mut ViewContext<Self>) {
        // Get content from the TextInput entity
        let text = self.message_input.read(cx).content().to_string();
        if text.trim().is_empty() {
            return;
        }

        // Clear the input
        self.message_input.update(cx, |input, cx| {
            input.clear(cx);
        });

        tracing::info!("Sending message: {}", text);

        // Use non-blocking send flow
        // This will:
        // 1. If connected with thread: send immediately
        // 2. If not connected: queue message and start connection
        // 3. When connected: start thread creation
        // 4. When thread ready: send the queued message
        self.acp.start_send_message(text);

        // Update UI thread list if we have a new active thread
        self.sync_thread_list();

        cx.notify();
    }

    /// Sync the thread list with the ACP manager state
    fn sync_thread_list(&mut self) {
        // Check if there's a new active thread we need to add to UI
        if let Some(thread_id) = &self.acp.active_session_id {
            // Check if this thread is already in our list
            let exists = self.threads.iter().any(|t| &t.id == thread_id);
            if !exists {
                // Add the new thread to the UI list
                let agent_id = self.acp.manager.selected_agent_id.clone().unwrap_or_default();
                let thread_name = "New thread".to_string();
                let new_thread = ThreadEntry::new(thread_id, &thread_name, &agent_id, 0);

                self.threads.insert(0, new_thread);
                self.active_thread_idx = Some(0);
                for (idx, thread) in self.threads.iter_mut().enumerate() {
                    thread.is_active = idx == 0;
                }
                tracing::info!("Added new thread to UI: {}", thread_id);
            }
        }

        // Update message counts
        if let Some(idx) = self.active_thread_idx {
            if idx < self.threads.len() {
                if let Some(session) = self.acp.active_session() {
                    self.threads[idx].message_count = session.messages.len();
                }
            }
        }
    }

    fn timeline_len(&self) -> usize {
        let len = self.acp.messages().len() + self.acp.tool_calls().len();
        if len == 0 {
            0
        } else {
            // +1 spacer at the bottom to keep a comfortable gap
            len + 1
        }
    }

    fn is_near_bottom(&self, item_count: usize) -> bool {
        if item_count == 0 {
            return true;
        }

        let bounds = self.message_scroll_handle.bounds();
        if bounds.size.height <= px(0.0) {
            return true;
        }

        let Some(last_bounds) = self.message_scroll_handle.bounds_for_item(item_count - 1) else {
            return true;
        };

        let bottom_pad = px(8.0);
        let offset = self.message_scroll_handle.offset();
        let viewport_bottom = bounds.bottom() - offset.y;
        let distance = last_bounds.bottom() - viewport_bottom;
        distance <= bottom_pad + px(8.0)
    }

    fn scroll_to_bottom_if_needed(&self, item_count: usize) {
        if item_count == 0 {
            return;
        }

        self.message_scroll_handle.scroll_to_item(item_count - 1);
    }

    fn select_thread(&mut self, idx: usize, cx: &mut ViewContext<Self>) {
        if idx < self.threads.len() {
            // Deselect previous
            if let Some(prev_idx) = self.active_thread_idx {
                if prev_idx < self.threads.len() {
                    self.threads[prev_idx].is_active = false;
                }
            }
            // Select new
            self.threads[idx].is_active = true;
            self.active_thread_idx = Some(idx);

            // Update the ACP model's active session to match
            let session_id = self.threads[idx].id.clone();
            self.acp.active_session_id = Some(session_id.clone());
            tracing::info!("Switched to thread: {}", session_id);
            self.message_markdown_cache.clear();
            self.collapsed_thinking.clear();
            self.stick_to_bottom = true;
            self.last_timeline_len = 0;
            self.message_scroll_handle
                .set_offset(point(px(0.0), px(0.0)));

            cx.notify();
        }
    }

    fn toggle_section(&mut self, section: &str, cx: &mut ViewContext<Self>) {
        if self.expanded_sections.contains(&section.to_string()) {
            self.expanded_sections.retain(|s| s != section);
        } else {
            self.expanded_sections.push(section.to_string());
        }
        cx.notify();
    }

    fn close_menus(&mut self, cx: &mut ViewContext<Self>) {
        if self.show_agent_menu || self.show_mode_menu || self.show_new_thread_dialog || self.show_user_menu {
            self.show_agent_menu = false;
            self.show_mode_menu = false;
            self.show_new_thread_dialog = false;
            self.show_user_menu = false;
            cx.notify();
        }
    }

    fn toggle_user_menu(&mut self, cx: &mut ViewContext<Self>) {
        self.show_user_menu = !self.show_user_menu;
        self.show_agent_menu = false;
        self.show_mode_menu = false;
        cx.notify();
    }

    fn select_workspace(&mut self, cx: &mut ViewContext<Self>) {
        // Open native folder picker dialog asynchronously
        cx.spawn(|view, mut cx| async move {
            let folder = rfd::AsyncFileDialog::new()
                .set_title("Select Agent Workspace")
                .pick_folder()
                .await;

            if let Some(folder) = folder {
                let path = folder.path().to_path_buf();
                let path_str = path.display().to_string();
                let _ = view.update(&mut cx, |this, cx| {
                    this.workspace_path = Some(path_str.clone());
                    // Update ACP working directory so agent uses this directory
                    this.acp.set_working_dir(Some(path));
                    tracing::info!("Workspace set to: {}", path_str);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn add_attachment(&mut self, cx: &mut ViewContext<Self>) {
        // Open native file picker dialog asynchronously
        cx.spawn(|view, mut cx| async move {
            let files = rfd::AsyncFileDialog::new()
                .set_title("Add File")
                .pick_files()
                .await;

            if let Some(files) = files {
                let _ = view.update(&mut cx, |this, cx| {
                    for file in files {
                        let path_str = file.path().display().to_string();
                        if !this.attached_files.contains(&path_str) {
                            this.attached_files.push(path_str);
                        }
                    }
                    tracing::info!("Attached files: {:?}", this.attached_files);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn remove_attachment(&mut self, file_path: &str, cx: &mut ViewContext<Self>) {
        self.attached_files.retain(|f| f != file_path);
        cx.notify();
    }

    fn toggle_mcp_panel(&mut self, cx: &mut ViewContext<Self>) {
        self.show_mcp_panel = !self.show_mcp_panel;
        // Close other menus
        self.show_agent_menu = false;
        self.show_mode_menu = false;
        cx.notify();
    }

    fn toggle_mcp_server(&mut self, server_name: &str, cx: &mut ViewContext<Self>) {
        if let Some(server) = self.mcp_servers.iter_mut().find(|s| s.name == server_name) {
            server.enabled = !server.enabled;
        }
        cx.notify();
    }

    /// Show new thread dialog with agent selection
    fn show_new_thread_dialog(&mut self, cx: &mut ViewContext<Self>) {
        self.show_new_thread_dialog = true;
        self.show_agent_menu = false;
        self.show_mode_menu = false;
        cx.notify();
    }

    /// Create a new thread with the specified agent (non-blocking)
    fn create_new_thread_with_agent(&mut self, agent_id: &str, cx: &mut ViewContext<Self>) {
        tracing::info!("Creating new thread with agent: {}", agent_id);

        // Close the dialog
        self.show_new_thread_dialog = false;

        // Start creating the new thread with the selected agent
        self.acp.start_new_thread_with_agent(agent_id);

        cx.notify();
    }

    /// Legacy: create new session (now shows dialog)
    fn create_new_thread(&mut self, cx: &mut ViewContext<Self>) {
        // Show the new thread dialog instead of immediately creating
        self.show_new_thread_dialog(cx);
    }

    fn start_resizing_sidebar(&mut self, event: &MouseDownEvent, cx: &mut ViewContext<Self>) {
        self.resizing_sidebar = true;
        self.sidebar_resize_start_x = f32::from(event.position.x);
        self.sidebar_resize_start_width = self.sidebar_width;
        cx.notify();
    }

    fn resize_sidebar(&mut self, event: &MouseMoveEvent, cx: &mut ViewContext<Self>) {
        if !self.resizing_sidebar {
            return;
        }

        let current_x = f32::from(event.position.x);
        let delta_x = current_x - self.sidebar_resize_start_x;
        let new_width = (self.sidebar_resize_start_width + delta_x).clamp(180.0, 480.0);

        if (new_width - self.sidebar_width).abs() > 0.5 {
            self.sidebar_width = new_width;
            cx.notify();
        }
    }

    fn stop_resizing_sidebar(&mut self, _event: &MouseUpEvent, cx: &mut ViewContext<Self>) {
        if self.resizing_sidebar {
            self.resizing_sidebar = false;
            cx.notify();
        }
    }

    fn start_resizing_context_panel(&mut self, event: &MouseDownEvent, cx: &mut ViewContext<Self>) {
        self.resizing_context_panel = true;
        self.context_panel_resize_start_x = f32::from(event.position.x);
        self.context_panel_resize_start_width = self.context_panel_width;
        cx.notify();
    }

    fn resize_context_panel(&mut self, event: &MouseMoveEvent, cx: &mut ViewContext<Self>) {
        if !self.resizing_context_panel {
            return;
        }

        let current_x = f32::from(event.position.x);
        // Right sidebar: delta is inverted (dragging left increases width)
        let delta_x = self.context_panel_resize_start_x - current_x;
        let new_width = (self.context_panel_resize_start_width + delta_x).clamp(200.0, 500.0);

        if (new_width - self.context_panel_width).abs() > 0.5 {
            self.context_panel_width = new_width;
            cx.notify();
        }
    }

    fn stop_resizing_context_panel(&mut self, _event: &MouseUpEvent, cx: &mut ViewContext<Self>) {
        if self.resizing_context_panel {
            self.resizing_context_panel = false;
            cx.notify();
        }
    }

    // ========================================================================
    // Top Bar
    // ========================================================================

    fn render_top_bar(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let show_user_menu = self.show_user_menu;

        div()
            .id("top-bar")
            .w_full()
            .h(px(40.0))
            .px(px(16.0))
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(colors.sidebar_bg))
            .border_b_1()
            .border_color(rgb(colors.border))
            // Left side: App title (with space for traffic lights on macOS)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    // Space for macOS traffic lights
                    .pl(px(70.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(colors.text_primary))
                            .child("cocowork"),
                    ),
            )
            // Right side: User avatar with dropdown menu (coconut icon)
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id("user-btn")
                            .w(px(28.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgb(colors.surface_elevated))
                            .border_1()
                            .border_color(rgb(colors.border))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba(colors.hover)))
                            .on_click(cx.listener(|this, _, cx| {
                                this.toggle_user_menu(cx);
                            }))
                            .child("🥥"),
                    )
                    // User menu dropdown
                    .when(show_user_menu, |el| {
                        el.child(self.render_user_menu(cx))
                    }),
            )
    }

    fn render_user_menu(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .absolute()
            .top(px(36.0))
            .right(px(0.0))
            .w(px(180.0))
            .bg(rgb(colors.surface_elevated))
            .border_1()
            .border_color(rgb(colors.border))
            .rounded(px(8.0))
            .shadow_lg()
            .py(px(4.0))
            .flex()
            .flex_col()
            // Settings option (placeholder - not implemented)
            .child(
                div()
                    .id("user-menu-settings")
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .on_click(cx.listener(|this, _, cx| {
                        this.show_user_menu = false;
                        // TODO: Open settings panel
                        tracing::info!("Settings clicked - not yet implemented");
                        cx.notify();
                    }))
                    .child(
                        // Settings icon (gear shape using CSS)
                        svg_icon(IconName::Settings, IconSize::Small)
                            .text_color(rgb(colors.text_secondary)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.text_primary))
                            .child("Settings"),
                    ),
            )
            // Separator
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .my(px(4.0))
                    .bg(rgb(colors.border)),
            )
            // About
            .child(
                div()
                    .id("user-menu-about")
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .on_click(cx.listener(|this, _, cx| {
                        this.show_user_menu = false;
                        tracing::info!("About clicked - version {}", env!("CARGO_PKG_VERSION"));
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(16.0))
                            .h(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(colors.text_secondary))
                                    .child("ⓘ"),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.text_primary))
                            .child("About"),
                    ),
            )
    }

    // ========================================================================
    // Bottom Bar
    // ========================================================================

    fn render_bottom_bar(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let enabled_count = self.mcp_servers.iter().filter(|s| s.enabled).count();
        let show_panel = self.show_mcp_panel;

        div()
            .id("bottom-bar")
            .w_full()
            .h(px(32.0))
            .px(px(16.0))
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(colors.sidebar_bg))
            .border_t_1()
            .border_color(rgb(colors.border))
            // Left side: Status info
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    // Connection status
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .w(px(6.0))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .bg(rgb(colors.success)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors.text_secondary))
                                    .child("Connected"),
                            ),
                    )
                    // Message count
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.text_secondary))
                            .child(format!(
                                "{} messages",
                                self.acp.active_session().map(|s| s.messages.len()).unwrap_or(0)
                            )),
                    ),
            )
            // Right side: Tools status
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    // MCP servers button with popup
                    .child(
                        div()
                            .relative()
                            .child(
                                div()
                                    .id("mcp-servers")
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .when(show_panel, |el| el.bg(rgba(colors.hover)))
                                    .hover(|s| s.bg(rgba(colors.hover)))
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.toggle_mcp_panel(cx);
                                    }))
                                    // Status indicator dot
                                    .child(
                                        div()
                                            .w(px(6.0))
                                            .h(px(6.0))
                                            .rounded_full()
                                            .bg(if enabled_count > 0 {
                                                rgb(colors.success)
                                            } else {
                                                rgb(colors.text_secondary)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(colors.text_secondary))
                                            .child(format!("MCP: {}", enabled_count)),
                                    ),
                            )
                            // MCP Panel popup
                            .when(show_panel, |el| {
                                el.child(self.render_mcp_panel(cx))
                            }),
                    )
                    // Version
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.text_secondary))
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
    }

    fn render_mcp_panel(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .absolute()
            .bottom(px(36.0))
            .right(px(0.0))
            .w(px(320.0))
            .bg(rgb(colors.surface_elevated))
            .border_1()
            .border_color(rgb(colors.border))
            .rounded(px(8.0))
            .shadow_lg()
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            // Header
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(colors.text_primary))
                            .child("MCP Servers"),
                    )
                    .child(
                        div()
                            .id("close-mcp-panel")
                            .text_sm()
                            .text_color(rgb(colors.text_secondary))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(colors.text_primary)))
                            .on_click(cx.listener(|this, _, cx| {
                                this.show_mcp_panel = false;
                                cx.notify();
                            }))
                            .child("×"),
                    ),
            )
            // Server list
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(self.mcp_servers.iter().map(|server| {
                        let server_name = server.name.clone();
                        let is_enabled = server.enabled;

                        div()
                            .id(SharedString::from(format!("mcp-{}", server.name)))
                            .w_full()
                            .p(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .rounded(px(6.0))
                            .bg(rgb(colors.surface))
                            // Toggle button
                            .child(
                                div()
                                    .id(SharedString::from(format!("toggle-{}", server.name)))
                                    .w(px(36.0))
                                    .h(px(20.0))
                                    .rounded(px(10.0))
                                    .cursor_pointer()
                                    .bg(if is_enabled {
                                        rgb(colors.primary)
                                    } else {
                                        rgb(colors.border)
                                    })
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .w(px(16.0))
                                            .h(px(16.0))
                                            .rounded_full()
                                            .bg(white())
                                            .ml(if is_enabled { px(18.0) } else { px(2.0) }),
                                    )
                                    .on_click(cx.listener(move |this, _, cx| {
                                        this.toggle_mcp_server(&server_name, cx);
                                    })),
                            )
                            // Server info
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(colors.text_primary))
                                            .child(server.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(colors.text_secondary))
                                            .overflow_hidden()
                                            .child(server.command.clone()),
                                    ),
                            )
                    })),
            )
            // Empty state
            .when(self.mcp_servers.is_empty(), |el: Div| {
                el.child(
                    div()
                        .py(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(colors.text_secondary))
                                .child("No MCP servers configured"),
                        ),
                )
            })
            // Add server button (placeholder)
            .child(
                div()
                    .id("add-mcp-server")
                    .w_full()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.text_secondary))
                            .child("+ Add Server"),
                    ),
            )
    }

    // ========================================================================
    // Sidebar
    // ========================================================================

    fn render_sidebar(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .id("sidebar")
            .w(px(self.sidebar_width))
            .flex_shrink_0()  // Don't shrink
            .h_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(colors.sidebar_bg))
            .border_r_1()
            .border_color(rgb(colors.border))
            // Search box
            .child(self.render_search_box(cx))
            // Threads header
            .child(self.render_threads_header(cx))
            // Threads list
            .child(self.render_threads_list(cx))
    }

    fn render_sidebar_resizer(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let resizing = self.resizing_sidebar;

        div()
            .id("sidebar-resizer")
            .w(px(4.0))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .when(resizing, |el| {
                el.bg(rgba(colors.primary.with_alpha(0.35)))
            })
            .when(!resizing, |el| {
                el.hover(|s| s.bg(rgba(colors.border.with_alpha(0.35))))
            })
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, cx| {
                this.start_resizing_sidebar(event, cx);
            }))
    }

    fn render_context_panel_resizer(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let resizing = self.resizing_context_panel;

        div()
            .id("context-panel-resizer")
            .w(px(4.0))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .when(resizing, |el| {
                el.bg(rgba(colors.primary.with_alpha(0.35)))
            })
            .when(!resizing, |el| {
                el.hover(|s| s.bg(rgba(colors.border.with_alpha(0.35))))
            })
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, cx| {
                this.start_resizing_context_panel(event, cx);
            }))
    }

    fn render_search_box(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let search_text = self.search_text.clone();
        let has_search = !search_text.is_empty();

        div()
            .id("search-box-container")
            .w_full()
            .p(px(Spacing::default().md))
            .child(
                div()
                    .id("search-box")
                    .w_full()
                    .h(px(32.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(6.0))
                    .bg(rgb(colors.input_bg))
                    // Search icon
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.text_secondary))
                            .child("⌕"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.search_input.clone()),
                    )
                    // Clear button
                    .when(has_search, |el| {
                        el.child(
                            div()
                                .id("clear-search")
                                .text_sm()
                                .text_color(rgb(colors.text_secondary))
                                .cursor_pointer()
                                .hover(|s| s.text_color(rgb(colors.text_primary)))
                                .on_click(cx.listener(|this, _, cx| {
                                    this.search_input.update(cx, |input, cx| input.clear(cx));
                                }))
                                .child("×"),
                        )
                    }),
            )
    }

    fn render_threads_header(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .w_full()
            .h(px(32.0))
            .px(px(16.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(colors.text_secondary))
                    .child("Threads"),
            )
            .child(
                div()
                    .id("new-session-btn")
                    .w(px(20.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .on_click(cx.listener(|this, _, cx| {
                        this.create_new_thread(cx);
                    }))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.text_secondary))
                            .child("+"),
                    ),
            )
    }

    fn render_threads_list(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let search_query = self.search_text.to_lowercase();

        // Filter threads based on search query
        let filtered_threads: Vec<(usize, &ThreadEntry)> = self
            .threads
            .iter()
            .enumerate()
            .filter(|(_, thread)| {
                if search_query.is_empty() {
                    true
                } else {
                    thread.name.to_lowercase().contains(&search_query)
                        || thread.agent_id.to_lowercase().contains(&search_query)
                }
            })
            .collect();

        let no_results = filtered_threads.is_empty() && !search_query.is_empty();

        div()
            .id("threads-list")
            .flex_1()
            .min_h_0()  // Critical: Allow shrinking for scrolling to work
            .overflow_y_scroll()
            .px(px(8.0))
            .py(px(4.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    // No results message
                    .when(no_results, |el| {
                        el.child(
                            div()
                                .w_full()
                                .py(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(colors.text_secondary))
                                        .child(format!("No threads match \"{}\"", self.search_text)),
                                ),
                        )
                    })
                    .children(filtered_threads.iter().map(|(idx, session)| {
                        let idx = *idx;
                        let is_active = self.active_thread_idx == Some(idx);
                        let session_name = session.name.clone();
                        let session_id = session.id.clone();
                        let agent_icon_name = match session.agent_id.as_str() {
                            "claude-code" => IconName::AiClaude,
                            "gemini" => IconName::AiGemini,
                            _ => IconName::Chat,
                        };

                        div()
                            .id(SharedString::from(format!("session-{}", session_id)))
                            .w_full()
                            .h(px(28.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .when(is_active, |el| {
                                el.bg(rgba(colors.primary.with_alpha(0.15)))
                            })
                            .when(!is_active, |el| el.hover(|s| s.bg(rgba(colors.hover))))
                            .on_click(cx.listener(move |this, _, cx| {
                                this.select_thread(idx, cx);
                            }))
                            .child(
                                svg_icon(agent_icon_name, IconSize::Small)
                                    .text_color(rgb(colors.text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(rgb(colors.text_primary))
                                    .text_ellipsis()
                                    .child(session_name),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors.text_secondary))
                                    .child(format!("{}", session.message_count)),
                            )
                    })),
            )
    }

    // ========================================================================
    // Main Panel
    // ========================================================================

    fn render_main_panel(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = self.theme.colors.clone();

        div()
            .id("main-panel")
            .flex_1()
            .h_full()
            .min_w_0()  // Allow shrinking below content size
            .min_h_0()  // Critical: Allow shrinking in flex column for scrolling to work
            .flex()
            .flex_col()
            .overflow_hidden()  // Clip overflow from this panel, children handle their own scroll
            .bg(rgb(colors.panel_bg))
            .child(self.render_session_header(cx))
            .child(self.render_message_area(cx))
            .child(self.render_input_bar(cx))
    }

    fn render_session_header(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let is_preparing = self.acp.is_creating_thread() ||
            self.acp.connection_state() == cocowork_ui::ConnectionState::Connecting;

        let agent_name = self.acp.selected_agent_name();

        // Determine title based on state
        let (title, title_color, show_spinner) = if is_preparing {
            (format!("{} Preparing...", agent_name), colors.text_secondary, true)
        } else if let Some(session) = self.active_thread_idx.and_then(|idx| self.threads.get(idx)) {
            (session.name.clone(), colors.text_primary, false)
        } else {
            ("New Thread".to_string(), colors.text_secondary, false)
        };

        div()
            .id("session-header")
            .w_full()
            .h(px(40.0))  // Aligned with context panel sections
            .flex_shrink_0()  // Never shrink, keep fixed height
            .px(px(16.0))
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(rgb(colors.border))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(8.0))
                    // Spinner or arrow (using SVG icons)
                    .child(
                        svg_icon(
                            if show_spinner { IconName::Circle } else { IconName::ChevronRight },
                            IconSize::XSmall
                        ).text_color(rgb(colors.text_secondary)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(title_color))
                            .text_ellipsis()
                            .child(title),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    // New session button
                    .child(
                        div()
                            .id("header-new-session-btn")
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba(colors.hover)))
                            .on_click(cx.listener(|this, _, cx| {
                                this.create_new_thread(cx);
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(colors.text_secondary))
                                    .child("+"),
                            ),
                    )
                    // More options button
                    .child(self.render_header_button("···")),
            )
    }

    fn render_header_button(&self, label: &str) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(colors.hover)))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(colors.text_secondary))
                    .child(label.to_string()),
            )
    }

    fn render_message_area(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = self.theme.colors.clone();
        let messages = self.acp.messages().into_iter().cloned().collect::<Vec<_>>();
        let mut tool_calls = self.acp.tool_calls().into_iter().cloned().collect::<Vec<_>>();
        tool_calls.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        let has_timeline = !messages.is_empty() || !tool_calls.is_empty();
        let timeline_children = if has_timeline {
            self.build_timeline_children(&messages, &tool_calls, cx)
        } else {
            Vec::new()
        };

        // NOTE: In GPUI layouts, relying on `size_full()` (100% height) inside a flex item can
        // fail to produce a definite height, which prevents overflow scrolling and causes the
        // message list to expand and "push" other UI off-screen. Keep the scroll container as a
        // real flex child (`flex_1 + min_h_0`) so it always has a constrained height.
        div()
            .id("message-area-container")
            .flex_1()
            .min_h_0()  // Critical: Allow shrinking in flex column for scrolling to work
            .w_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("message-area")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.message_scroll_handle)
                    .flex()
                    .flex_col()
            .when(!has_timeline, |el| {
                // Empty state - centered with nice styling
                el.items_center()
                    .justify_center()
                    .p(px(32.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(16.0))
                            // Logo image
                            .child(
                                img("images/cocowork-logo-256.png")
                                    .size(px(200.0)),
                            )
                            // Title
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(colors.text_primary))
                                    .child("Start a conversation"),
                            )
                            // Subtitle
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(colors.text_secondary))
                                    .child("Type a message below to chat with CocoWork's Agent"),
                            )
                            // Hint
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(colors.surface))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(colors.text_secondary))
                                            .child("Use 📁 to set workspace, + to attach files"),
                                    ),
                            ),
                    )
            })
            .when(has_timeline, move |el| {
                el.px(px(16.0))
                    .pt(px(16.0))
                    .gap(px(12.0))
                    .children(timeline_children)
            }),
            )  // Close the outer .child()
    }

    fn build_timeline_children(
        &mut self,
        messages: &[MessageBlock],
        tool_calls: &[ToolCallState],
        cx: &mut ViewContext<Self>,
    ) -> Vec<AnyElement> {
        enum TimelineItem {
            Message { idx: usize, msg: MessageBlock },
            ToolCall { idx: usize, call: ToolCallState },
        }

        impl TimelineItem {
            fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
                match self {
                    Self::Message { msg, .. } => msg.timestamp(),
                    Self::ToolCall { call, .. } => call.started_at,
                }
            }

            fn kind_order(&self) -> u8 {
                match self {
                    Self::ToolCall { .. } => 0,
                    Self::Message { .. } => 1,
                }
            }

            fn tie_index(&self) -> usize {
                match self {
                    Self::Message { idx, .. } => *idx,
                    Self::ToolCall { idx, .. } => *idx,
                }
            }
        }

        let mut timeline = Vec::with_capacity(messages.len() + tool_calls.len());
        for (idx, msg) in messages.iter().cloned().enumerate() {
            timeline.push(TimelineItem::Message { idx, msg });
        }
        for (idx, call) in tool_calls.iter().cloned().enumerate() {
            timeline.push(TimelineItem::ToolCall { idx, call });
        }

        timeline.sort_by(|a, b| {
            a.timestamp()
                .cmp(&b.timestamp())
                .then_with(|| a.kind_order().cmp(&b.kind_order()))
                .then_with(|| a.tie_index().cmp(&b.tie_index()))
        });

        let mut children = Vec::with_capacity(timeline.len() + 1);
        for item in timeline {
            match item {
                TimelineItem::Message { idx, msg } => {
                    children.push(self.render_message(idx, &msg, cx).into_any_element());
                }
                TimelineItem::ToolCall { call, .. } => {
                    children.push(self.render_tool_call(&call, cx).into_any_element());
                }
            }
        }

        // Spacer at the bottom to avoid jitter and keep a comfortable gap.
        children.push(
            div()
                .w_full()
                .h(px(32.0))
                .flex_shrink_0()
                .into_any_element(),
        );

        children
    }

    fn render_message(&mut self, idx: usize, message: &MessageBlock, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = self.theme.colors.clone();

        match message {
            // User message: Dark rounded pill style (like Zed's input box)
            MessageBlock::User { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                div()
                    .w_full()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .px(px(16.0))
                            .py(px(12.0))
                            .rounded(px(8.0))
                            .bg(rgb(colors.input_bg))
                            .overflow_hidden()
                            .child(
                                div()
                                    .w_full()
                                    .text_sm()
                                    .text_color(rgb(colors.text_primary))
                                    .overflow_x_hidden()
                                    .child(text),
                            ),
                    )
            }

            // Thinking block: Zed style with left border and lightbulb icon
            MessageBlock::Thought { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                let is_collapsed = self.collapsed_thinking.contains(&idx);
                let markdown = self.render_markdown_view(&format!("thought-{}", idx), &text, true, cx);

                div()
                    .w_full()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        // Thinking header (clickable to collapse)
                        div()
                            .id(SharedString::from(format!("thinking-header-{}", idx)))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, cx| {
                                this.toggle_thinking(idx, cx);
                            }))
                            .child(
                                // Lightbulb icon
                                div()
                                    .text_sm()
                                    .text_color(rgb(colors.text_secondary))
                                    .child("💡"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(colors.text_secondary))
                                    .child("Thinking"),
                            )
                            .child(
                                // Collapse indicator
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors.text_secondary))
                                    .child(if is_collapsed { "▶" } else { "▼" }),
                            ),
                    )
                    // Thinking content with left border
                    .when(!is_collapsed, move |el| {
                        el.child(
                            div()
                                .w_full()
                                .mt(px(8.0))
                                .pl(px(12.0))
                                .overflow_hidden()
                                .border_l_2()
                                .border_color(rgb(colors.border))
                                .child(
                                    div()
                                        .w_full()
                                        .overflow_x_hidden()
                                        .text_sm()
                                        .text_color(rgba(colors.text_secondary.with_alpha(0.9)))
                                        .child(markdown),
                                ),
                        )
                    })
            }

            // Agent response: Markdown (Zed renderer)
            MessageBlock::Agent { content, .. } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                div()
                    .w_full()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(self.render_markdown_view(&format!("agent-{}", idx), &text, false, cx))
            }

            // System message: Muted style
            MessageBlock::System { content, .. } => {
                div()
                    .w_full()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.text_secondary))
                            .child(content.clone()),
                    )
            }
        }
    }

    fn render_markdown_view(
        &mut self,
        key: &str,
        text: &str,
        muted: bool,
        cx: &mut ViewContext<Self>,
    ) -> AnyElement {
        let view = self.markdown_view(key, text, muted, cx);
        div()
            .w_full()
            .min_w_0()
            .overflow_x_hidden()
            .child(view)
            .into_any_element()
    }

    fn markdown_view(
        &mut self,
        key: &str,
        text: &str,
        muted: bool,
        cx: &mut ViewContext<Self>,
    ) -> View<Markdown> {
        let cache_key = format!("{}:{}", key, if muted { "muted" } else { "normal" });
        if let Some(view) = self.message_markdown_cache.get(&cache_key) {
            let _ = view.update(cx, |markdown, cx| {
                markdown.reset(text.to_string(), cx);
            });
            return view.clone();
        }

        let style = self.markdown_style(muted, cx);
        let view = cx.new_view(|cx| Markdown::new(text.to_string(), style, None, cx, None));
        self.message_markdown_cache.insert(cache_key, view.clone());
        view
    }

    fn markdown_style(&self, muted: bool, cx: &mut ViewContext<Self>) -> MarkdownStyle {
        let colors = &self.theme.colors;
        let base_color = if muted {
            rgba(colors.text_secondary.with_alpha(0.9))
        } else {
            rgb(colors.text_primary)
        };
        let code_bg = rgb(colors.code_bg);
        let code_text = rgb(colors.code_text);
        let link_color = rgb(colors.text_link);

        let mut base_text_style = cx.text_style();
        base_text_style.color = Hsla::from(base_color);
        base_text_style.font_size = px(self.theme.typography.base_size).into();

        MarkdownStyle {
            base_text_style,
            code_block: StyleRefinement {
                background: Some(code_bg.into()),
                padding: EdgesRefinement {
                    top: Some(px(8.0).into()),
                    left: Some(px(10.0).into()),
                    right: Some(px(10.0).into()),
                    bottom: Some(px(8.0).into()),
                },
                margin: EdgesRefinement {
                    top: Some(Length::Definite(px(6.0).into())),
                    left: Some(Length::Definite(px(0.0).into())),
                    right: Some(Length::Definite(px(0.0).into())),
                    bottom: Some(Length::Definite(px(6.0).into())),
                },
                border_color: Some(rgba(colors.border).into()),
                border_widths: EdgesRefinement {
                    top: Some(px(1.0).into()),
                    left: Some(px(1.0).into()),
                    right: Some(px(1.0).into()),
                    bottom: Some(px(1.0).into()),
                },
                text: Some(TextStyleRefinement {
                    font_family: Some("monospace".into()),
                    color: Some(Hsla::from(code_text)),
                    ..Default::default()
                }),
                ..Default::default()
            },
            inline_code: TextStyleRefinement {
                font_family: Some("monospace".into()),
                background_color: Some(Hsla::from(code_bg)),
                color: Some(Hsla::from(code_text)),
                ..Default::default()
            },
            block_quote: TextStyleRefinement {
                color: Some(Hsla::from(rgba(colors.text_secondary))),
                ..Default::default()
            },
            link: TextStyleRefinement {
                color: Some(Hsla::from(link_color)),
                underline: Some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(Hsla::from(link_color)),
                    wavy: false,
                }),
                ..Default::default()
            },
            rule_color: Hsla::from(rgba(colors.divider)),
            block_quote_border_color: Hsla::from(rgba(colors.border)),
            selection_background_color: Hsla::from(rgba(colors.selection)),
            ..Default::default()
        }
    }

    fn toggle_thinking(&mut self, idx: usize, cx: &mut ViewContext<Self>) {
        if self.collapsed_thinking.contains(&idx) {
            self.collapsed_thinking.remove(&idx);
        } else {
            self.collapsed_thinking.insert(idx);
        }
        cx.notify();
    }

    fn render_tool_call(&self, tool_call: &ToolCallState, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        // Status color
        let status_color = match tool_call.status {
            ToolCallStatus::Pending => rgb(colors.text_secondary),
            ToolCallStatus::InProgress => rgb(colors.primary),
            ToolCallStatus::Completed => rgb(ThemeRgba::rgb(0x4ADE80)),
            ToolCallStatus::Failed => rgb(ThemeRgba::rgb(0xF87171)),
            ToolCallStatus::Cancelled => rgb(colors.text_secondary),
        };

        // Tool kind icon
        let kind_icon = match tool_call.kind {
            Some(ToolCallKind::Read) => IconName::File,
            Some(ToolCallKind::Write) => IconName::Pencil,
            Some(ToolCallKind::Edit) => IconName::Pencil,
            Some(ToolCallKind::Delete) => IconName::Close,
            Some(ToolCallKind::Execute) | Some(ToolCallKind::Bash) | Some(ToolCallKind::Terminal) => IconName::Terminal,
            Some(ToolCallKind::Search) | Some(ToolCallKind::Grep) | Some(ToolCallKind::Glob) => IconName::Search,
            Some(ToolCallKind::Fetch) => IconName::Web,
            Some(ToolCallKind::Task) => IconName::CircleCheck,
            Some(ToolCallKind::Plan) => IconName::CircleCheck,
            Some(ToolCallKind::Think) => IconName::Chat,
            _ => IconName::Settings,
        };

        // Status icon based on status
        let status_icon = match tool_call.status {
            ToolCallStatus::Pending => IconName::Circle,
            ToolCallStatus::InProgress => IconName::Circle,
            ToolCallStatus::Completed => IconName::Check,
            ToolCallStatus::Failed => IconName::Close,
            ToolCallStatus::Cancelled => IconName::Close,
        };

        let title = tool_call.title.as_deref().unwrap_or("Tool call");

        div()
            .w_full()
            .flex_shrink_0()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .bg(rgb(colors.surface))
            .border_1()
            .border_color(rgb(colors.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    // Status indicator (SVG icon)
                    .child(
                        svg_icon(status_icon, IconSize::XSmall)
                            .text_color(status_color),
                    )
                    // Kind icon (SVG icon)
                    .child(
                        svg_icon(kind_icon, IconSize::Small)
                            .text_color(rgb(colors.text_secondary)),
                    )
                    // Title
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(rgb(colors.text_primary))
                            .child(title.to_string()),
                    )
                    // Tool ID (dimmed)
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.text_secondary))
                            .child(format!("#{}", &tool_call.id[..8.min(tool_call.id.len())])),
                    ),
            )
    }

    fn render_input_bar(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .id("input-bar")
            .w_full()
            .flex_shrink_0()  // Never shrink, keep natural height
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .bg(rgb(colors.panel_bg))
            .border_t_1()
            .border_color(rgb(colors.border))
            // Handle Enter key for sending
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, cx| {
                if event.keystroke.key == "enter" && !event.keystroke.modifiers.shift {
                    this.handle_send_message(cx);
                }
            }))
            // Editor container (like Zed's message editor)
            .child(
                div()
                    .w_full()
                    .rounded(px(8.0))
                    .bg(rgb(colors.surface))
                    .border_1()
                    .border_color(rgb(colors.border_subtle))
                    .flex()
                    .flex_col()
                    // Text input area - use the TextInput view
                    .child(
                        div()
                            .w_full()
                            .min_h(px(80.0))
                            .max_h(px(200.0))
                            .p(px(12.0))
                            .overflow_hidden()
                            .child(self.message_input.clone()),
                    )
                    // Bottom controls inside the editor box
                    .child(
                        div()
                            .w_full()
                            .px(px(8.0))
                            .py(px(6.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_t_1()
                            .border_color(rgb(colors.border_subtle))
                            // Left: Context button
                            .child(self.render_context_button(cx))
                            // Right: Send button only (agent selection moved to new thread dialog)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(self.render_send_button(cx)),
                            ),
                    ),
            )
    }

    fn render_context_button(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let workspace_display = self.workspace_path.as_ref().map(|p| {
            // Show only the last folder name
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone())
        });

        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            // Folder button (workspace selector)
            .child(
                div()
                    .id("folder-btn")
                    .h(px(26.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .on_click(cx.listener(|this, _, cx| {
                        this.select_workspace(cx);
                    }))
                    .child(
                        svg_icon(IconName::Folder, IconSize::Small)
                            .text_color(rgb(colors.text_secondary)),
                    )
                    .when_some(workspace_display.clone(), |el, name| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(rgb(colors.text_secondary))
                                .max_w(px(120.0))
                                .text_ellipsis()
                                .child(name),
                        )
                    }),
            )
            // + button (add attachment)
            .child(
                div()
                    .id("add-btn")
                    .h(px(26.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .on_click(cx.listener(|this, _, cx| {
                        this.add_attachment(cx);
                    }))
                    .child(
                        svg_icon(IconName::Plus, IconSize::Small)
                            .text_color(rgb(colors.text_secondary)),
                    ),
            )
            // Show attached files as chips
            .children(self.attached_files.iter().map(|file| {
                let file_name = file.clone();
                let display_name = std::path::Path::new(file)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| file.clone());

                div()
                    .id(SharedString::from(format!("attach-{}", file)))
                    .h(px(22.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(4.0))
                    .bg(rgba(colors.primary.with_alpha(0.2)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.text_primary))
                            .max_w(px(100.0))
                            .text_ellipsis()
                            .child(display_name),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("remove-{}", file)))
                            .text_xs()
                            .text_color(rgb(colors.text_secondary))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(colors.error)))
                            .on_click(cx.listener(move |this, _, cx| {
                                this.remove_attachment(&file_name, cx);
                            }))
                            .child("×"),
                    )
            }))
    }

    fn render_send_button(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let has_text = !self.message_input.read(cx).content().is_empty();

        div()
            .id("send-button")
            .h(px(26.0))
            .w(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .when(has_text, |el| {
                el.bg(rgb(colors.primary))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(colors.primary_hover)))
            })
            .when(!has_text, |el| {
                el.bg(rgb(colors.surface))
                    .cursor_default()
            })
            .on_click(cx.listener(|this, _, cx| {
                this.handle_send_message(cx);
            }))
            .child(
                svg_icon(IconName::ArrowUp, IconSize::Small)
                    .text_color(if has_text { white() } else { rgb(colors.text_secondary) }),
            )
    }

    // ========================================================================
    // Context Panel
    // ========================================================================

    fn render_context_panel(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .id("context-panel")
            .w(px(self.context_panel_width))
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(colors.sidebar_bg))  // Same as left sidebar
            .border_l_1()                 // Left border for separation
            .border_color(rgb(colors.border))
            .child(self.render_progress_section(cx))
            .child(self.render_collapsible_section("Artifacts", cx))
            .child(self.render_collapsible_section("Context", cx))
    }

    /// Render the Progress section showing task/plan completion
    fn render_progress_section(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let is_expanded = self.expanded_sections.contains(&"Progress".to_string());
        let arrow_icon = if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight };

        // Get real plan data from ACP session
        let plan_entries: Vec<PlanEntry> = self
            .acp
            .active_session()
            .and_then(|s| s.current_task.as_ref())
            .map(|t| t.plan.clone())
            .unwrap_or_default();

        let completed_count = plan_entries
            .iter()
            .filter(|e| matches!(e.status, PlanStatus::Completed))
            .count();
        let total_count = plan_entries.len();
        let has_plan = !plan_entries.is_empty();

        div()
            .w_full()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(rgb(colors.border))
            .child(
                div()
                    .id("section-progress")
                    .w_full()
                    .h(px(40.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .on_click(cx.listener(|this, _, cx| {
                        this.toggle_section("Progress", cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                svg_icon(arrow_icon, IconSize::XSmall)
                                    .text_color(rgb(colors.text_secondary)),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(colors.text_primary))
                                    .child("Progress"),
                            ),
                    )
                    // Progress indicator
                    .when(has_plan, |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(rgb(colors.text_secondary))
                                .child(format!("{}/{}", completed_count, total_count)),
                        )
                    }),
            )
            .when(is_expanded, |el| {
                el.child(
                    div()
                        .w_full()
                        .px(px(16.0))
                        .py(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        // Show progress bar only if there's a plan
                        .when(has_plan, |el| {
                            let progress_pct = if total_count > 0 {
                                (completed_count as f32 / total_count as f32) * 100.0
                            } else {
                                0.0
                            };
                            el.child(
                                div()
                                    .w_full()
                                    .h(px(4.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(colors.surface))
                                    .child(
                                        div()
                                            .h_full()
                                            .w(px(progress_pct * 2.48)) // 248px max width
                                            .rounded(px(2.0))
                                            .bg(rgb(colors.primary)),
                                    ),
                            )
                        })
                        // Plan items or empty state
                        .when(has_plan, |el| {
                            el.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .children(plan_entries.iter().map(|entry| {
                                        self.render_plan_item(&entry.content, &entry.status)
                                    })),
                            )
                        })
                        .when(!has_plan, |el| {
                            el.child(
                                div()
                                    .py(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(colors.text_secondary))
                                            .child("No active plan"),
                                    ),
                            )
                        }),
                )
            })
    }

    /// Render a single plan item
    fn render_plan_item(&self, title: &str, status: &PlanStatus) -> impl IntoElement {
        let colors = &self.theme.colors;

        let (status_icon, icon_color) = match status {
            PlanStatus::Completed => (IconName::Check, colors.success),
            PlanStatus::InProgress => (IconName::Circle, colors.primary),
            PlanStatus::Pending => (IconName::Circle, colors.text_secondary),
            PlanStatus::Skipped => (IconName::Close, colors.text_secondary),
        };

        div()
            .w_full()
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                svg_icon(status_icon, IconSize::XSmall)
                    .text_color(rgb(icon_color)),
            )
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(match status {
                        PlanStatus::Completed => rgb(colors.text_secondary),
                        PlanStatus::InProgress => rgb(colors.text_primary),
                        PlanStatus::Pending => rgb(colors.text_secondary),
                        PlanStatus::Skipped => rgb(colors.text_secondary),
                    })
                    .child(title.to_string()),
            )
    }

    fn render_collapsible_section(
        &self,
        title: &str,
        cx: &mut ViewContext<Self>,
    ) -> impl IntoElement {
        let colors = &self.theme.colors;
        let is_expanded = self.expanded_sections.contains(&title.to_string());
        let arrow_icon = if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight };
        let section_name = title.to_string();

        div()
            .w_full()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(rgb(colors.border))
            .child(
                div()
                    .id(SharedString::from(format!("section-{}", title.to_lowercase())))
                    .w_full()
                    .h(px(40.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(colors.hover)))
                    .on_click(cx.listener(move |this, _, cx| {
                        this.toggle_section(&section_name, cx);
                    }))
                    .child(
                        svg_icon(arrow_icon, IconSize::XSmall)
                            .text_color(rgb(colors.text_secondary)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(colors.text_primary))
                            .child(title.to_string()),
                    ),
            )
            .when(is_expanded, |el| {
                el.child(
                    div()
                        .w_full()
                        .min_h(px(80.0))
                        .px(px(16.0))
                        .py(px(12.0))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(colors.text_secondary))
                                .child(self.render_section_content(title)),
                        ),
                )
            })
    }

    fn render_section_content(&self, section: &str) -> String {
        match section {
            "Artifacts" => "No artifacts yet".to_string(),
            "Context" => "No context added".to_string(),
            _ => "".to_string(),
        }
    }
}

// ============================================================================
// Render Implementation
// ============================================================================

impl FocusableView for CocoWorkWindow {
    fn focus_handle(&self, _cx: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CocoWorkWindow {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;

        div()
            .id("cocowork-window")
            .key_context("CocoWorkWindow")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(colors.panel_bg))
            .text_color(rgb(colors.text_primary))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, cx| {
                this.close_menus(cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, cx| {
                this.resize_sidebar(event, cx);
                this.resize_context_panel(event, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, event: &MouseUpEvent, cx| {
                this.stop_resizing_sidebar(event, cx);
                this.stop_resizing_context_panel(event, cx);
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, event: &MouseUpEvent, cx| {
                this.stop_resizing_sidebar(event, cx);
                this.stop_resizing_context_panel(event, cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, cx| {
                if event.keystroke.key == "escape" {
                    this.close_menus(cx);
                }
            }))
            // Top bar
            .child(self.render_top_bar(cx))
            // Main content (three panels)
            .child(
                div()
                    .flex_1()
                    .min_h_0()  // Critical: Allow shrinking in flex column for child scrolling to work
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(self.render_sidebar(cx))
                    .child(self.render_sidebar_resizer(cx))
                    .child(self.render_main_panel(cx))
                    .child(self.render_context_panel_resizer(cx))
                    .child(self.render_context_panel(cx))
            )
            // Bottom bar
            .child(self.render_bottom_bar(cx))
            // New thread dialog (modal overlay)
            .when(self.show_new_thread_dialog, |el| {
                el.child(self.render_new_thread_dialog(cx))
            })
    }
}

impl CocoWorkWindow {
    fn render_new_thread_dialog(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let colors = &self.theme.colors;
        let agents = self.acp.available_agents();

        // Modal overlay
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(colors.panel_bg.with_alpha(0.9)))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, cx| {
                this.show_new_thread_dialog = false;
                cx.notify();
            }))
            .child(
                // Dialog box
                div()
                    .w(px(400.0))
                    .max_h(px(500.0))
                    .bg(rgb(colors.surface_elevated))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, cx| {
                        cx.stop_propagation();
                    })
                    // Header
                    .child(
                        div()
                            .px(px(20.0))
                            .py(px(16.0))
                            .border_b_1()
                            .border_color(rgb(colors.border))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(colors.text_primary))
                                    .child("New Thread"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(colors.text_secondary))
                                    .child("Select an agent"),
                            ),
                    )
                    // Agent list
                    .child(
                        div()
                            .id("agent-list")
                            .flex_1()
                            .overflow_scroll()
                            .p(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .children(agents.iter().map(|agent| {
                                let agent_id = agent.id.clone();
                                let agent_name = agent.name.clone();
                                let agent_desc = agent.description.clone().unwrap_or_default();
                                let is_selected = self.acp.manager.selected_agent_id.as_ref() == Some(&agent_id);

                                div()
                                    .id(SharedString::from(format!("agent-{}", agent_id)))
                                    .px(px(16.0))
                                    .py(px(12.0))
                                    .rounded(px(8.0))
                                    .border_1()
                                    .when(is_selected, |el| {
                                        el.border_color(rgb(colors.primary))
                                            .bg(rgba(colors.primary.with_alpha(0.1)))
                                    })
                                    .when(!is_selected, |el| {
                                        el.border_color(rgb(colors.border))
                                            .hover(|el| el.bg(rgb(colors.surface)))
                                    })
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, cx| {
                                        this.create_new_thread_with_agent(&agent_id, cx);
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .child(
                                                        div()
                                                            .text_base()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(colors.text_primary))
                                                            .child(agent_name),
                                                    )
                                                    .when(is_selected, |el| {
                                                        el.child(
                                                            div()
                                                                .text_xs()
                                                                .px(px(6.0))
                                                                .py(px(2.0))
                                                                .rounded(px(4.0))
                                                                .bg(rgb(colors.primary))
                                                                .text_color(rgb(ThemeRgba::rgb(0xFFFFFF))) // White text on primary
                                                                .child("Current"),
                                                        )
                                                    }),
                                            )
                                            .when(!agent_desc.is_empty(), |el| {
                                                el.child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(colors.text_secondary))
                                                        .child(agent_desc),
                                                )
                                            }),
                                    )
                            })),
                    )
                    // Footer
                    .child(
                        div()
                            .px(px(20.0))
                            .py(px(12.0))
                            .border_t_1()
                            .border_color(rgb(colors.border))
                            .flex()
                            .justify_end()
                            .child(
                                div()
                                    .id("cancel-btn")
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(colors.surface))
                                    .text_sm()
                                    .text_color(rgb(colors.text_secondary))
                                    .cursor_pointer()
                                    .hover(|el| el.bg(rgb(colors.border)))
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.show_new_thread_dialog = false;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            ),
                    ),
            )
    }
}

// ============================================================================
// Color Helpers
// ============================================================================

fn rgb(c: cocowork_ui::Rgba) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: 1.0,
    }
}

fn rgba(c: cocowork_ui::Rgba) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

fn white() -> Rgba {
    Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    }
}
