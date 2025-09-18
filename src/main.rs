use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use jot::{core::App, ui};
use std::{env, io, path::PathBuf};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let args: Vec<String> = env::args().collect();
    let file_path = args.get(1).map(PathBuf::from);
    let mut app = App::new(file_path)?;

    loop {
        ui::draw_ui(&mut stdout, &app)?;

        let event = event::read()?;
        let (width, _) = crossterm::terminal::size()?;
        app.handle_event(event, width);

        if app.should_quit {
            break;
        }
    }

    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    disable_raw_mode()?;
    Ok(())
}

