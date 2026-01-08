//! TermDAW - A terminal-based Digital Audio Workstation
//!
//! This is the main entry point for the application.

#![deny(warnings)]

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use termdaw::app::App;
use termdaw::audio::AudioEngine;
use termdaw::input;
use termdaw::ui;

/// TermDAW - Terminal Digital Audio Workstation
#[derive(Parser, Debug)]
#[command(name = "termdaw")]
#[command(about = "A terminal-based Digital Audio Workstation", long_about = None)]
struct Args {
    /// Project name or path to open/create (auto-generates if not specified)
    #[arg()]
    project: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Ensure templates are downloaded (first run setup)
    if let Err(e) = termdaw::templates::ensure_templates() {
        eprintln!("Warning: Could not download templates: {}", e);
        eprintln!("You can still use the app, but new projects won't have starter content.");
        eprintln!("Press Enter to continue...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }

    // Determine project name: use provided name or auto-generate
    let project_name = args
        .project
        .unwrap_or_else(termdaw::project::generate_project_name);

    // Initialize audio engine
    let (mut audio_engine, audio_handle) =
        AudioEngine::new().map_err(|e| anyhow::anyhow!("Failed to initialize audio: {}", e))?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Try to enable keyboard enhancement for key release events (not supported on all terminals)
    let keyboard_enhancement_enabled = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    )
    .is_ok();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state with audio handle
    let mut app = App::new(&project_name, audio_handle);

    // Run the main loop
    let result = run_app(&mut terminal, &mut app, &mut audio_engine);

    // Restore terminal
    disable_raw_mode()?;
    if keyboard_enhancement_enabled {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Propagate any errors
    result
}

/// Main application loop
fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    audio_engine: &mut AudioEngine,
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let mut last_tick = Instant::now();

    loop {
        // Draw the UI
        terminal.draw(|frame| ui::render(frame, app))?;

        // Drain all pending events (prevents input queue buildup during slow renders)
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key_event) => {
                    if input::handle_key(key_event, app) {
                        // App requested quit
                        return Ok(());
                    }
                }
                Event::Mouse(mouse_event) => {
                    input::handle_mouse(mouse_event, app);
                }
                Event::Resize(width, height) => {
                    app.on_resize(width, height);
                }
                _ => {}
            }
        }

        // Calculate delta time for transport
        let now = Instant::now();
        let delta = now - last_tick;
        last_tick = now;

        // Update app state (transport timing, playhead, etc.)
        app.tick(delta);

        // Process audio commands
        audio_engine.process_commands();

        // Auto-save if needed
        app.maybe_auto_save();
    }
}
