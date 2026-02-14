//! # Python Lab Screen
//!
//! Notebook-style experimentation surface that talks to a decoupled kernel
//! adapter over a JSON-lines stdio protocol.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui;
use egui_async::Bind;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use serde::{Deserialize, Serialize};

use crate::data_bus::DataBus;
use crate::state::ServiceSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CellStatus {
    Idle,
    Queued,
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone)]
struct NotebookCell {
    code: String,
    output: String,
    status: CellStatus,
    exec_count: Option<u64>,
    duration_ms: Option<u64>,
}

impl NotebookCell {
    fn new(code: String) -> Self {
        Self {
            code,
            output: String::new(),
            status: CellStatus::Idle,
            exec_count: None,
            duration_ms: None,
        }
    }
}

#[derive(Debug)]
struct PendingExecution {
    request_id: u64,
    cell_index: usize,
}

#[derive(Debug)]
struct KernelSession {
    child: Child,
    stdin: ChildStdin,
    events_rx: Receiver<KernelEvent>,
    next_request_id: u64,
    ready: bool,
    protocol: Option<String>,
}

#[derive(Debug)]
enum KernelEvent {
    Message(KernelResponse),
    Stderr(String),
    ParseError(String),
    Exit,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum KernelRequest {
    Execute { request_id: u64, code: String },
    Reset { request_id: u64 },
    Shutdown,
    Ping { request_id: u64 },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum KernelResponse {
    Ready {
        protocol: Option<String>,
    },
    ExecuteResult {
        request_id: u64,
        status: String,
        stdout: String,
        stderr: String,
        result: Option<String>,
        error: Option<KernelExecError>,
        exec_count: u64,
        duration_ms: u64,
    },
    ResetResult {
        request_id: u64,
    },
    Pong {
        request_id: u64,
    },
    Error {
        request_id: Option<u64>,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
struct KernelExecError {
    name: String,
    message: String,
    traceback: String,
}

pub struct PythonLabScreen {
    cells: Vec<NotebookCell>,
    selected_cell: usize,
    kernel: Option<KernelSession>,
    pending_execution: Option<PendingExecution>,
    queued_cells: Vec<usize>,
    log_output: String,
    uv_sync_task: Bind<String, String>,
    monitor_rows: usize,
    monitor_preview_values: usize,
}

impl Default for PythonLabScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonLabScreen {
    pub fn new() -> Self {
        Self {
            cells: vec![NotebookCell::new(
                "from neurohid_ml import __version__\nprint('neurohid_ml', __version__)"
                    .to_string(),
            )],
            selected_cell: 0,
            kernel: None,
            pending_execution: None,
            queued_cells: vec![],
            log_output: String::new(),
            uv_sync_task: Bind::new(true),
            monitor_rows: 10,
            monitor_preview_values: 12,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        kernel_command: &str,
        data_bus: &DataBus,
        service_snapshot: &ServiceSnapshot,
    ) {
        self.poll_kernel_events(kernel_command);
        self.poll_tool_output();

        ui.heading("Python Lab");
        ui.label(
            egui::RichText::new("Notebook-style execution over decoupled kernel IPC")
                .small()
                .weak(),
        );
        ui.label(
            egui::RichText::new(
                "Kernel protocol: JSON lines over stdio (execute/reset/ping/shutdown)",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);
        self.show_bridge_monitor(ui, data_bus, service_snapshot);

        ui.horizontal(|ui| {
            if ui.button("Add Cell").clicked() {
                self.cells.push(NotebookCell::new(String::new()));
                self.selected_cell = self.cells.len().saturating_sub(1);
            }

            let run_selected = self.selected_cell < self.cells.len();
            if ui
                .add_enabled(run_selected, egui::Button::new("Run Selected"))
                .clicked()
            {
                self.enqueue_cell(self.selected_cell, kernel_command);
            }

            if ui.button("Run All").clicked() {
                self.enqueue_all_cells(kernel_command);
            }

            if ui.button("Restart Kernel").clicked() {
                self.restart_kernel(kernel_command);
            }

            if ui.button("Stop Kernel").clicked() {
                self.stop_kernel();
            }

            let uv_sync_running = self.uv_sync_task.is_pending();
            if ui
                .add_enabled(!uv_sync_running, egui::Button::new("uv sync"))
                .clicked()
            {
                self.run_uv_sync();
            }

            if ui.button("Clear Log").clicked() {
                self.log_output.clear();
            }
        });

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!(
                "Kernel cmd: {}",
                if kernel_command.trim().is_empty() {
                    "<empty>"
                } else {
                    kernel_command
                }
            ))
            .small()
            .weak(),
        );

        let kernel_status = self.kernel_status();
        let (status_color, status_text) = match kernel_status.as_str() {
            "running" => (egui::Color32::GREEN, "Kernel: running"),
            "starting" => (egui::Color32::YELLOW, "Kernel: starting"),
            _ => (egui::Color32::GRAY, "Kernel: stopped"),
        };

        ui.horizontal(|ui| {
            ui.colored_label(status_color, "●");
            ui.label(status_text);
            if !self.queued_cells.is_empty() {
                ui.label(
                    egui::RichText::new(format!("Queued cells: {}", self.queued_cells.len()))
                        .small()
                        .color(egui::Color32::YELLOW),
                );
            }
        });

        ui.separator();

        let mut run_clicked: Option<usize> = None;
        let mut delete_clicked: Option<usize> = None;

        egui::ScrollArea::vertical()
            .id_salt("python_lab_cells_scroll")
            .show(ui, |ui| {
                for index in 0..self.cells.len() {
                    let cell = &mut self.cells[index];
                    ui.push_id(("python_cell", index), |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let selected = self.selected_cell == index;
                                if ui
                                    .selectable_label(selected, format!("Cell {}", index + 1))
                                    .clicked()
                                {
                                    self.selected_cell = index;
                                }

                                let run_button = ui.add_enabled(
                                    !matches!(cell.status, CellStatus::Running),
                                    egui::Button::new("Run"),
                                );
                                if run_button.clicked() {
                                    run_clicked = Some(index);
                                }

                                let delete_button = ui.add_enabled(
                                    !matches!(cell.status, CellStatus::Running),
                                    egui::Button::new("Delete"),
                                );
                                if delete_button.clicked() {
                                    delete_clicked = Some(index);
                                }

                                let (color, text) = status_badge(&cell.status);
                                ui.colored_label(color, text);

                                if let Some(duration_ms) = cell.duration_ms {
                                    ui.label(
                                        egui::RichText::new(format!("{} ms", duration_ms))
                                            .small()
                                            .weak(),
                                    );
                                }
                                if let Some(exec_count) = cell.exec_count {
                                    ui.label(
                                        egui::RichText::new(format!("exec #{}", exec_count))
                                            .small()
                                            .weak(),
                                    );
                                }
                            });

                            CodeEditor::default()
                                .id_source(format!("python_cell_code_{}", index))
                                .with_rows(8)
                                .with_fontsize(14.0)
                                .with_theme(ColorTheme::GRUVBOX)
                                .with_syntax(Syntax::python())
                                .with_numlines(true)
                                .show(ui, &mut cell.code);

                            ui.label(egui::RichText::new("Output").small().strong());
                            let output_editor = egui::TextEdit::multiline(&mut cell.output)
                                .id_salt(("python_cell_output", index))
                                .font(egui::TextStyle::Monospace)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .interactive(false);
                            ui.add(output_editor);
                        });
                    });
                    ui.add_space(8.0);
                }
            });

