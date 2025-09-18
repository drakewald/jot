use crossterm::event::KeyCode;
use std::fs;
use std::io;
use std::path::PathBuf;

pub struct Zipper {
    before: Vec<char>,
    after: Vec<char>,
}

impl Zipper {
    pub fn new() -> Self {
        Zipper {
            before: Vec::new(),
            after: Vec::new(),
        }
    }

    pub fn from_str(text: &str) -> Self {
        Zipper {
            before: Vec::new(),
            after: text.chars().rev().collect(),
        }
    }

    pub fn move_left(&mut self) {
        if let Some(c) = self.before.pop() {
            self.after.push(c);
        }
    }

    pub fn move_right(&mut self) {
        if let Some(c) = self.after.pop() {
            self.after.push(c);
        }
    }

    pub fn insert(&mut self, c: char) {
        self.before.push(c);
    }

    pub fn delete(&mut self) {
        self.before.pop();
    }

    pub fn cursor_position(&self) -> usize {
        self.before.len()
    }

    pub fn set_cursor_position(&mut self, pos: usize) {
        let mut content: Vec<char> = self.before.clone();
        content.extend(self.after.iter().rev());

        let (before, after) = content.split_at(pos);
        self.before = before.to_vec();
        self.after = after.iter().rev().cloned().collect();
    }

    pub fn to_string(&self) -> String {
        let mut result = String::new();
        result.extend(self.before.iter());
        result.extend(self.after.iter().rev());
        result
    }
}
#[derive(PartialEq, Eq)]
pub enum Mode {
    Edit,
    Command,
    PromptSave,
    PromptSaveAndQuit,
}

pub struct Page {
    pub before: Vec<String>,
    pub current: Zipper,
    pub after: Vec<String>,
    pub file_path: Option<PathBuf>,
    pub mode: Mode,
    pub command_buffer: String,
    pub status_message: String,
}

impl Page {
    pub fn new() -> Self {
        Page {
            before: Vec::new(),
            current: Zipper::new(),
            after: Vec::new(),
            file_path: None,
            mode: Mode::Edit,
            command_buffer: String::new(),
            status_message: String::new(),
        }
    }

    pub fn from_file(path: Option<PathBuf>) -> Self {
        let mut page = Self::new();
        page.file_path = path;
        if let Some(p) = &page.file_path {
            if let Ok(contents) = fs::read_to_string(p) {
                page.load_from_string(&contents);
            }
        }
        page
    }

    fn load_from_string(&mut self, contents: &str) {
        let mut lines: Vec<String> = contents.lines().map(String::from).collect();
        if lines.is_empty() {
            self.before = Vec::new();
            self.current = Zipper::new();
            self.after = Vec::new();
        } else {
            self.current = Zipper::from_str(&lines.remove(0));
            self.before = Vec::new();
            self.after = lines;
        }
    }

    pub fn move_up(&mut self) {
        if !self.before.is_empty() {
            let prev_line = self.before.pop().unwrap();
            self.after.insert(0, self.current.to_string());
            self.current = Zipper::from_str(&prev_line);
        }
    }

    pub fn move_down(&mut self) {
        if !self.after.is_empty() {
            let next_line = self.after.remove(0);
            self.before.push(self.current.to_string());
            self.current = Zipper::from_str(&next_line);
        }
    }

    pub fn insert_newline(&mut self) {
        let current_line = self.current.to_string();
        let (left, right) = current_line.split_at(self.current.cursor_position());
        self.current = Zipper::from_str(left);
        self.after.insert(0, right.to_string());
        self.move_down();
        self.current.set_cursor_position(0);
    }

    pub fn delete(&mut self) {
        if self.current.cursor_position() == 0 && !self.before.is_empty() {
            let prev_line = self.before.pop().unwrap();
            let prev_line_len = prev_line.len();
            let current_line = self.current.to_string();
            let merged_line = prev_line + &current_line;
            self.current = Zipper::from_str(&merged_line);
            self.current.set_cursor_position(prev_line_len);
        } else {
            self.current.delete();
        }
    }

    pub fn move_cursor_to(&mut self, row: usize, col: usize) {
        let mut lines = self.get_all_lines();
        let target_row = row.min(lines.len() - 1);

        let after_lines = lines.split_off(target_row + 1);
        let current_line = lines.pop().unwrap_or_default();
        let before_lines = lines;

        self.before = before_lines;
        self.after = after_lines;
        self.current = Zipper::from_str(&current_line);

        let target_col = col.min(self.current.to_string().chars().count());
        self.current.set_cursor_position(target_col);
    }

