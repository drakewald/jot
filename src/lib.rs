use crossterm::{
    cursor::MoveTo,
    event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    queue,
    terminal::{Clear, ClearType, DisableLineWrap, EnableLineWrap},
};
use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
};

/// Core application logic, state, and text editing structures.
pub mod core {
    use super::*;

    // Zipper remains unchanged as its logic for line editing is solid.
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
                self.before.push(c);
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
            let (before, after) = content.split_at(pos.min(content.len()));
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

    /// Represents the state of a single open file buffer (a "tab").
    pub struct Page {
        pub before: Vec<String>,
        pub current: Zipper,
        pub after: Vec<String>,
        pub file_path: Option<PathBuf>,
        pub scroll_offset: usize,
        pub horizontal_scroll_offset: usize,
    }

    impl Page {
        pub fn new() -> Self {
            Page {
                before: Vec::new(),
                current: Zipper::new(),
                after: Vec::new(),
                file_path: None,
                scroll_offset: 0,
                horizontal_scroll_offset: 0,
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

        pub fn load_from_string(&mut self, contents: &str) {
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
                let cursor_pos = self.current.cursor_position();
                let prev_line = self.before.pop().unwrap();
                self.after.insert(0, self.current.to_string());
                self.current = Zipper::from_str(&prev_line);
                self.current.set_cursor_position(cursor_pos);
            }
        }

        pub fn move_down(&mut self) {
            if !self.after.is_empty() {
                let cursor_pos = self.current.cursor_position();
                let next_line = self.after.remove(0);
                self.before.push(self.current.to_string());
                self.current = Zipper::from_str(&next_line);
                self.current.set_cursor_position(cursor_pos);
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
            let target_row = row.min(lines.len().saturating_sub(1));

            let after_lines = lines.split_off(target_row + 1);
            let current_line = lines.pop().unwrap_or_default();
            let before_lines = lines;

            self.before = before_lines;
            self.after = after_lines;
            self.current = Zipper::from_str(&current_line);

            self.current.set_cursor_position(col);
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
    }

    /// Represents the state of the file tree view.
    pub struct DirectoryView {
        pub path: PathBuf,
        pub entries: Vec<fs::DirEntry>,
        pub selected_index: usize,
        pub scroll_offset: usize,
    }

    impl DirectoryView {
        pub fn new(path: PathBuf) -> io::Result<Self> {
            let mut entries = fs::read_dir(&path)?.filter_map(Result::ok).collect::<Vec<_>>();
            entries.sort_by_key(|a| {
                (
                    !a.path().is_dir(),
                    a.path().file_name().unwrap_or_default().to_ascii_lowercase(),
                )
            });
            Ok(Self {
                path,
                entries,
                selected_index: 0,
                scroll_offset: 0,
            })
        }

        pub fn move_up(&mut self) {
            self.selected_index = self.selected_index.saturating_sub(1);
        }

        pub fn move_down(&mut self) {
            if !self.entries.is_empty() {
                self.selected_index = (self.selected_index + 1).min(self.entries.len() - 1);
            }
        }
    }

    /// Global application modes.
    #[derive(PartialEq, Eq, Clone, Copy)]
    pub enum Mode {
        Command,
        Edit,
        FileTree,
        PromptSave,
        PromptSaveAndQuit,
        Find,
        ConfirmDelete,
        PromptNewFile,
        PromptNewDirectory,
        PromptRename,
    }

    /// The currently focused UI pane.
    #[derive(PartialEq, Eq)]
    pub enum ActivePane {
        FileTree,
        Editor,
    }

    /// The main struct holding all application state.
    pub struct App {
        pub tabs: Vec<Page>,
        pub active_tab_index: usize,
        pub directory_view: DirectoryView,
        pub active_pane: ActivePane,
        pub mode: Mode,
        pub command_buffer: String,
        pub status_message: String,
        pub should_quit: bool,
        pub find_query: String,
        pub find_matches: Vec<(usize, usize)>,
        pub current_match_index: usize,
        pub path_to_delete: Option<PathBuf>,
        pub path_to_rename: Option<PathBuf>,
        pub find_navigation_active: bool,
    }

    impl App {
        pub fn new(initial_path: Option<PathBuf>) -> io::Result<Self> {
            let directory_view = DirectoryView::new(env::current_dir()?)?;
            let mut tabs = Vec::new();
            let mut active_pane = ActivePane::FileTree;
            let mut mode = Mode::FileTree;

            if let Some(path) = initial_path {
                tabs.push(Page::from_file(Some(path)));
                active_pane = ActivePane::Editor;
                mode = Mode::Edit; // Default to Edit mode when opening a file from CLI
            }
            // If no path, tabs vec remains empty, showing the logo.

            Ok(Self {
                tabs,
                active_tab_index: 0,
                directory_view,
                active_pane,
                mode,
                command_buffer: String::new(),
                status_message: String::new(),
                should_quit: false,
                find_query: String::new(),
                find_matches: Vec::new(),
                current_match_index: 0,
                path_to_delete: None,
                path_to_rename: None,
                find_navigation_active: false,
            })
        }

        /// Central event handler for the entire application.
        pub fn handle_event(&mut self, event: Event, term_width: u16, term_height: u16) {
            self.status_message.clear();
            match event {
                Event::Key(key_event) => self.handle_key_event(key_event, term_width, term_height),
                Event::Mouse(mouse_event) => {
                    self.handle_mouse_event(mouse_event, term_width, term_height)
                }
                _ => {}
            }
        }

        fn handle_mouse_event(&mut self, event: MouseEvent, term_width: u16, term_height: u16) {
            let MouseEvent { kind, column, row, .. } = event;
            let file_tree_width = (term_width as f32 * 0.25).round() as u16;

            match kind {
                MouseEventKind::ScrollUp => {
                    if column < file_tree_width {
                        self.directory_view.scroll_offset =
                            self.directory_view.scroll_offset.saturating_sub(1);
                    } else if let Some(page) = self.get_active_page() {
                        page.scroll_offset = page.scroll_offset.saturating_sub(1);
                    }
                }
                MouseEventKind::ScrollDown => {
                    if column < file_tree_width {
                        let view_height = term_height.saturating_sub(2) as usize;
                        if self.directory_view.entries.len() > view_height {
                            self.directory_view.scroll_offset = (self.directory_view.scroll_offset + 1)
                                .min(self.directory_view.entries.len() - view_height);
                        }
                    } else if let Some(page) = self.get_active_page() {
                        let total_lines = page.get_all_lines().len();
                        let view_height = term_height.saturating_sub(2) as usize;
                        if total_lines > view_height {
                            page.scroll_offset =
                                (page.scroll_offset + 1).min(total_lines - view_height);
                        }
                    }
                }
                MouseEventKind::Down(_) => {
                    // 1. Check for File Tree Click
                    if column < file_tree_width {
                        self.active_pane = ActivePane::FileTree;
                        self.mode = Mode::FileTree;
                        // row 0 is header. Clicks are relative to view + scroll offset.
                        let target_index = row.saturating_sub(1) as usize + self.directory_view.scroll_offset;
                        if !self.directory_view.entries.is_empty() {
                            let max_index = self.directory_view.entries.len().saturating_sub(1);
                            self.directory_view.selected_index = target_index.min(max_index);
                        }
                        return;
                    }

                    let editor_start_col = file_tree_width + 1;

                    // 2. Check for Tab Bar Click
                    if row == 0 && column >= editor_start_col && !self.tabs.is_empty() {
                        let mut current_col = editor_start_col;
                        for (i, page) in self.tabs.iter().enumerate() {
                            let file_name = page
                                .file_path
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .and_then(|f| f.to_str())
                                .unwrap_or("[No Name]");
                            let tab_text = format!(" {} ", file_name);
                            let tab_width = tab_text.len() as u16;

                            if column >= current_col && column < current_col + tab_width {
                                self.active_tab_index = i;
                                break;
                            }
                            current_col += tab_width;
                        }
                        return;
                    }

                    // 3. Check for Editor Content Click
                    if row > 0 && column >= editor_start_col && !self.tabs.is_empty() {
                        self.active_pane = ActivePane::Editor;
                        self.mode = Mode::Edit;

                        if let Some(page) = self.get_active_page() {
                            let line_gutter_width = page.get_all_lines().len().to_string().len() + 2;
                            let adjusted_row = row.saturating_sub(1) as usize + page.scroll_offset;
                            let adjusted_col = column
                                .saturating_sub(editor_start_col + line_gutter_width as u16)
                                as usize;
                            page.move_cursor_to(adjusted_row, adjusted_col);
                        }
                    }
                }
                _ => {}
            }
        }

        fn scroll_to_cursor(&mut self, term_width: u16, term_height: u16) {
            let file_tree_width = (term_width as f32 * 0.25).round() as u16;
            let view_height = term_height as usize;

            match self.active_pane {
                ActivePane::Editor => {
                    if let Some(page) = self.get_active_page() {
                        let cursor_row = page.cursor_row();
                        let scroll_offset = page.scroll_offset;
                        let editor_view_height = view_height.saturating_sub(2);

                        // Vertical scroll logic
                        if cursor_row < scroll_offset {
                            page.scroll_offset = cursor_row;
                        } else if cursor_row >= scroll_offset + editor_view_height {
                            page.scroll_offset = cursor_row - editor_view_height + 1;
                        }

                        // Horizontal scroll logic
                        let cursor_col = page.current.cursor_position();
                        let h_scroll_offset = page.horizontal_scroll_offset;
                        let line_gutter_width = page.get_all_lines().len().to_string().len() + 2;
                        let editor_width = term_width.saturating_sub(file_tree_width).saturating_sub(1);
                        let editor_text_area_width = editor_width.saturating_sub(line_gutter_width as u16) as usize;

                        if cursor_col < h_scroll_offset {
                            page.horizontal_scroll_offset = cursor_col;
                        } else if cursor_col >= h_scroll_offset + editor_text_area_width {
                            page.horizontal_scroll_offset = cursor_col - editor_text_area_width + 1;
                        }
                    }
                }
                ActivePane::FileTree => {
                    let selected_index = self.directory_view.selected_index;
                    let scroll_offset = self.directory_view.scroll_offset;
                    let file_tree_view_height = view_height.saturating_sub(2);

                    if selected_index < scroll_offset {
                        self.directory_view.scroll_offset = selected_index;
                    } else if selected_index >= scroll_offset + file_tree_view_height {
                        self.directory_view.scroll_offset = selected_index - file_tree_view_height + 1;
                    }
                }
            }
        }

        fn handle_key_event(&mut self, event: KeyEvent, term_width: u16, term_height: u16) {
            if self.mode == Mode::ConfirmDelete {
                self.handle_delete_confirm_event(event.code);
                return;
            }

            if self.mode == Mode::PromptNewFile
                || self.mode == Mode::PromptNewDirectory
                || self.mode == Mode::PromptRename
            {
                self.handle_prompt_input_event(event.code);
                return;
            }
            
            match self.active_pane {
                ActivePane::Editor => self.handle_editor_event(event),
                ActivePane::FileTree => self.handle_file_tree_event(event.code),
            };
            self.scroll_to_cursor(term_width, term_height);
        }

        fn handle_file_tree_event(&mut self, key_code: KeyCode) {
            match key_code {
                // Navigation
                KeyCode::Up | KeyCode::Char('k') => self.directory_view.move_up(),
                KeyCode::Down | KeyCode::Char('j') => self.directory_view.move_down(),

                // Actions that clear buffer
                KeyCode::Left => {
                    self.go_up_directory();
                    self.command_buffer.clear();
                },
                KeyCode::Right | KeyCode::Char('l') => {
                    self.open_selected_entry();
                    self.command_buffer.clear();
                },
                KeyCode::Enter => {
                    if self.command_buffer.is_empty() {
                        self.open_selected_entry();
                    } else {
                        let cmd = self.command_buffer.clone();
                        self.command_buffer.clear();
                        match cmd.as_str() {
                            "d" => self.prompt_for_delete(),
                            "nf" => self.mode = Mode::PromptNewFile,
                            "nd" => self.mode = Mode::PromptNewDirectory,
                            "rn" => self.prompt_for_rename(),
                            _ => {
                                self.status_message = format!("Unknown command: {}", cmd);
                            }
                        }
                    }
                },

                // Mode Switching
                KeyCode::Esc => {
                    self.active_pane = ActivePane::Editor;
                    self.mode = Mode::Command;
                    self.command_buffer.clear();
                },
                KeyCode::Tab => {
                    self.active_pane = ActivePane::Editor;
                    if self.tabs.is_empty() {
                        self.mode = Mode::Command;
                    } else {
                        self.mode = Mode::Edit;
                    }
                    self.command_buffer.clear();
                },
                
                // Command Input
                KeyCode::Char(c) => {
                    self.command_buffer.push(c);
                },
                KeyCode::Backspace => {
                    self.command_buffer.pop();
                },

                _ => {}
            }
        }


        fn prompt_for_delete(&mut self) {
            if let Some(entry) = self.directory_view.entries.get(self.directory_view.selected_index) {
                let path = entry.path();
                self.path_to_delete = Some(path);
                self.mode = Mode::ConfirmDelete;
            }
        }

        fn handle_delete_confirm_event(&mut self, key_code: KeyCode) {
            match key_code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(path) = self.path_to_delete.take() {
                        let result = if path.is_dir() {
                            fs::remove_dir_all(&path)
                        } else {
                            fs::remove_file(&path)
                        };

                        match result {
                            Ok(_) => {
                                self.status_message = format!("Deleted {}", path.display());
                                self.tabs.retain(|page| {
                                    if let Some(page_path) = &page.file_path {
                                        !page_path.starts_with(&path)
                                    } else {
                                        true
                                    }
                                });
                                if self.tabs.is_empty() {
                                    self.mode = Mode::Command;
                                    self.active_tab_index = 0;
                                } else if self.active_tab_index >= self.tabs.len() {
                                    self.active_tab_index = self.tabs.len() - 1;
                                }

                                let current_dir = self.directory_view.path.clone();
                                if let Ok(new_view) = DirectoryView::new(current_dir) {
                                    self.directory_view = new_view;
                                }
                            }
                            Err(e) => {
                                self.status_message = format!("Error deleting: {}", e);
                            }
                        }
                    }
                    self.mode = Mode::FileTree;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.path_to_delete = None;
                    self.status_message = "Delete cancelled.".to_string();
                    self.mode = Mode::FileTree;
                }
                _ => {}
            }
        }

        fn handle_prompt_input_event(&mut self, key_code: KeyCode) {
            match key_code {
                KeyCode::Esc => {
                    self.status_message = "Cancelled.".to_string();
                    self.command_buffer.clear();
                    self.mode = Mode::FileTree;
                }
                KeyCode::Char(c) => self.command_buffer.push(c),
                KeyCode::Backspace => {
                    self.command_buffer.pop();
                }
                KeyCode::Enter => {
                    if !self.command_buffer.is_empty() {
                        let name = self.command_buffer.clone();
                        let current_mode = self.mode;
                        self.command_buffer.clear();

                        if current_mode == Mode::PromptRename {
                           self.execute_rename(name);
                        } else {
                            self.execute_new_item(name, current_mode);
                        }
                    }
                }
                _ => {}
            }
        }

        fn execute_rename(&mut self, new_name: String) {
            if let Some(old_path) = self.path_to_rename.take() {
                let mut new_path = old_path.clone();
                new_path.set_file_name(new_name);

                match fs::rename(&old_path, &new_path) {
                    Ok(_) => {
                        self.status_message = format!("Renamed to {}", new_path.display());
                        for tab in self.tabs.iter_mut() {
                            if tab.file_path.as_ref() == Some(&old_path) {
                                tab.file_path = Some(new_path.clone());
                            }
                        }
                        let current_dir = self.directory_view.path.clone();
                        if let Ok(new_view) = DirectoryView::new(current_dir) {
                            self.directory_view = new_view;
                        }
                    },
                    Err(e) => {
                        self.status_message = format!("Error: {}", e);
                    }
                }
            }
            self.mode = Mode::FileTree;
        }

        fn execute_new_item(&mut self, name: String, mode: Mode) {
            let mut path = self.directory_view.path.clone();
            path.push(name);

            let result = if mode == Mode::PromptNewFile {
                fs::write(&path, "")
            } else {
                fs::create_dir(&path)
            };

            match result {
                Ok(_) => {
                    self.status_message = format!("Created {}", path.display());
                    if mode == Mode::PromptNewFile {
                        self.tabs.push(Page::from_file(Some(path)));
                        self.active_tab_index = self.tabs.len() - 1;
                        self.active_pane = ActivePane::Editor;
                        self.mode = Mode::Edit;
                    } else {
                        self.mode = Mode::FileTree;
                    }
                    let current_dir = self.directory_view.path.clone();
                    if let Ok(new_view) = DirectoryView::new(current_dir) {
                        self.directory_view = new_view;
                    }
                },
                Err(e) => {
                    self.status_message = format!("Error: {}", e);
                    self.mode = Mode::FileTree;
                }
            }
        }

        fn prompt_for_rename(&mut self) {
            if let Some(entry) = self.directory_view.entries.get(self.directory_view.selected_index) {
                let path = entry.path();
                self.path_to_rename = Some(path);
                self.mode = Mode::PromptRename;
            }
        }


        fn go_up_directory(&mut self) {
            if let Some(parent) = self.directory_view.path.parent() {
                match DirectoryView::new(parent.to_path_buf()) {
                    Ok(new_view) => self.directory_view = new_view,
                    Err(_) => self.status_message = "Cannot access parent directory.".to_string(),
                }
            }
        }

        fn handle_editor_event(&mut self, event: KeyEvent) {
            if self.mode == Mode::Find {
                self.handle_find_event(event);
                return;
            }

            if self.mode == Mode::PromptSave || self.mode == Mode::PromptSaveAndQuit {
                self.handle_prompt_event(event.code);
                return;
            }

            match event.code {
                KeyCode::Esc => match self.mode {
                    Mode::Edit => self.mode = Mode::Command,
                    Mode::Command => {
                        if !self.tabs.is_empty() {
                            self.mode = Mode::Edit;
                            self.command_buffer.clear();
                        }
                    }
                    _ => {}
                },
                KeyCode::Char(c) => match self.mode {
                    Mode::Edit => {
                        if let Some(page) = self.get_active_page() {
                            page.current.insert(c);
                        }
                    }
                    Mode::Command => self.command_buffer.push(c),
                    _ => {}
                },
                KeyCode::Backspace => match self.mode {
                    Mode::Edit => {
                        if let Some(page) = self.get_active_page() {
                            page.delete();
                        }
                    }
                    Mode::Command => {
                        self.command_buffer.pop();
                    }
                    _ => {}
                },
                KeyCode::Enter => match self.mode {
                    Mode::Edit => {
                        if let Some(page) = self.get_active_page() {
                            page.insert_newline();
                        }
                    }
                    Mode::Command => {
                        self.execute_command();
                    }
                    _ => {}
                },
                KeyCode::Left => {
                    if self.mode == Mode::Command {
                        if self.tabs.len() > 1 {
                            self.active_tab_index =
                                (self.active_tab_index + self.tabs.len() - 1) % self.tabs.len();
                        }
                    } else if self.mode == Mode::Edit {
                        if let Some(p) = self.get_active_page() {
                            p.current.move_left()
                        }
                    }
                }
                KeyCode::Right => {
                    if self.mode == Mode::Command {
                        if self.tabs.len() > 1 {
                            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
                        }
                    } else if self.mode == Mode::Edit {
                        if let Some(p) = self.get_active_page() {
                            p.current.move_right()
                        }
                    }
                }
                KeyCode::Up => {
                    if self.mode == Mode::Edit {
                        if let Some(p) = self.get_active_page() {
                            p.move_up()
                        }
                    }
                }
                KeyCode::Down => {
                    if self.mode == Mode::Edit {
                        if let Some(p) = self.get_active_page() {
                            p.move_down()
                        }
                    }
                }
                KeyCode::Tab => {
                    self.active_pane = ActivePane::FileTree;
                    self.mode = Mode::FileTree;
                    self.command_buffer.clear();
                }
                _ => {}
            }
        }

        fn handle_find_event(&mut self, event: KeyEvent) {
            match event.code {
                KeyCode::Esc => {
                    self.mode = Mode::Command;
                    self.find_query.clear();
                    self.find_matches.clear();
                    self.find_navigation_active = false;
                }
                KeyCode::Enter => {
                    if !self.find_query.is_empty() {
                        self.find_navigation_active = true;
                        self.jump_to_match();
                    }
                }
                KeyCode::Char('n') if self.find_navigation_active && event.modifiers == KeyModifiers::NONE => {
                    self.jump_to_next_match();
                }
                KeyCode::Char('N') | KeyCode::Char('n') if self.find_navigation_active && event.modifiers == KeyModifiers::SHIFT => {
                    self.jump_to_prev_match();
                }
                KeyCode::Char(c) => {
                    if self.find_navigation_active {
                        self.find_query.clear();
                        self.find_navigation_active = false;
                    }
                    self.find_query.push(c);
                    self.update_search_matches();
                }
                KeyCode::Backspace => {
                    if !self.find_query.is_empty() {
                        self.find_navigation_active = false;
                        self.find_query.pop();
                        self.update_search_matches();
                    }
                }
                _ => {}
            }
        }
        
        fn update_search_matches(&mut self) {
            self.find_matches.clear();
            if self.find_query.is_empty() {
                return;
            }
            if let Some(page) = self.tabs.get(self.active_tab_index) {
                for (row, line) in page.get_all_lines().iter().enumerate() {
                    for (col, _) in line.match_indices(&self.find_query) {
                        self.find_matches.push((row, col));
                    }
                }
            }
            if !self.find_matches.is_empty() {
                self.current_match_index = 0;
                self.jump_to_match();
            }
        }

        fn jump_to_next_match(&mut self) {
            if !self.find_matches.is_empty() {
                self.current_match_index = (self.current_match_index + 1) % self.find_matches.len();
                self.jump_to_match();
            }
        }

        fn jump_to_prev_match(&mut self) {
            if !self.find_matches.is_empty() {
                self.current_match_index = (self.current_match_index + self.find_matches.len() - 1) % self.find_matches.len();
                self.jump_to_match();
            }
        }
        
        fn jump_to_match(&mut self) {
            let match_coords = self.find_matches.get(self.current_match_index).copied();
            if let Some((row, col)) = match_coords {
                if let Some(page) = self.get_active_page() {
                    page.move_cursor_to(row, col);
                }
            }
        }

        fn handle_prompt_event(&mut self, key_code: KeyCode) {
            match key_code {
                KeyCode::Esc => {
                    self.status_message = "Save cancelled.".to_string();
                    self.command_buffer.clear();
                    self.mode = Mode::Command;
                }
                KeyCode::Char(c) => self.command_buffer.push(c),
                KeyCode::Backspace => {
                    self.command_buffer.pop();
                }
                KeyCode::Enter => {
                    if !self.command_buffer.is_empty() {
                        let file_name = self.command_buffer.clone();
                        let mut path = self.directory_view.path.clone();
                        path.push(file_name);

                        let should_quit_after = self.mode == Mode::PromptSaveAndQuit;
                        let content = self
                            .get_active_page()
                            .map(|p| p.get_all_lines().join("\n"))
                            .unwrap_or_default();

                        match fs::write(&path, content) {
                            Ok(_) => {
                                if let Some(page) = self.get_active_page() {
                                    page.file_path = Some(path.clone());
                                }
                                self.status_message = format!("Saved to {}", path.display());
                                self.mode = Mode::Command;
                                if should_quit_after {
                                    self.should_quit = true;
                                }
                                // Refresh the directory view to show the new file.
                                let current_dir_path = self.directory_view.path.clone();
                                if let Ok(new_view) = DirectoryView::new(current_dir_path) {
                                    self.directory_view = new_view;
                                }
                            }
                            Err(e) => {
                                self.status_message = format!("Error: {}", e);
                                self.mode = Mode::Command;
                            }
                        }
                        self.command_buffer.clear();
                    }
                }
                _ => {}
            }
        }

        fn open_selected_entry(&mut self) {
            if let Some(entry) = self.directory_view.entries.get(self.directory_view.selected_index)
            {
                let path = entry.path();
                if path.is_dir() {
                    self.directory_view = DirectoryView::new(path).unwrap_or_else(|e| {
                        self.status_message = format!("Error: {}", e);
                        DirectoryView::new(self.directory_view.path.clone()).unwrap()
                    });
                } else {
                    // Check if the file is already open in a tab
                    if let Some(index) = self
                        .tabs
                        .iter()
                        .position(|p| p.file_path.as_ref() == Some(&path))
                    {
                        self.active_tab_index = index;
                    } else {
                        // If no tabs are open, replace the empty state.
                        if self.tabs.is_empty() {
                            self.tabs.push(Page::from_file(Some(path)));
                            self.active_tab_index = 0;
                        } else {
                            // Otherwise, add a new tab.
                            self.tabs.push(Page::from_file(Some(path)));
                            self.active_tab_index = self.tabs.len() - 1;
                        }
                    }
                    self.active_pane = ActivePane::Editor;
                    self.mode = Mode::Edit;
                }
            }
        }

        fn execute_command(&mut self) {
            let cmd_line = self.command_buffer.clone();
            let parts: Vec<&str> = cmd_line.split_whitespace().collect();
            let command = parts.get(0).cloned().unwrap_or("");
            let arg = parts.get(1).cloned();

            match command {
                "f" | "find" => {
                    self.mode = Mode::Find;
                    self.find_query.clear();
                }
                "q" | "quit" => {
                    if !self.tabs.is_empty() {
                        self.tabs.remove(self.active_tab_index);
                    }
                    if self.tabs.is_empty() {
                        self.mode = Mode::Command;
                        self.active_tab_index = 0;
                    } else if self.active_tab_index >= self.tabs.len() {
                        self.active_tab_index = self.tabs.len() - 1;
                    }
                }
                "x" | "exit" => {
                    self.should_quit = true;
                }
                "wx" => {
                    let mut errors = Vec::new();
                    for page in &self.tabs {
                        if let Some(path) = &page.file_path {
                            let content = page.get_all_lines().join("\n");
                            if let Err(e) = fs::write(path, content) {
                                errors.push(format!("{}: {}", path.display(), e));
                            }
                        }
                    }

                    if !errors.is_empty() {
                        self.status_message = format!("Errors saving files: {}", errors.join(", "));
                    } else {
                        self.status_message = "All files saved.".to_string();
                    }
                    self.should_quit = true;
                }
                "h" | "help" => {
                    self.status_message =
                        "Help | Modes: Esc (Cmd/Edit), Tab (Dir) | Cmds: f, q, w, wq, x, wx, r | Dir Cmds: nf, nd, rn, d"
                            .to_string();
                }
                "r" | "revert" => self.revert_active_file(),
                "w" | "write" => { self.save_active_file(arg, false); },
                "wq" => {
                    if self.save_active_file(arg, false) {
                        if !self.tabs.is_empty() {
                            self.tabs.remove(self.active_tab_index);
                        }
                        if self.tabs.is_empty() {
                            self.mode = Mode::Command;
                            self.active_tab_index = 0;
                        } else if self.active_tab_index >= self.tabs.len() {
                            self.active_tab_index = self.tabs.len() - 1;
                        }
                    }
                },
                _ => self.status_message = format!("Unknown command: {}", cmd_line),
            }
            self.command_buffer.clear();
        }

        fn revert_active_file(&mut self) {
            let file_path = self.get_active_page().and_then(|p| p.file_path.clone());
            if let Some(path) = file_path {
                if let Ok(contents) = fs::read_to_string(&path) {
                    if let Some(page) = self.get_active_page() {
                        page.load_from_string(&contents);
                        self.status_message = "Reverted to saved version.".to_string();
                    }
                } else {
                    self.status_message = format!("Error reading file: {}", path.display());
                }
            } else {
                self.status_message = "No file to revert from.".to_string();
            }
        }

        fn save_active_file(&mut self, arg: Option<&str>, quit_after_app: bool) -> bool {
            let path_from_arg = arg.map(PathBuf::from);

            let path_from_page = self
                .tabs
                .get(self.active_tab_index)
                .and_then(|p| p.file_path.clone());

            let path_to_write = path_from_arg.or(path_from_page);

            if let Some(path) = path_to_write {
                let content = self
                    .get_active_page()
                    .map(|p| p.get_all_lines().join("\n"))
                    .unwrap_or_default();

                match fs::write(&path, content) {
                    Ok(_) => {
                        self.status_message = format!("Saved to {}", path.display());
                        if let Some(page) = self.get_active_page() {
                            page.file_path = Some(path);
                        }
                        if quit_after_app {
                            self.should_quit = true;
                        }
                        true
                    }
                    Err(e) => {
                        self.status_message = format!("Error: {}", e);
                        false
                    }
                }
            } else {
                self.mode = if quit_after_app {
                    Mode::PromptSaveAndQuit
                } else {
                    Mode::PromptSave
                };
                self.command_buffer.clear();
                false
            }
        }

        pub fn get_active_page(&mut self) -> Option<&mut Page> {
            self.tabs.get_mut(self.active_tab_index)
        }
    }
}

/// All UI drawing and rendering logic.
pub mod ui {
    use super::*;
    use self::core::{ActivePane, App, Mode};