        if let Some(index) = delete_clicked {
            self.delete_cell(index);
        }
        if let Some(index) = run_clicked {
            self.enqueue_cell(index, kernel_command);
        }

        ui.separator();
        ui.label(egui::RichText::new("Kernel / Tool Log").strong());
        egui::ScrollArea::vertical()
            .id_salt("python_lab_log_scroll")
            .max_height(180.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.log_output)
                        .id_salt("python_lab_log_output")
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            });
    }

    fn show_bridge_monitor(
        &mut self,
        ui: &mut egui::Ui,
        data_bus: &DataBus,
        service_snapshot: &ServiceSnapshot,
    ) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Bridge Monitor (Always On)").strong());

                let (bridge_color, bridge_text) = if service_snapshot.ml_bridge_connected {
                    if service_snapshot.ml_bridge_stalled {
                        (egui::Color32::YELLOW, "stalled")
                    } else {
                        (egui::Color32::GREEN, "connected")
                    }
                } else {
                    (egui::Color32::GRAY, "disconnected")
                };
                ui.colored_label(bridge_color, format!("ML bridge: {}", bridge_text));

                ui.label(
                    egui::RichText::new(format!(
                        "runtime mode: {:?}",
                        service_snapshot.runtime_mode_state
                    ))
                    .small()
                    .weak(),
                );
            });

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("feature ring: {}", data_bus.features.len()))
                        .small()
                        .weak(),
                );
                ui.add(egui::Slider::new(&mut self.monitor_rows, 3..=40).text("rows"));
                ui.add(egui::Slider::new(&mut self.monitor_preview_values, 4..=64).text("values"));
            });

            if let Some(latest) = data_bus.features.back() {
                ui.label(
                    egui::RichText::new(format!(
                        "latest ts={} dim={}",
                        latest.timestamp,
                        latest.values.len()
                    ))
                    .small(),
                );
            } else {
                ui.label(
                    egui::RichText::new("No feature vectors yet.")
                        .small()
                        .weak(),
                );
            }

            egui::ScrollArea::vertical()
                .id_salt("python_lab_bridge_monitor_scroll")
                .max_height(180.0)
                .show(ui, |ui| {
                    for feature in data_bus.features.iter().rev().take(self.monitor_rows) {
                        let preview = feature
                            .values
                            .iter()
                            .take(self.monitor_preview_values)
                            .map(|v| format!("{:.4}", v))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let ellipsis = if feature.values.len() > self.monitor_preview_values {
                            ", ..."
                        } else {
                            ""
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "[{}] dim={} [{}{}]",
                                feature.timestamp,
                                feature.values.len(),
                                preview,
                                ellipsis
                            ))
                            .monospace()
                            .small(),
                        );
                    }
                });
        });
        ui.add_space(8.0);
    }

    fn delete_cell(&mut self, index: usize) {
        if index >= self.cells.len() {
            return;
        }

        self.cells.remove(index);
        self.queued_cells.retain(|queued| *queued != index);

        if self.cells.is_empty() {
            self.cells.push(NotebookCell::new(String::new()));
            self.selected_cell = 0;
        } else if self.selected_cell >= self.cells.len() {
            self.selected_cell = self.cells.len().saturating_sub(1);
        }
    }

    fn enqueue_all_cells(&mut self, kernel_command: &str) {
        for index in 0..self.cells.len() {
            if self.cells[index].code.trim().is_empty() {
                continue;
            }
            self.enqueue_cell(index, kernel_command);
        }
    }

    fn enqueue_cell(&mut self, cell_index: usize, kernel_command: &str) {
        if cell_index >= self.cells.len() {
            return;
        }
        if self.cells[cell_index].code.trim().is_empty() {
            self.cells[cell_index].output = "Cell is empty; nothing to run.".to_string();
            self.cells[cell_index].status = CellStatus::Error;
            return;
        }

        if self.pending_execution.is_some() {
            if !self.queued_cells.contains(&cell_index) {
                self.queued_cells.push(cell_index);
                self.cells[cell_index].status = CellStatus::Queued;
            }
            return;
        }

        self.execute_cell(cell_index, kernel_command);
    }

    fn execute_cell(&mut self, cell_index: usize, kernel_command: &str) {
        if cell_index >= self.cells.len() {
            return;
        }

        if let Err(error) = self.ensure_kernel(kernel_command) {
            self.cells[cell_index].status = CellStatus::Error;
            self.cells[cell_index].output = format!("Failed to start kernel: {}", error);
            self.log_output
                .push_str(&format!("Kernel start failed: {}\n", error));
            return;
        }

        let Some(kernel) = &mut self.kernel else {
            return;
        };

        let request_id = kernel.next_request_id;
        kernel.next_request_id += 1;

        let code = self.cells[cell_index].code.clone();
        let request = KernelRequest::Execute { request_id, code };

        if let Err(error) = send_kernel_request(&mut kernel.stdin, &request) {
            self.cells[cell_index].status = CellStatus::Error;
            self.cells[cell_index].output = format!("Failed to send execute request: {}", error);
            self.log_output.push_str(&format!(
                "Kernel execute request {} failed: {}\n",
                request_id, error
            ));
            return;
        }

        self.cells[cell_index].status = CellStatus::Running;
        self.cells[cell_index].output.clear();
        self.pending_execution = Some(PendingExecution {
            request_id,
            cell_index,
        });
    }

    fn ensure_kernel(&mut self, kernel_command: &str) -> Result<(), String> {
        if self.kernel.is_some() {
            return Ok(());
        }

        let session = spawn_kernel(kernel_command)?;
        self.log_output
            .push_str(&format!("Spawned kernel: {}\n", kernel_command));
        self.kernel = Some(session);
        Ok(())
    }

    fn stop_kernel(&mut self) {
        self.pending_execution = None;
        self.queued_cells.clear();

        for cell in &mut self.cells {
            if matches!(cell.status, CellStatus::Running | CellStatus::Queued) {
                cell.status = CellStatus::Idle;
            }
        }

        let Some(mut kernel) = self.kernel.take() else {
            return;
        };

        let _ = send_kernel_request(&mut kernel.stdin, &KernelRequest::Shutdown);
        let _ = kernel.child.kill();
        let _ = kernel.child.wait();
        self.log_output.push_str("Kernel stopped.\n");
    }

    fn restart_kernel(&mut self, kernel_command: &str) {
        self.stop_kernel();
        if let Err(error) = self.ensure_kernel(kernel_command) {
            self.log_output
                .push_str(&format!("Kernel restart failed: {}\n", error));
            return;
        }

        if let Some(kernel) = &mut self.kernel {
            let request_id = kernel.next_request_id;
            kernel.next_request_id += 1;
            let _ = send_kernel_request(&mut kernel.stdin, &KernelRequest::Reset { request_id });
        }
    }

    fn kernel_status(&self) -> String {
        let Some(kernel) = &self.kernel else {
            return "stopped".to_string();
        };
        if kernel.ready {
            "running".to_string()
        } else {
            "starting".to_string()
        }
    }

    fn poll_kernel_events(&mut self, kernel_command: &str) {
        let mut drained_events = Vec::new();
        let mut should_drop_kernel = false;

        {
            let Some(kernel) = &mut self.kernel else {
                return;
            };

            loop {
                match kernel.events_rx.try_recv() {
                    Ok(event) => drained_events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.log_output
                            .push_str("Kernel event channel disconnected.\n");
                        should_drop_kernel = true;
                        break;
                    }
                }
            }
        }

        for event in drained_events {
            match event {
                KernelEvent::Message(message) => {
                    self.handle_kernel_message(message);
                    self.dispatch_next_queued(kernel_command);
                }
                KernelEvent::Stderr(text) => {
                    self.log_output
                        .push_str(&format!("[kernel stderr] {}\n", text));
                }
                KernelEvent::ParseError(text) => {
                    self.log_output
                        .push_str(&format!("[kernel parse error] {}\n", text));
                }
                KernelEvent::Exit => {
                    self.log_output.push_str("Kernel process exited.\n");
                    should_drop_kernel = true;
                }
            }
        }

        if should_drop_kernel {
            self.kernel = None;
            self.pending_execution = None;
            self.queued_cells.clear();
            for cell in &mut self.cells {
                if matches!(cell.status, CellStatus::Running | CellStatus::Queued) {
                    cell.status = CellStatus::Error;
                    if cell.output.is_empty() {
                        cell.output = "Kernel exited before returning results.".to_string();
                    }
                }
            }
        }
    }

    fn handle_kernel_message(&mut self, message: KernelResponse) {
        match message {
            KernelResponse::Ready { protocol } => {
                if let Some(kernel) = &mut self.kernel {
                    kernel.ready = true;
                    kernel.protocol = protocol.clone();
                }

                if let Some(protocol) = protocol {
                    self.log_output
                        .push_str(&format!("Kernel ready (protocol: {}).\n", protocol));
                } else {
                    self.log_output.push_str("Kernel ready.\n");
                }
            }
            KernelResponse::ExecuteResult {
                request_id,
                status,
                stdout,
                stderr,
                result,
                error,
                exec_count,
                duration_ms,
            } => {
                let Some(pending) = self.pending_execution.take() else {
                    self.log_output.push_str(&format!(
                        "Received execute result {} but no pending request.\n",
                        request_id
                    ));
                    return;
                };

                if pending.request_id != request_id {
                    self.log_output.push_str(&format!(
                        "Out-of-order execute result: expected {}, got {}.\n",
                        pending.request_id, request_id
                    ));
                }

                if let Some(cell) = self.cells.get_mut(pending.cell_index) {
                    cell.exec_count = Some(exec_count);
                    cell.duration_ms = Some(duration_ms);

                    let mut output = String::new();
                    if !stdout.is_empty() {
                        output.push_str(&stdout);
                        if !stdout.ends_with('\n') {
                            output.push('\n');
                        }
                    }
                    if !stderr.is_empty() {
                        output.push_str("[stderr]\n");
                        output.push_str(&stderr);
                        if !stderr.ends_with('\n') {
                            output.push('\n');
                        }
                    }
                    if let Some(result) = result {
                        output.push_str(&format!("Out: {}\n", result));
                    }

                    if status == "ok" {
                        if output.is_empty() {
                            output = "(no output)".to_string();
                        }
                        cell.status = CellStatus::Success;
                        cell.output = output;
                    } else {
                        cell.status = CellStatus::Error;
                        if let Some(error) = error {
                            output.push_str(&format!(
                                "[error] {}: {}\n{}",
                                error.name, error.message, error.traceback
                            ));
                        }
                        cell.output = output;
                    }
                }
            }
            KernelResponse::ResetResult { request_id } => {
                self.log_output
                    .push_str(&format!("Kernel reset acknowledged ({request_id}).\n"));
            }
            KernelResponse::Pong { request_id } => {
                self.log_output
                    .push_str(&format!("Kernel pong ({request_id}).\n"));
            }
            KernelResponse::Error {
                request_id,
                message,
            } => {
                self.log_output.push_str(&format!(
                    "Kernel error{}: {}\n",
                    request_id.map(|id| format!(" [{id}]")).unwrap_or_default(),
                    message
                ));

                if let Some(pending) = self.pending_execution.take()
                    && let Some(cell) = self.cells.get_mut(pending.cell_index) {
                        cell.status = CellStatus::Error;
                        cell.output = format!("Kernel error: {}", message);
                    }
            }
        }
    }

    fn dispatch_next_queued(&mut self, kernel_command: &str) {
        if self.pending_execution.is_some() {
            return;
        }

        let Some(next_cell_index) = self.queued_cells.first().copied() else {
            return;
        };

        self.queued_cells.remove(0);
        self.execute_cell(next_cell_index, kernel_command);
    }

    fn run_uv_sync(&mut self) {
        if self.uv_sync_task.is_pending() {
            return;
        }

        self.log_output.push_str("$ uv sync\n");

        self.uv_sync_task.request(async {
            tokio::task::spawn_blocking(run_uv_sync_blocking)
                .await
                .map_err(|error| format!("uv sync worker join failed: {}", error))
        });
    }

    fn poll_tool_output(&mut self) {
        if let Some(result) = self.uv_sync_task.take() {
            match result {
                Ok(text) => self.log_output.push_str(&text),
                Err(error) => self
                    .log_output
                    .push_str(&format!("Failed to run uv sync: {}\n", error)),
            }
        }
    }
}

