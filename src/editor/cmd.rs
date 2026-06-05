use crate::editor::Editor;
use crate::types::{ConfirmAction, Mode, SearchDirection};
use crossterm::event::KeyCode;

impl Editor {
    pub(crate) async fn handle_command(&mut self, key: KeyCode) {
        if self.engine.state.has_confirmation() {
            self.handle_confirmation(key).await;
            return;
        }

        match key {
            KeyCode::Enter => {
                let cmd = self.engine.state.command_buffer.trim().to_string();
                let cmd = cmd.strip_prefix(':').unwrap_or(&cmd);
                self.engine.state.command_buffer.clear();

                match cmd {
                    "q" => {
                        self.handle_quit();
                    }
                    "q!" => {
                        self.running = false;
                    }
                    "w" | "w!" => {
                        self.save_and_emit().await;
                    }
                    "wq" | "wq!" | "wqa" | "wa" => {
                        self.save_and_emit().await;
                        self.running = false;
                    }
                    "qa" => {
                        self.running = false;
                    }
                    "e" => {
                        self.reload_file().await;
                    }
                    "e!" => {
                        self.reload_file_discard().await;
                    }
                    cmd if cmd.starts_with("keymap ") => {
                        let keymap_name = cmd.trim_start_matches("keymap ").trim();
                        self.switch_keymap(keymap_name);
                    }
                    _ => {
                        if let Some(query) = cmd.strip_prefix('/') {
                            let direction = SearchDirection::Forward;
                            self.do_search(query, direction);
                        } else if let Some(query) = cmd.strip_prefix('?') {
                            let direction = SearchDirection::Backward;
                            self.do_search(query, direction);
                        } else if cmd.starts_with("set ") {
                            self.handle_set_command(cmd);
                        } else if cmd.starts_with("w ") || cmd.starts_with("w! ") {
                            self.handle_write_path(cmd).await;
                        } else if cmd.starts_with("wq ") || cmd.starts_with("wq! ") {
                            self.handle_write_quit_path(cmd).await;
                        } else {
                            // Split cmd into command name and args
                            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
                            let cmd_name = parts[0];
                            let args: Vec<String> = if parts.len() > 1 {
                                parts[1].split(' ').map(|s| s.to_string()).collect()
                            } else {
                                vec![]
                            };
                            if !self.engine.plugin_manager.execute_command(cmd_name, args) {
                                self.engine
                                    .state
                                    .set_message(format!("Unknown command: {}", cmd));
                            }
                        }
                    }
                }

                if !self.engine.state.has_confirmation() {
                    self.transition_mode(Mode::Normal);
                    self.needs_render = true;
                }
            }
            KeyCode::Esc => {
                self.engine.state.command_buffer.clear();
                self.transition_mode(Mode::Normal);
                self.needs_render = true;
            }
            KeyCode::Backspace => {
                self.engine.state.command_buffer.pop();
                self.needs_render = true;
            }
            KeyCode::Char(c) => {
                self.engine.state.command_buffer.push(c);
                self.needs_render = true;
            }
            _ => {}
        }
    }

    pub fn handle_quit(&mut self) {
        if self.engine.buffer.is_dirty() {
            self.engine.state.set_confirmation(
                "No write since last change. Quit anyway? (y/n Enter/Esc: yes, n: no)".to_string(),
                ConfirmAction::Quit,
            );
            self.needs_render = true;
        } else {
            self.running = false;
            self.engine.state.mode = Mode::Normal;
        }
    }