    const LOGO: &[&str] = &[
        "JJJJJJJ   OOOOO   TTTTTTT",
        "   J     O     O     T    ",
        "   J     O     O     T    ",
        "J  J     O     O     T    ",
        " JJJ      OOOOO      T    ",
    ];

    pub fn draw_ui(stdout: &mut io::Stdout, app: &App) -> io::Result<()> {
        let (width, height) = crossterm::terminal::size()?;
        queue!(stdout, Clear(ClearType::All))?;

        let view_height = height.saturating_sub(1);
        let file_tree_width = (width as f32 * 0.25).round() as u16;
        let editor_width = width.saturating_sub(file_tree_width);
        let divider_col = file_tree_width;

        draw_file_tree(stdout, app, file_tree_width, view_height)?;
        draw_divider(stdout, divider_col, view_height)?;
        draw_editor(
            stdout,
            app,
            divider_col + 1,
            editor_width.saturating_sub(1),
            view_height,
        )?;
        draw_status_bar(stdout, app, width, height)?;
        place_cursor(stdout, app, divider_col + 1, height)?;

        stdout.flush()
    }

    fn wrap_text(text: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![text.to_string()];
        }
        text.chars()
            .collect::<Vec<char>>()
            .chunks(width)
            .map(|chunk| chunk.iter().collect::<String>())
            .collect()
    }