impl Drop for PythonLabScreen {
    fn drop(&mut self) {
        self.stop_kernel();
    }
}

fn status_badge(status: &CellStatus) -> (egui::Color32, &'static str) {
    match status {
        CellStatus::Idle => (egui::Color32::GRAY, "idle"),
        CellStatus::Queued => (egui::Color32::YELLOW, "queued"),
        CellStatus::Running => (egui::Color32::LIGHT_BLUE, "running"),
        CellStatus::Success => (egui::Color32::GREEN, "ok"),
        CellStatus::Error => (egui::Color32::RED, "error"),
    }
}

fn spawn_kernel(command_line: &str) -> Result<KernelSession, String> {
    let parts = split_command_line(command_line);
    if parts.is_empty() {
        return Err("kernel command is empty".to_string());
    }

    let program = &parts[0];
    let args = &parts[1..];

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to spawn '{command_line}': {error}"))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "kernel stdin was not available".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "kernel stdout was not available".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "kernel stderr was not available".to_string())?;

    let (events_tx, events_rx) = mpsc::channel();
    let events_tx_stdout = events_tx.clone();
    let events_tx_stderr = events_tx.clone();

    std::thread::spawn(move || read_kernel_stdout(stdout, events_tx_stdout));
    std::thread::spawn(move || read_kernel_stderr(stderr, events_tx_stderr));

    let mut session = KernelSession {
        child,
        stdin,
        events_rx,
        next_request_id: 1,
        ready: false,
        protocol: None,
    };

    let ping_id = session.next_request_id;
    session.next_request_id += 1;
    send_kernel_request(
        &mut session.stdin,
        &KernelRequest::Ping {
            request_id: ping_id,
        },
    )
    .map_err(|error| format!("failed to send initial ping: {error}"))?;

    Ok(session)
}

