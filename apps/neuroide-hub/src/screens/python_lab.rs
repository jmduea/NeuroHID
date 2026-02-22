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
use crate::theme;

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

        theme::page_header(
            ui,
            "Python Lab",
            "Notebook-style execution over decoupled kernel IPC. Kernel protocol: JSON lines over stdio (execute/reset/ping/shutdown)",
        );
        self.show_bridge_monitor(ui, data_bus, service_snapshot);

        let running_cells = self
            .cells
            .iter()
            .filter(|cell| matches!(cell.status, CellStatus::Running))
            .count();
        let error_cells = self
            .cells
            .iter()
            .filter(|cell| matches!(cell.status, CellStatus::Error))
            .count();
        let success_cells = self
            .cells
            .iter()
            .filter(|cell| matches!(cell.status, CellStatus::Success))
            .count();

        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                theme::operation_state_chip(ui, "Kernel", self.kernel_status());

                theme::status_chip(
                    ui,
                    &format!("Cells {}", self.cells.len()),
                    theme::Intent::Info,
                );
                theme::status_chip(
                    ui,
                    &format!("Queued {}", self.queued_cells.len()),
                    if self.queued_cells.is_empty() {
                        theme::Intent::Muted
                    } else {
                        theme::Intent::Warning
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Running {}", running_cells),
                    if running_cells == 0 {
                        theme::Intent::Muted
                    } else {
                        theme::Intent::Info
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Succeeded {}", success_cells),
                    if success_cells == 0 {
                        theme::Intent::Muted
                    } else {
                        theme::Intent::Success
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Errors {}", error_cells),
                    if error_cells == 0 {
                        theme::Intent::Muted
                    } else {
                        theme::Intent::Danger
                    },
                );
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

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if action_button_tone(ui, "Add Cell", true, theme::ButtonTone::Secondary) {
                    self.cells.push(NotebookCell::new(String::new()));
                    self.selected_cell = self.cells.len().saturating_sub(1);
                }

                let run_selected = self.selected_cell < self.cells.len();
                if action_button_tone(ui, "Run Selected", run_selected, theme::ButtonTone::Primary)
                {
                    self.enqueue_cell(self.selected_cell, kernel_command);
                }

                if action_button_tone(ui, "Run All", true, theme::ButtonTone::Secondary) {
                    self.enqueue_all_cells(kernel_command);
                }

                if action_button_tone(ui, "Restart Kernel", true, theme::ButtonTone::Secondary) {
                    self.restart_kernel(kernel_command);
                }

                if action_button_tone(ui, "Stop Kernel", true, theme::ButtonTone::Ghost) {
                    self.stop_kernel();
                }

                let uv_sync_running = self.uv_sync_task.is_pending();
                if action_button_tone(
                    ui,
                    "uv sync",
                    !uv_sync_running,
                    theme::ButtonTone::Secondary,
                ) {
                    self.run_uv_sync();
                }

                if action_button_tone(ui, "Clear Log", true, theme::ButtonTone::Ghost) {
                    self.log_output.clear();
                }
            });
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
                        theme::card_frame(ui).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let selected = self.selected_cell == index;
                                if theme::nav_button(ui, &format!("Cell {}", index + 1), selected)
                                    .clicked()
                                {
                                    self.selected_cell = index;
                                }

                                let can_run = !matches!(cell.status, CellStatus::Running);
                                if action_button_tone(
                                    ui,
                                    "Run",
                                    can_run,
                                    theme::ButtonTone::Primary,
                                ) {
                                    run_clicked = Some(index);
                                }

                                let can_delete = !matches!(cell.status, CellStatus::Running);
                                if action_button_tone(
                                    ui,
                                    "Delete",
                                    can_delete,
                                    theme::ButtonTone::Ghost,
                                ) {
                                    delete_clicked = Some(index);
                                }

                                let (text, intent) = status_badge(&cell.status);
                                theme::status_chip(ui, text, intent);

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
                            let _ = theme::textarea_readonly(
                                ui,
                                format!("python_cell_output_{}", index),
                                &mut cell.output,
                                4,
                                f32::INFINITY,
                            );
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
        theme::card_frame(ui).show(ui, |ui| {
            ui.label(egui::RichText::new("Kernel / Tool Log").strong());
            egui::ScrollArea::vertical()
                .id_salt("python_lab_log_scroll")
                .max_height(180.0)
                .show(ui, |ui| {
                    let _ = theme::textarea_readonly(
                        ui,
                        "python_lab_log_output",
                        &mut self.log_output,
                        8,
                        f32::INFINITY,
                    );
                });
        });
    }

    fn show_bridge_monitor(
        &mut self,
        ui: &mut egui::Ui,
        data_bus: &DataBus,
        service_snapshot: &ServiceSnapshot,
    ) {
        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Bridge Monitor (Always On)").strong());

                let (bridge_text, bridge_intent) = if service_snapshot.ml_bridge_connected {
                    if service_snapshot.ml_bridge_stalled {
                        ("ML bridge stalled", theme::Intent::Warning)
                    } else {
                        ("ML bridge connected", theme::Intent::Success)
                    }
                } else {
                    ("ML bridge disconnected", theme::Intent::Muted)
                };
                theme::status_chip(ui, bridge_text, bridge_intent);

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
                let mut monitor_rows = self.monitor_rows as f32;
                if theme::slider_f32(
                    ui,
                    "python_lab_monitor_rows",
                    &mut monitor_rows,
                    3.0,
                    40.0,
                    Some("rows"),
                ) {
                    self.monitor_rows = monitor_rows.round().clamp(3.0, 40.0) as usize;
                }
                let mut monitor_preview_values = self.monitor_preview_values as f32;
                if theme::slider_f32(
                    ui,
                    "python_lab_monitor_preview_values",
                    &mut monitor_preview_values,
                    4.0,
                    64.0,
                    Some("values"),
                ) {
                    self.monitor_preview_values =
                        monitor_preview_values.round().clamp(4.0, 64.0) as usize;
                }
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
                theme::status_chip(ui, "No feature vectors yet", theme::Intent::Warning);
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

    fn kernel_status(&self) -> theme::OperationState {
        let Some(kernel) = &self.kernel else {
            return theme::OperationState::Idle;
        };
        if kernel.ready {
            theme::OperationState::Ready
        } else {
            theme::OperationState::Running
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
                    && let Some(cell) = self.cells.get_mut(pending.cell_index)
                {
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

fn status_badge(status: &CellStatus) -> (&'static str, theme::Intent) {
    match status {
        CellStatus::Idle => ("idle", theme::Intent::Muted),
        CellStatus::Queued => ("queued", theme::Intent::Warning),
        CellStatus::Running => ("running", theme::Intent::Info),
        CellStatus::Success => ("ok", theme::Intent::Success),
        CellStatus::Error => ("error", theme::Intent::Danger),
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
            text.push_str(&format!(
                "[completed in {}]\n",
                format_duration(start.elapsed())
            ));
            text
        }
        Err(error) => format!("Failed to run uv sync: {}\n", error),
    }
}

fn action_button_tone(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    tone: theme::ButtonTone,
) -> bool {
    theme::action_button(ui, label, enabled, tone)
}

#[cfg(test)]
mod tests {
    use egui_kittest::{Harness, kittest::Queryable};

    use super::PythonLabScreen;
    use crate::{data_bus::DataBus, state::ServiceSnapshot};

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
