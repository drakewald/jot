use std::{
    env,
    io::{self, stdout},
    path::PathBuf,
    time::Duration,
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};

// Use the library crate, assuming the package name in Cargo.toml is 'jot'
use jot::{core, ui};

fn main() -> io::Result<()> {
    // --- Terminal setup ---
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // --- App setup ---
    let args: Vec<String> = env::args().collect();
    let file_path = args.get(1).map(PathBuf::from);
    let mut app = core::App::new(file_path)?;

    // --- Main loop ---
    loop {
        ui::draw_ui(&mut stdout, &app)?;

        if event::poll(Duration::from_millis(500))? {
            let event = event::read()?;
            match event {
                Event::Key(key_event) if key_event.kind == event::KeyEventKind::Press => {
                    app.handle_event(key_event);
                }
                Event::Mouse(MouseEvent {
                    kind,
                    column,
                    row,
                    ..
                }) => {
                    if app.active_pane == core::ActivePane::Editor {
                        if let MouseEventKind::Down(_) = kind {
                            if let Some(page) = app.get_active_page() {
                                let (width, _) = crossterm::terminal::size()?;
                                let file_tree_width = (width as f32 * 0.25).round() as u16;
                                let editor_start_col = file_tree_width + 1;
                                let line_gutter_width =
                                    page.get_all_lines().len().to_string().len() + 2;

                                if column >= editor_start_col + line_gutter_width as u16 {
                                    let adjusted_col = column
                                        .saturating_sub(editor_start_col + line_gutter_width as u16);
                                    let adjusted_row = row.saturating_sub(1); // for tab bar
                                    page.move_cursor_to(
                                        adjusted_row as usize,
                                        adjusted_col as usize,
                                    );
                                }
                            }
                        }
                    }
                }
                Event::Resize(_, _) => { /* Handled by redrawing */ }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    // --- Terminal cleanup ---
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    disable_raw_mode()?;
    Ok(())
}