fn send_kernel_request(stdin: &mut ChildStdin, request: &KernelRequest) -> Result<(), String> {
    let payload = serde_json::to_string(request)
        .map_err(|error| format!("failed to encode kernel request: {error}"))?;

    stdin
        .write_all(payload.as_bytes())
        .map_err(|error| format!("failed to write kernel request: {error}"))?;
    stdin
        .write_all(b"\n")
        .map_err(|error| format!("failed to write kernel request newline: {error}"))?;
    stdin
        .flush()
        .map_err(|error| format!("failed to flush kernel request: {error}"))
}

fn read_kernel_stdout(stdout: ChildStdout, events_tx: mpsc::Sender<KernelEvent>) {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<KernelResponse>(&line) {
                    Ok(message) => {
                        let _ = events_tx.send(KernelEvent::Message(message));
                    }
                    Err(error) => {
                        let _ = events_tx.send(KernelEvent::ParseError(format!(
                            "{} | line={}",
                            error, line
                        )));
                    }
                }
            }
            Err(error) => {
                let _ = events_tx.send(KernelEvent::ParseError(format!(
                    "failed to read kernel stdout: {}",
                    error
                )));
                break;
            }
        }
    }

    let _ = events_tx.send(KernelEvent::Exit);
}

fn read_kernel_stderr(stderr: ChildStderr, events_tx: mpsc::Sender<KernelEvent>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                if !line.trim().is_empty() {
                    let _ = events_tx.send(KernelEvent::Stderr(line));
                }
            }
            Err(error) => {
                let _ = events_tx.send(KernelEvent::ParseError(format!(
                    "failed to read kernel stderr: {}",
                    error
                )));
                break;
            }
        }
    }
}