    fn draw_file_tree(
        stdout: &mut io::Stdout,
        app: &App,
        width: u16,
        height: u16,
    ) -> io::Result<()> {
        let path_str = app.directory_view.path.to_string_lossy();
        let title_lines = wrap_text(&format!(" {}", path_str), width as usize);

        for (i, line) in title_lines.iter().enumerate() {
            queue!(stdout, MoveTo(0, i as u16))?;
            queue!(
                stdout,
                crossterm::style::Print(format!(
                    "\x1b[4m\x1b[1m{:width$}\x1b[0m",
                    line,
                    width = width as usize
                ))
            )?;
        }
        
        let title_height = title_lines.len();

        let view_height = height.saturating_sub(title_height as u16) as usize;
        let visible_entries = app
            .directory_view
            .entries
            .iter()
            .enumerate()
            .skip(app.directory_view.scroll_offset)
            .take(view_height);

        for (i, entry) in visible_entries {
            let screen_row = (i - app.directory_view.scroll_offset + title_height) as u16;
            queue!(stdout, MoveTo(0, screen_row))?;
            let mut name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() {
                name.push('/');
            }
            let line = format!(" {}", name);

            if i == app.directory_view.selected_index {
                let style = if app.active_pane == ActivePane::FileTree {
                    "\x1b[7m"
                } else {
                    "\x1b[2m"
                }; // Inverse or Dim
                queue!(
                    stdout,
                    crossterm::style::Print(format!(
                        "{}{:width$}\x1b[0m",
                        style,
                        line.chars().take(width as usize).collect::<String>(),
                        width = width as usize
                    ))
                )?;
            } else {
                queue!(
                    stdout,
                    crossterm::style::Print(format!(
                        "{:width$}",
                        line.chars().take(width as usize).collect::<String>(),
                        width = width as usize
                    ))
                )?;
            }
        }
        Ok(())
    }

