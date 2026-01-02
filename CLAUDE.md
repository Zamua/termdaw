# TUI DAW - Claude Code Instructions

## Shell Commands

- Do NOT use `timeout` command - it's not available on this Mac
- Use `cargo run -- <project>` to run the Rust app

## Package Manager

This project uses **bun** as the package manager for the TypeScript version (deprecated).

## Rust Version

The main app is now in Rust. Use:

```bash
# Build
cargo build

# Run with a project
cargo run -- test-song

# Run tests
cargo test
```

## Project Overview

A terminal-based Digital Audio Workstation (DAW) built with Ratatui (Rust TUI). Features vim-style navigation with terminal aesthetics.

## Key Technologies

- **Ratatui** - Rust TUI framework
- **cpal** - Low-level audio with callback-based mixing
- **Crossterm** - Terminal handling
- **nih-plug** - Plugin creation (CLAP/VST3)

## Vim Keybinding Architecture

Grid components (channel rack, piano roll, playlist) MUST route keys through `src/input/vim.rs`.

**Pattern**: Handle component-specific keys first (m, J/K, </>), then call `app.vim.process_key()` and execute returned `VimAction`s. See `handle_channel_rack_key` for reference.

**NEVER** reimplement: hjkl, w/b/e, 0/$, gg/G, d/y/c, v, x. These are vim's job.