fn split_command_line(command_line: &str) -> Vec<String> {
    command_line
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        return format!("{} ms", millis);
    }

    let secs = duration.as_secs();
    let rem_millis = millis % 1_000;
    format!("{}.{:03} s", secs, rem_millis)
}

fn run_uv_sync_blocking() -> String {
    let start = Instant::now();
    let mut command = Command::new("uv");
    command.arg("sync").current_dir("python");

    match command.output() {
        Ok(out) => {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&out.stdout));
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            if text.trim().is_empty() {
                text.push_str("(command produced no output)\n");
            }
            if !out.status.success() {
                text.push_str(&format!("Command exited with status {}\n", out.status));
            }
            text.push_str(&format!("[completed in {}]\n", format_duration(start.elapsed())));
            text
        }
        Err(error) => format!("Failed to run uv sync: {}\n", error),
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::{
        kittest::Queryable,
        Harness,
    };

    use super::PythonLabScreen;
    use crate::{
        data_bus::DataBus,
        state::ServiceSnapshot,
    };

    struct PythonLabHarnessState {
        screen: PythonLabScreen,
        data_bus: DataBus,
        service_snapshot: ServiceSnapshot,
    }

    #[test]
    fn renders_controls_and_add_cell_interaction() {
        let mut harness = Harness::new_ui_state(
            |ui, state: &mut PythonLabHarnessState| {
                state.screen.show(
                    ui,
                    "uv run --project python neurohid-ml kernel-adapter",
                    &state.data_bus,
                    &state.service_snapshot,
                );
            },
            PythonLabHarnessState {
                screen: PythonLabScreen::new(),
                data_bus: DataBus::new(),
                service_snapshot: ServiceSnapshot::default(),
            },
        );

        harness.get_by_label("Add Cell");
        harness.get_by_label("Run All");
        harness.get_by_label("uv sync");
        harness.get_by_label("Kernel / Tool Log");
        harness.get_by_label("Cell 1");

        harness.get_by_label("Add Cell").click();
        harness.run();

        harness.get_by_label("Cell 2");
    }
}