    pub(crate) async fn handle_confirmation(&mut self, key: KeyCode) {
        let should_quit = match key {
            KeyCode::Char('y') | KeyCode::Enter => true,
            KeyCode::Char('n') | KeyCode::Esc => false,
            _ => {
                self.engine.state.clear_confirmation();
                self.needs_render = true;
                return;
            }
        };

        let action = self
            .engine
            .state
            .confirmation_prompt
            .as_ref()
            .unwrap()
            .action
            .clone();
        self.engine.state.clear_confirmation();
        self.needs_render = true;

        match action {
            ConfirmAction::Quit => {
                if should_quit {
                    self.running = false;
                    self.engine.state.mode = Mode::Normal;
                }
            }
            ConfirmAction::QuitDiscard => {
                self.running = false;
                self.engine.state.mode = Mode::Normal;
            }
            ConfirmAction::WriteQuitAll => {
                if !should_quit {
                    return;
                }
                if let Err(e) = self.engine.buffer.save_file().await {
                    self.engine.state.set_message(format!("Save failed: {}", e));
                } else {
                    self.engine
                        .plugin_manager
                        .emit(crate::types::PluginEvent::BufferSave {
                            file_path: self.engine.state.file_path.clone(),
                        });
                    self.running = false;
                    self.engine.state.mode = Mode::Normal;
                }
            }
        }
    }

    fn handle_set_command(&mut self, cmd: &str) {
        let args = cmd.trim_start_matches("set ").trim();

        match args {
            "number" => {
                self.engine.state.show_line_numbers = true;
            }
            "nonumber" | "nonumber!" => {
                self.engine.state.show_line_numbers = false;
            }
            "number!" => {
                self.engine.state.show_line_numbers = !self.engine.state.show_line_numbers;
            }
            _ => {
                self.engine
                    .state
                    .set_message(format!("Unknown set option: {}", args));
            }
        }
    }

    async fn reload_file(&mut self) {
        if let Some(path) = &self.engine.state.file_path {
            match crate::buffer::TextBuffer::load_file(path.to_str().unwrap_or("")).await {
                Ok(buf) => {
                    self.engine.buffer = buf;
                }
                Err(e) => {
                    self.engine
                        .state
                        .set_message(format!("Failed to reload file: {}", e));
                }
            }
        }
    }

    async fn reload_file_discard(&mut self) {
        self.reload_file().await;
    }

    /// Save the current buffer and emit a BufferSave plugin event.
    /// If the save fails, set an error message instead.
    async fn save_and_emit(&mut self) {
        match self.engine.buffer.save_file().await {
            Ok(()) => {
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::BufferSave {
                        file_path: self.engine.state.file_path.clone(),
                    });
            }
            Err(e) => {
                self.engine.state.set_message(format!("Save failed: {}", e));
            }
        }
    }

    async fn handle_write_path(&mut self, cmd: &str) {
        let path_str = cmd.trim_start_matches("w!").trim_start_matches("w ").trim();
        if path_str.is_empty() {
            self.engine.state.set_message("Expected filename after 'w'");
            return;
        }
        let path = std::path::PathBuf::from(path_str);
        match self.engine.buffer.save_to_path(&path).await {
            Ok(_) => {
                self.engine.state.file_path = Some(path);
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::BufferSave {
                        file_path: self.engine.state.file_path.clone(),
                    });
            }
            Err(e) => {
                self.engine.state.set_message(format!("Save failed: {}", e));
            }
        }
    }

    async fn handle_write_quit_path(&mut self, cmd: &str) {
        let path_str = cmd
            .trim_start_matches("wq!")
            .trim_start_matches("wq ")
            .trim();
        if path_str.is_empty() {
            self.engine
                .state
                .set_message("Expected filename after 'wq'");
            return;
        }
        let path = std::path::PathBuf::from(path_str);
        match self.engine.buffer.save_to_path(&path).await {
            Ok(_) => {
                self.engine.state.file_path = Some(path);
                self.engine
                    .plugin_manager
                    .emit(crate::types::PluginEvent::BufferSave {
                        file_path: self.engine.state.file_path.clone(),
                    });
                self.running = false;
            }
            Err(e) => {
                self.engine.state.set_message(format!("Save failed: {}", e));
            }
        }
    }
}
