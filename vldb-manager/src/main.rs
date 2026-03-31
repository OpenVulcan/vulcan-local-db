mod app;
mod installed;
mod service;
mod ui;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::prelude::CrosstermBackend;

fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let run_result = run(&mut terminal);
    let restore_result = restore_terminal(&mut terminal);

    if let Err(error) = restore_result {
        return Err(error.into());
    }

    run_result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let mut app = App::new()?;

    while !app.should_quit() {
        app.drain_background_events();
        app.tick();

        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(Duration::from_millis(120))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key)?;
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    app.shutdown();
    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}