    pub fn get_all_lines(&self) -> Vec<String> {
        let mut lines = self.before.clone();
        lines.push(self.current.to_string());
        lines.extend(self.after.clone());
        lines
    }

    pub fn cursor_row(&self) -> usize {
        self.before.len()
    }

    pub fn handle_event(&mut self, key_code: KeyCode) -> bool {
        self.status_message.clear();
        match key_code {
            KeyCode::Esc => match self.mode {
                Mode::Edit => self.mode = Mode::Command,
                Mode::Command => self.mode = Mode::Edit,
                Mode::PromptSave | Mode::PromptSaveAndQuit => {
                    self.status_message = "Save cancelled.".to_string();
                    self.command_buffer.clear();
                    self.mode = Mode::Command;
                }
            },
            KeyCode::Char(c) => match self.mode {
                Mode::Edit => self.current.insert(c),
                Mode::Command | Mode::PromptSave | Mode::PromptSaveAndQuit => {
                    self.command_buffer.push(c)
                }
            },
            KeyCode::Backspace => match self.mode {
                Mode::Edit => self.delete(),
                Mode::Command | Mode::PromptSave | Mode::PromptSaveAndQuit => {
                    self.command_buffer.pop();
                }
            },
            KeyCode::Enter => match self.mode {
                Mode::Edit => self.insert_newline(),
                Mode::Command => {
                    let parts: Vec<&str> = self.command_buffer.split_whitespace().collect();
                    let command = parts.get(0).cloned().unwrap_or("");
                    let arg = parts.get(1).cloned();

                    match command {
                        "q" | "quit" => return false,
                        "h" | "help" => {
                            self.status_message =
                                "Commands: q(uit), h(elp), w(rite), wq(rite & quit), r(evert)"
                                    .to_string();
                            self.command_buffer.clear();
                            return true;
                        }
                        "r" | "revert" => {
                            if let Some(path) = &self.file_path {
                                if let Ok(contents) = fs::read_to_string(path) {
                                    self.load_from_string(&contents);
                                    self.status_message = "Reverted to saved version.".to_string();
                                } else {
                                    self.status_message =
                                        format!("Error reading file: {}", path.display());
                                }
                            } else {
                                self.status_message = "No file to revert from.".to_string();
                            }
                        }
                        "w" | "write" => {
                            let path_to_write =
                                arg.map(PathBuf::from).or_else(|| self.file_path.clone());
                            if let Some(path) = path_to_write {
                                match fs::write(&path, self.get_all_lines().join("\n")) {
                                    Ok(_) => {
                                        self.status_message = format!("Saved to {}", path.display());
                                        self.file_path = Some(path);
                                    }
                                    Err(e) => self.status_message = format!("Error: {}", e),
                                }
                            } else {
                                self.command_buffer.clear();
                                self.mode = Mode::PromptSave;
                                return true;
                            }
                        }
                        "wq" => {
                            let path_to_write =
                                arg.map(PathBuf::from).or_else(|| self.file_path.clone());
                            if let Some(path) = path_to_write {
                                match fs::write(&path, self.get_all_lines().join("\n")) {
                                    Ok(_) => return false,
                                    Err(e) => self.status_message = format!("Error: {}", e),
                                }
                            } else {
                                self.command_buffer.clear();
                                self.mode = Mode::PromptSaveAndQuit;
                                return true;
                            }
                        }
                        _ => {
                            self.status_message =
                                format!("Unknown command: {}", self.command_buffer);
                        }
                    }
                    self.command_buffer.clear();
                    self.mode = Mode::Command;
                }
                Mode::PromptSave | Mode::PromptSaveAndQuit => {
                    if !self.command_buffer.is_empty() {
                        let path = PathBuf::from(&self.command_buffer);
                        match fs::write(&path, self.get_all_lines().join("\n")) {
                            Ok(_) => {
                                if self.mode == Mode::PromptSaveAndQuit {
                                    return false;
                                }
                                self.status_message = format!("Saved to {}", path.display());
                                self.file_path = Some(path);
                                self.mode = Mode::Command;
                            }
                            Err(e) => {
                                self.status_message = format!("Error: {}", e);
                                self.mode = Mode::Command;
                            }
                        }
                        self.command_buffer.clear();
                    }
                }
            },
            KeyCode::Left => self.current.move_left(),
            KeyCode::Right => self.current.move_right(),
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            _ => {}
        }
        true
    }
}

