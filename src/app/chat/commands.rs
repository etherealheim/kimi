use crate::app::AgentEvent;
use crate::app::App;
use color_eyre::Result;

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

        let url = content.trim_start_matches("download").trim().to_string();
        self.chat_input.clear();
        self.reset_chat_scroll();

        if url.is_empty() {
            self.add_system_message("Usage: download <url>");
            return Ok(true);
        }

        let tx = self.agent_tx.clone();
        self.download_active = true;
        self.download_frame = 0;
        self.last_download_tick = None;
        self.download_progress = None;

        std::thread::spawn(move || {
            let result = crate::services::link_download::download_video_with_progress(
                &url,
                |progress| {
                    if let Some(tx) = &tx {
                        let _ = tx.send(AgentEvent::DownloadProgress(progress));
                    }
                },
            );
            if let Some(tx) = tx {
                if let Err(error) = result {
                    let _ = tx.send(AgentEvent::SystemMessage(format!(
                        "Download failed: {}",
                        error
                    )));
                }
                let _ = tx.send(AgentEvent::DownloadFinished);
            }
        });

        Ok(true)
    }
}
