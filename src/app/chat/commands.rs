use crate::app::AgentEvent;
use crate::app::App;
use color_eyre::Result;
use std::process::{Command, Stdio};

impl App {
    pub(crate) fn handle_convert_command(&mut self) -> Result<bool> {
        let content = self.chat_input.content().trim().to_string();
        if !(content == "convert" || content.starts_with("convert ")) {
            return Ok(false);
        }

        let mut parts = content.splitn(3, ' ');
        let _ = parts.next();
        let format = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").trim();

        self.chat_input.clear();
        self.reset_chat_scroll();

        if format.is_empty() || path.is_empty() {
            self.add_system_message("Usage: convert <format> <path>");
            return Ok(true);
        }

        let tx = self.agent_tx.clone();
        self.conversion_active = true;
        self.conversion_frame = 0;
        self.last_conversion_tick = None;

        let input_path = path.to_string();
        let format_copy = format.clone();
        std::thread::spawn(move || {
            let result = crate::services::convert::convert_file(&input_path, &format_copy);
            if let Some(tx) = tx {
                if let Err(error) = result {
                    let _ = tx.send(AgentEvent::SystemMessage(format!(
                        "Conversion failed: {}",
                        error
                    )));
                }
                let _ = tx.send(AgentEvent::ConversionFinished);
            }
        });

        Ok(true)
    }

    pub(crate) fn handle_download_command(&mut self) -> Result<bool> {
        let content = self.chat_input.content().trim().to_string();
        if !(content == "download" || content.starts_with("download ")) {
            return Ok(false);
        }

        let urls_str = content.trim_start_matches("download").trim();
        self.chat_input.clear();
        self.reset_chat_scroll();

        if urls_str.is_empty() {
            self.add_system_message("Usage: download <url> [url2] [url3] ...");
            return Ok(true);
        }

        // Parse multiple URLs separated by spaces
        let urls: Vec<String> = urls_str
            .split_whitespace()
            .map(str::to_string)
            .collect();

        if urls.is_empty() {
            self.add_system_message("Usage: download <url> [url2] [url3] ...");
            return Ok(true);
        }

        // Start downloads for all URLs
        for url in urls {
            // Add download item to active list
            self.active_downloads.push(crate::app::types::DownloadItem {
                url: url.clone(),
                progress: None,
                frame: 0,
                last_tick: None,
            });

            // Spawn download thread
            let tx = self.agent_tx.clone();
            let url_clone = url.clone();
            std::thread::spawn(move || {
                let result = crate::services::link_download::download_video_with_progress(
                    &url,
                    |progress| {
                        if let Some(tx) = &tx {
                            let _ = tx.send(AgentEvent::DownloadProgress {
                                url: url.clone(),
                                progress,
                            });
                        }
                    },
                );
                if let Some(tx) = tx {
                    if let Err(error) = result {
                        let _ = tx.send(AgentEvent::SystemMessage(format!(
                            "Download failed for {}: {}",
                            url_clone,
                            error
                        )));
                    }
                    let _ = tx.send(AgentEvent::DownloadFinished { url: url_clone });
                }
            });
        }

        Ok(true)
    }

    pub(crate) fn handle_comfyui_command(&mut self) -> Result<bool> {
        let content = self.chat_input.content().trim().to_string();
        if !(content == "comfyui" || content.starts_with("comfyui ")) {
            return Ok(false);
        }

        let mut parts = content.splitn(2, ' ');
        let _ = parts.next(); // Skip "comfyui"
        let subcommand = parts.next().unwrap_or("").trim();

        self.chat_input.clear();
        self.reset_chat_scroll();

        match subcommand {
            "start" => {
                // Check if ComfyUI is already running
                if self.comfyui_process.is_some() {
                    self.add_system_message("ComfyUI is already running");
                    return Ok(true);
                }

                // Get home directory
                let home = std::env::var("HOME").unwrap_or_else(|_| "/home/ethereal".to_string());
                let comfyui_path = format!("{}/git-local/comfy-ui", home);

                // Start ComfyUI using run.sh
                match Command::new("bash")
                    .arg("run.sh")
                    .current_dir(&comfyui_path)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                {
                    Ok(child) => {
                        self.comfyui_process = Some(child);
                        self.add_system_message("ComfyUI started. Server is active on port 8188");
                    }
                    Err(error) => {
                        self.add_system_message(&format!("Failed to start ComfyUI: {}", error));
                    }
                }
            }
            "stop" => {
                if let Some(mut process) = self.comfyui_process.take() {
                    match process.kill() {
                        Ok(_) => {
                            self.add_system_message("ComfyUI stopped");
                        }
                        Err(error) => {
                            self.add_system_message(&format!("Failed to stop ComfyUI: {}", error));
                        }
                    }
                } else {
                    self.add_system_message("ComfyUI is not running");
                }
            }
            _ => {
                self.add_system_message("Usage: comfyui <start|stop>");
            }
        }

        Ok(true)
    }
}
