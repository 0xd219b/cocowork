<p align="center">
  <img src="img/cocowork/cocowork-logo-raw.png" alt="CocoWork Logo">
</p>

<h1 align="center">CocoWork</h1>

<p align="center">
  <strong>An agent-agnostic desktop application for AI coding agents</strong>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#supported-agents">Supported Agents</a> •
  <a href="#installation">Installation</a> •
  <a href="#development">Development</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#acknowledgments">Acknowledgments</a>
</p>

---

CocoWork is a native desktop application that provides a unified interface for communicating with AI coding agents via the [Agent Client Protocol (ACP)](https://github.com/anthropics/acp). Built with pure Rust and GPUI, it offers a fast, lightweight, and consistent experience across different AI agents.

## Features

- **Agent Agnostic**: Work with multiple AI coding agents through a single interface
- **Native Performance**: Built with Rust for speed and efficiency
- **Modern UI**: Beautiful, responsive interface powered by GPUI
- **Session Management**: Persistent conversations with SQLite storage
- **File System Integration**: Secure file access with permission management
- **MCP Support**: Model Context Protocol server integration
- **Cross-Platform**: Runs on macOS (Windows and Linux support planned)

## Supported Agents

CocoWork supports any ACP-compatible agent, including:

| Agent | Status | Notes |
|-------|--------|-------|
| [Claude Code](https://github.com/anthropics/claude-code) | Supported | Via `@zed-industries/claude-code-acp` bridge |
| [Gemini CLI](https://github.com/google-gemini/gemini-cli) | Supported | Uses `--experimental-acp` flag |
| [Codex](https://github.com/openai/codex) | Supported | Via [`codex-acp`](https://github.com/zed-industries/codex-acp) binary (auto-downloaded) |
| [Goose](https://github.com/block/goose) | Supported | Uses `--acp` flag |
| Custom Agents | Supported | Any ACP-compatible agent |

## Installation

### Prerequisites

- Rust 1.77 or later
- macOS 12.0 or later (for GPUI support)
- Node.js (for Claude Code agent)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/0xd219b/cocowork.git
cd cocowork

# Build and run
cargo run --package cocowork-ui

# Or build a release version
cargo build --release
```

### Installing Agents

Each agent needs to be installed separately:

```bash
# Claude Code (requires Node.js)
npm install -g @anthropic-ai/claude-code

# Gemini CLI
# Follow instructions at https://github.com/google-gemini/gemini-cli

# Codex - no manual installation needed!
# CocoWork automatically downloads the codex-acp binary from
# https://github.com/zed-industries/codex-acp
# Just set CODEX_API_KEY or OPEN_AI_API_KEY environment variable
```

## Development

### Commands

```bash
# Build all crates
cargo build

# Run the application
cargo run --package cocowork-ui

# Run tests
cargo test --workspace

# Run linter
cargo clippy --workspace

# Check compilation
cargo check --workspace
```

### Project Structure

```
cocowork/
├── crates/
│   ├── cocowork-core/     # Core library (ACP, agents, storage)
│   └── cocowork-ui/       # GPUI desktop application
├── assets/
│   ├── icons/             # SVG icons
│   └── images/            # Logo and images
└── document/              # Design documents
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      cocowork-ui (GPUI)                         │
│  ├── window/          Main window + 3-panel layout              │
│  ├── components/      Reusable UI components                    │
│  ├── acp_integration  Bridge between GPUI and ACP               │
│  └── theme/           Colors, typography, layout constants      │
├─────────────────────────────────────────────────────────────────┤
│                      cocowork-core                              │
│  ├── acp/             ACP client, protocol, session manager     │
│  ├── agent/           Agent adapters and lifecycle              │
│  ├── sandbox/         File permissions, watcher                 │
│  ├── storage/         SQLite persistence                        │
│  └── types/           Shared type definitions                   │
└─────────────────────────────────────────────────────────────────┘
```

### ACP Protocol Flow

```
initialize → session/new → session/prompt → streaming session/update → prompt_response
```

## Acknowledgments

CocoWork would not be possible without the incredible work of the open-source community.

### Special Thanks to Zed

We extend our deepest gratitude to the [Zed](https://github.com/zed-industries/zed) team for their outstanding work on:

- **[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)**: The powerful, high-performance GPU-accelerated UI framework that powers CocoWork's interface
- **Architecture Patterns**: Many of our UI components and patterns are inspired by Zed's elegant codebase
- **ACP Bridges**: The [`@zed-industries/claude-code-acp`](https://github.com/nicolo-ribaudo/claude-code-acp) package enables Claude Code integration, and [`codex-acp`](https://github.com/zed-industries/codex-acp) enables Codex integration

Zed is an exceptional code editor, and we highly recommend checking it out at [zed.dev](https://zed.dev).

### Other Acknowledgments

- [Anthropic](https://anthropic.com) for Claude and the Agent Client Protocol specification
- The Rust community for the amazing ecosystem of crates

## License

CocoWork is licensed under the [GNU General Public License v3.0](LICENSE).

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

---

<p align="center">
  Made with Rust and GPUI
</p>
