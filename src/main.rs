use jot::{Mode, Page};
use crossterm::{
    self,
    cursor::MoveTo,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use std::env;
use std::io::{self, stdout, Write};
use std::path::PathBuf;

fn draw_screen(stdout: &mut io::Stdout, editor: &Page) -> io::Result<()> {
    let line_gutter_width = editor.get_all_lines().len().to_string().len() + 2;

    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

    for (i, line) in editor.get_all_lines().iter().enumerate() {
        write!(stdout, "{: >width$} {}\r\n", i + 1, line, width = line_gutter_width - 1)?;
    }

    let status_text = match editor.mode {
        Mode::Edit => format!(
            "--EDIT-- {} {}",
            editor
                .file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|f| f.to_str())
                .unwrap_or("[No Name]"),
            editor.status_message
        ),
        // --- FIX START ---
        // If there's a status message, show it. Otherwise, show the command buffer.
        Mode::Command => {
            if !editor.status_message.is_empty() {
                editor.status_message.clone()
            } else {
                format!(":{}", editor.command_buffer)
            }
        }
        // --- FIX END ---
        Mode::PromptSave => {
            format!("Enter filename to save: {}", editor.command_buffer)
        }
        Mode::PromptSaveAndQuit => {
            format!("Enter filename to save and quit: {}", editor.command_buffer)
        }
    };

    let (width, height) = crossterm::terminal::size()?;
    execute!(stdout, MoveTo(0, height.saturating_sub(1)))?;
    write!(stdout, "{:width$}", status_text, width = width as usize)?;

    let cursor_col = editor.current.cursor_position() as u16 + line_gutter_width as u16;
    let cursor_row = editor.cursor_row() as u16;
    execute!(stdout, MoveTo(cursor_col, cursor_row))?;

    stdout.flush()
}


fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let args: Vec<String> = env::args().collect();
    let file_path = args.get(1).map(PathBuf::from);

    let mut editor = Page::from_file(file_path);

    loop {
        draw_screen(&mut stdout, &editor)?;

        let event = event::read()?;
        let should_continue = match event {
            Event::Key(key_event) => editor.handle_event(key_event.code),
            Event::Mouse(MouseEvent { kind, column, row, .. }) => {
                if let MouseEventKind::Down(_) = kind {
                    let line_gutter_width = editor.get_all_lines().len().to_string().len() + 2;
                    let adjusted_col = column.saturating_sub(line_gutter_width as u16);
                    editor.move_cursor_to(row as usize, adjusted_col as usize);
                }
                true
            }
            _ => true,
        };

        if !should_continue {
            break;
        }
    }

    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    disable_raw_mode()?;
    Ok(())
}