    fn draw_divider(stdout: &mut io::Stdout, col: u16, height: u16) -> io::Result<()> {
        for row in 0..height {
            queue!(stdout, MoveTo(col, row))?;
            queue!(stdout, crossterm::style::Print("│"))?;
        }
        Ok(())
    }

    fn draw_editor(
        stdout: &mut io::Stdout,
        app: &App,
        start_col: u16,
        width: u16,
        height: u16,
    ) -> io::Result<()> {
        queue!(stdout, DisableLineWrap)?;
        if app.tabs.is_empty() {
            let top_padding = height.saturating_sub(LOGO.len() as u16) / 2;
            let max_logo_width = LOGO.iter().map(|s| s.len()).max().unwrap_or(0) as u16;
            let left_padding = width.saturating_sub(max_logo_width) / 2;

            for (i, line) in LOGO.iter().enumerate() {
                queue!(
                    stdout,
                    MoveTo(start_col + left_padding, top_padding + i as u16)
                )?;
                queue!(stdout, crossterm::style::Print(line))?;
            }
        } else {
            // Draw tab bar at the top of the editor pane
            queue!(stdout, MoveTo(start_col, 0))?;
            for (i, page) in app.tabs.iter().enumerate() {
                let file_name = page
                    .file_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|f| f.to_str())
                    .unwrap_or("[No Name]");
                let tab_text = format!(" {} ", file_name);
                if i == app.active_tab_index {
                    queue!(
                        stdout,
                        crossterm::style::Print(format!("\x1b[7m{}\x1b[0m", tab_text))
                    )?;
                } else {
                    queue!(
                        stdout,
                        crossterm::style::Print(format!("\x1b[2m{}\x1b[0m", tab_text))
                    )?;
                }
            }

            // Draw active page content below the tab bar
            if let Some(page) = app.tabs.get(app.active_tab_index) {
                let view_height = height.saturating_sub(1) as usize;
                let line_gutter_width = page.get_all_lines().len().to_string().len() + 1;

                let visible_lines = page
                    .get_all_lines()
                    .into_iter()
                    .enumerate()
                    .skip(page.scroll_offset)
                    .take(view_height);
                
                let matches_on_screen: Vec<_> = app.find_matches.iter().filter(|(r, _)| *r >= page.scroll_offset && *r < page.scroll_offset + view_height).collect();

                for (i, line) in visible_lines {
                    let screen_row = (i - page.scroll_offset) as u16 + 1;
                    queue!(stdout, MoveTo(start_col, screen_row))?;
                    let line_num_str = format!("{:>width$}", i + 1, width = line_gutter_width);
                    
                    let h_scroll_offset = page.horizontal_scroll_offset;

                    queue!(
                        stdout,
                        crossterm::style::Print(format!("\x1b[34m{} \x1b[0m", line_num_str))
                    )?;

                    if app.mode == Mode::Find && !app.find_query.is_empty() {
                        let line_matches: Vec<_> = matches_on_screen.iter().filter(|(r, _)| *r == i).collect();
                        let mut last_end = 0;
                        let mut highlighted_line = String::new();

                        for (_, col) in line_matches {
                            if *col >= last_end {
                                if let Some(substring) = line.get(last_end..*col) {
                                    highlighted_line.push_str(substring);
                                }
                                highlighted_line.push_str("\x1b[7m");
                                if let Some(match_str) = line.get(*col..*col + app.find_query.len()) {
                                    highlighted_line.push_str(match_str);
                                }
                                highlighted_line.push_str("\x1b[0m");
                                last_end = *col + app.find_query.len();
                            }
                        }
                        if let Some(remaining) = line.get(last_end..) {
                            highlighted_line.push_str(remaining);
                        }
                        
                        let visible_highlighted: String = highlighted_line.chars().skip(h_scroll_offset).collect();
                        queue!(stdout, crossterm::style::Print(visible_highlighted))?;

                    } else {
                        let visible_line: String = line.chars().skip(h_scroll_offset).collect();
                         queue!(stdout, crossterm::style::Print(visible_line))?;
                    }
                }
            }
        }
        queue!(stdout, EnableLineWrap)?;
        Ok(())
    }

    fn draw_status_bar(
        stdout: &mut io::Stdout,
        app: &App,
        width: u16,
        height: u16,
    ) -> io::Result<()> {
        queue!(stdout, MoveTo(0, height.saturating_sub(1)))?;

        let status_text = if !app.status_message.is_empty() {
            app.status_message.clone()
        } else {
            match app.mode {
                Mode::PromptSave | Mode::PromptSaveAndQuit | Mode::PromptNewFile | Mode::PromptNewDirectory | Mode::PromptRename => {
                    let mode_str = match app.mode {
                        Mode::PromptSave => "Save As:",
                        Mode::PromptSaveAndQuit => "Save As & Quit:",
                        Mode::PromptNewFile => "New File Name:",
                        Mode::PromptNewDirectory => "New Directory Name:",
                        Mode::PromptRename => "Rename to:",
                        _ => "", // Unreachable
                    };
                    format!("{} {}", mode_str, app.command_buffer)
                },
                Mode::ConfirmDelete => {
                    let file_name = app.path_to_delete.as_ref().and_then(|p| p.file_name()).and_then(|f| f.to_str()).unwrap_or_default();
                    format!("Delete {}? (y/n)", file_name)
                },
                Mode::Command => {
                    format!("-- COMMAND -- :{}", app.command_buffer)
                },
                Mode::FileTree => {
                    if !app.command_buffer.is_empty() {
                        format!("-- DIR COMMAND -- :{}", app.command_buffer)
                    } else {
                        "-- FILE TREE --".to_string()
                    }
                },
                Mode::Find => {
                    if app.find_navigation_active {
                        format!("Find (Nav): {} (n/N)", app.find_query)
                    } else {
                        format!("Find: {}", app.find_query)
                    }
                },
                Mode::Edit => {
                    let file_info = app
                        .tabs
                        .get(app.active_tab_index)
                        .map(|p| {
                            p.file_path
                                .as_ref()
                                .map(|path| path.to_string_lossy().to_string())
                                .unwrap_or_else(|| "[No Name]".to_string())
                        })
                        .unwrap_or_else(|| "".to_string());
                    format!("-- INSERT -- {}", file_info)
                }
            }
        };

        queue!(
            stdout,
            crossterm::style::Print(format!(
                "\x1b[7m{:width$}\x1b[0m",
                status_text.chars().take(width as usize).collect::<String>(),
                width = width as usize
            ))
        )?;
        Ok(())
    }

    fn place_cursor(
        stdout: &mut io::Stdout,
        app: &App,
        editor_start_col: u16,
        term_height: u16,
    ) -> io::Result<()> {
        if app.active_pane == ActivePane::Editor && app.mode == Mode::Edit {
            if let Some(page) = app.tabs.get(app.active_tab_index) {
                let cursor_row = page.cursor_row();
                let scroll_offset = page.scroll_offset;
                let view_height = term_height.saturating_sub(2) as usize; // for tabs and status bar

                // Only place cursor if it's within the visible part of the editor view
                if cursor_row >= scroll_offset && cursor_row < scroll_offset + view_height {
                    let line_gutter_width = page.get_all_lines().len().to_string().len() + 2;
                    let cursor_col_in_string = page.current.cursor_position();
                    let h_scroll_offset = page.horizontal_scroll_offset;
                    
                    let screen_cursor_col = editor_start_col
                        + (cursor_col_in_string - h_scroll_offset) as u16
                        + line_gutter_width as u16;

                    // Calculate screen row relative to scroll offset
                    let screen_row = (cursor_row - scroll_offset) as u16 + 1; // +1 for tab bar
                    queue!(stdout, MoveTo(screen_cursor_col, screen_row))?;
                }
            }
        }
        // In FileTree and Command panes, the "cursor" is not shown.
        Ok(())
    }
}

