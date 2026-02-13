//! # Jupyter IDE Screen
//!
//! One-click managed JupyterLab workflow for Advanced mode.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui;
use neurohid_types::config::UiConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BootstrapState {
    Idle,
    Running,
    Ready,
    Failed,
}

pub struct JupyterIdeScreen {
    bootstrap_state: BootstrapState,
    bootstrap_started_once: bool,
    log_output: String,
    tool_running: bool,
    tool_rx: Option<Receiver<String>>,
    jupyter_process: Option<Child>,
    jupyter_events_rx: Option<Receiver<String>>,
    jupyter_ready: bool,
    jupyter_session_url: Option<String>,
}

impl JupyterIdeScreen {
    pub fn new() -> Self {
        Self {
            bootstrap_state: BootstrapState::Idle,
            bootstrap_started_once: false,
            log_output: String::new(),
            tool_running: false,
            tool_rx: None,
            jupyter_process: None,
            jupyter_events_rx: None,
            jupyter_ready: false,
            jupyter_session_url: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, ui_cfg: &UiConfig) {
        self.poll_tool_output();
        self.poll_jupyter_events();
        self.poll_jupyter_exit();

        if ui_cfg.jupyter_auto_bootstrap
            && !self.bootstrap_started_once
            && self.bootstrap_state == BootstrapState::Idle
        {
            self.bootstrap_started_once = true;
            self.start_bootstrap(&ui_cfg.jupyter_bootstrap_command);
        }

        ui.heading("Jupyter IDE");
        ui.label(
            egui::RichText::new("Managed JupyterLab for RL/ML experimentation")
                .small()
                .weak(),
        );
        ui.add_space(8.0);

        let bootstrap_text = match self.bootstrap_state {
            BootstrapState::Idle => "idle",
            BootstrapState::Running => "preparing",
            BootstrapState::Ready => "ready",
            BootstrapState::Failed => "failed",
        };
        let bootstrap_color = match self.bootstrap_state {
            BootstrapState::Ready => egui::Color32::GREEN,
            BootstrapState::Running => egui::Color32::YELLOW,
            BootstrapState::Failed => egui::Color32::RED,
            BootstrapState::Idle => egui::Color32::GRAY,
        };

        let jupyter_running = self
            .jupyter_process
            .as_mut()
            .is_some_and(|child| child.try_wait().ok().flatten().is_none());

        ui.horizontal(|ui| {
            ui.colored_label(bootstrap_color, "●");
            ui.label(format!("Environment: {bootstrap_text}"));

            ui.separator();

            let jupyter_color = if jupyter_running && self.jupyter_ready {
                egui::Color32::GREEN
            } else if jupyter_running {
                egui::Color32::YELLOW
            } else {
                egui::Color32::GRAY
            };
            ui.colored_label(jupyter_color, "●");
            ui.label(format!(
                "Jupyter: {}",
                if jupyter_running && self.jupyter_ready {
                    "ready"
                } else if jupyter_running {
                    "starting"
                } else {
                    "stopped"
                }
            ));
        });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.tool_running, egui::Button::new("Prepare Environment"))
                .clicked()
            {
                self.start_bootstrap(&ui_cfg.jupyter_bootstrap_command);
            }

            let can_start_jupyter = !jupyter_running
                && !self.tool_running
                && matches!(
                    self.bootstrap_state,
                    BootstrapState::Ready | BootstrapState::Idle
                );
            if ui
                .add_enabled(can_start_jupyter, egui::Button::new("Start Jupyter"))
                .clicked()
            {
                self.start_jupyter(&ui_cfg.jupyter_command);
            }

            if ui
                .add_enabled(jupyter_running, egui::Button::new("Stop Jupyter"))
                .clicked()
            {
                self.stop_jupyter();
            }

            if ui
                .add_enabled(jupyter_running, egui::Button::new("Open in Browser"))
                .clicked()
            {
                let browser_url = self
                    .jupyter_session_url
                    .as_deref()
                    .unwrap_or(&ui_cfg.jupyter_url);
                if let Err(error) = open_url(browser_url) {
                    self.log_output
                        .push_str(&format!("Failed to open browser: {}\n", error));
                }
            }

            if ui.button("Clear Log").clicked() {
                self.log_output.clear();
            }
        });

        ui.label(
            egui::RichText::new(format!("Bootstrap cmd: {}", ui_cfg.jupyter_bootstrap_command))
                .small()
                .weak(),
        );
        ui.label(
            egui::RichText::new(format!("Jupyter cmd: {}", ui_cfg.jupyter_command))
                .small()
                .weak(),
        );
        ui.label(
            egui::RichText::new(format!("Jupyter url: {}", ui_cfg.jupyter_url))
                .small()
                .weak(),
        );

        ui.separator();
        ui.label(egui::RichText::new("IDE Log").strong());
        egui::ScrollArea::vertical()
            .id_salt("jupyter_ide_log_scroll")
            .max_height(260.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.log_output)
                        .id_salt("jupyter_ide_log_output")
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(12)
                        .desired_width(f32::INFINITY),
                );
            });
    }

    fn start_bootstrap(&mut self, command_line: &str) {
        if self.tool_running {
            return;
        }
        self.bootstrap_state = BootstrapState::Running;
        self.tool_running = true;
        self.log_output
            .push_str(&format!("$ {}\n", command_line.trim()));

        let command_line = command_line.to_string();
        let (tx, rx) = mpsc::channel();
        self.tool_rx = Some(rx);

        std::thread::spawn(move || {
            let output = run_command_capture_output(&command_line);
            let _ = tx.send(output);
        });
    }

    fn start_jupyter(&mut self, command_line: &str) {
        if self.jupyter_process.is_some() {
            return;
        }

        if command_line.trim().is_empty() {
            self.log_output
                .push_str("Failed to start Jupyter: command is empty\n");
            return;
        }

        let mut command = build_shell_command(command_line);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        match command.spawn() {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();

                let (events_tx, events_rx) = mpsc::channel();
                if let Some(stdout) = stdout {
                    let tx = events_tx.clone();
                    std::thread::spawn(move || read_stream(stdout, tx));
                }
                if let Some(stderr) = stderr {
                    let tx = events_tx.clone();
                    std::thread::spawn(move || read_stream(stderr, tx));
                }

                self.log_output
                    .push_str("Jupyter process started, waiting for readiness...\n");
                self.jupyter_process = Some(child);
                self.jupyter_events_rx = Some(events_rx);
                self.jupyter_ready = false;
                self.jupyter_session_url = None;
            }
            Err(error) => {
                self.log_output
                    .push_str(&format!("Failed to start Jupyter: {}\n", error));
            }
        }
    }

    fn stop_jupyter(&mut self) {
        let Some(mut child) = self.jupyter_process.take() else {
            return;
        };

        let _ = child.kill();
        let _ = child.wait();
        self.jupyter_events_rx = None;
        self.jupyter_ready = false;
        self.jupyter_session_url = None;
        self.log_output.push_str("Jupyter stopped.\n");
    }

    fn poll_tool_output(&mut self) {
        let Some(rx) = &self.tool_rx else {
            return;
        };

        match rx.try_recv() {
            Ok(text) => {
                self.log_output.push_str(&text);
                self.tool_running = false;
                self.tool_rx = None;
                if text.contains("[exit=0]") {
                    self.bootstrap_state = BootstrapState::Ready;
                } else {
                    self.bootstrap_state = BootstrapState::Failed;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.log_output
                    .push_str("Bootstrap worker disconnected unexpectedly.\n");
                self.tool_running = false;
                self.tool_rx = None;
                self.bootstrap_state = BootstrapState::Failed;
            }
        }
    }

    fn poll_jupyter_events(&mut self) {
        let Some(rx) = &self.jupyter_events_rx else {
            return;
        };

        loop {
            match rx.try_recv() {
                Ok(line) => {
                    if let Some(url) = extract_jupyter_url(&line) {
                        self.jupyter_session_url = Some(url);
                    }
                    if !self.jupyter_ready && looks_like_jupyter_ready_line(&line) {
                        self.jupyter_ready = true;
                        self.log_output.push_str("Jupyter ready.\n");
                    }
                    self.log_output.push_str(&line);
                    self.log_output.push('\n');
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    fn poll_jupyter_exit(&mut self) {
        let Some(child) = &mut self.jupyter_process else {
            return;
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                self.log_output
                    .push_str(&format!("Jupyter exited with status {}\n", status));
                self.jupyter_process = None;
                self.jupyter_events_rx = None;
                self.jupyter_ready = false;
                self.jupyter_session_url = None;
            }
            Ok(None) => {}
            Err(error) => {
                self.log_output
                    .push_str(&format!("Jupyter status check failed: {}\n", error));
                self.jupyter_process = None;
                self.jupyter_events_rx = None;
                self.jupyter_ready = false;
                self.jupyter_session_url = None;
            }
        }
    }
}

impl Drop for JupyterIdeScreen {
    fn drop(&mut self) {
        self.stop_jupyter();
    }
}

fn run_command_capture_output(command_line: &str) -> String {
    if command_line.trim().is_empty() {
        return "Failed: empty command\n[exit=1]\n".to_string();
    }

    let mut command = build_shell_command(command_line);

    match command.output() {
        Ok(output) => {
            let mut text = String::new();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stdout.is_empty() {
                text.push_str(&stdout);
                if !stdout.ends_with('\n') {
                    text.push('\n');
                }
            }
            if !stderr.is_empty() {
                text.push_str("[stderr]\n");
                text.push_str(&stderr);
                if !stderr.ends_with('\n') {
                    text.push('\n');
                }
            }
            if text.is_empty() {
                text.push_str("(no output)\n");
            }
            text.push_str(&format!("[exit={}]\n", output.status.code().unwrap_or(-1)));
            text
        }
        Err(error) => format!("Failed: {}\n[exit=1]\n", error),
    }
}

fn read_stream<T: std::io::Read>(stream: T, tx: mpsc::Sender<String>) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(line) if !line.trim().is_empty() => {
                let _ = tx.send(line);
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

fn looks_like_jupyter_ready_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("jupyter server")
        || lower.contains("is running at")
}

fn extract_jupyter_url(line: &str) -> Option<String> {
    line.split_whitespace().find_map(|part| {
        if part.starts_with("http://") || part.starts_with("https://") {
            Some(
                part.trim_end_matches([')', ']', '}', ',', ';', '\'', '"'])
                    .to_string(),
            )
        } else {
            None
        }
    })
}

fn build_shell_command(command_line: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.args(["/D", "/S", "/C", command_line]);
        command
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut command = Command::new("sh");
        command.args(["-lc", command_line]);
        command
    }
}

fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "cmd start exited with status {}",
                status
            )))
        }
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open").arg(url).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "open exited with status {}",
                status
            )))
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let status = Command::new("xdg-open").arg(url).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "xdg-open exited with status {}",
                status
            )))
        }
    }
}
