//! # Python Lab Screen
//!
//! Lightweight in-app editor and command runner scoped to the `python/` workspace.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

use eframe::egui;

pub struct PythonLabScreen {
    files: Vec<PathBuf>,
    selected: Option<PathBuf>,
    editor_text: String,
    output: String,
    dirty: bool,
    running: bool,
    run_rx: Option<Receiver<String>>,
}

impl PythonLabScreen {
    pub fn new() -> Self {
        let mut this = Self {
            files: Vec::new(),
            selected: None,
            editor_text: String::new(),
            output: String::new(),
            dirty: false,
            running: false,
            run_rx: None,
        };
        this.refresh_files();
        this
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll_background_output();

        ui.heading("Python Lab");
        ui.label(
            egui::RichText::new("Scoped to ./python with uv tooling")
                .small()
                .weak(),
        );

        ui.horizontal(|ui| {
            if ui.button("Refresh Files").clicked() {
                self.refresh_files();
            }

            if ui
                .add_enabled(
                    self.selected.is_some() && self.dirty,
                    egui::Button::new("Save"),
                )
                .clicked()
            {
                self.save_selected_file();
            }

            if ui
                .add_enabled(
                    self.selected.is_some() && !self.running,
                    egui::Button::new("Run Selected"),
                )
                .clicked()
            {
                self.run_uv_selected();
            }

            if ui
                .add_enabled(!self.running, egui::Button::new("uv sync"))
                .clicked()
            {
                self.run_uv_sync();
            }

            if self.running {
                ui.label(egui::RichText::new("Running...").color(egui::Color32::YELLOW));
            }
        });

        ui.separator();

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ui.label(egui::RichText::new("Files").strong());
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut to_select: Option<PathBuf> = None;
                    for file in &self.files {
                        let label = file.display().to_string();
                        let selected = self.selected.as_ref() == Some(file);
                        if ui.selectable_label(selected, label).clicked() {
                            to_select = Some(file.clone());
                        }
                    }
                    if let Some(path) = to_select {
                        self.select_file(path);
                    }
                });
            });

            cols[1].vertical(|ui| {
                let current = self
                    .selected
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<none>".to_string());
                ui.label(egui::RichText::new(format!("Editor: {current}")).strong());
                let edit = egui::TextEdit::multiline(&mut self.editor_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(20)
                    .desired_width(f32::INFINITY);
                if ui.add(edit).changed() {
                    self.dirty = true;
                }

                ui.separator();
                ui.label(egui::RichText::new("Output").strong());
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.output)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });
            });
        });
    }

    fn refresh_files(&mut self) {
        self.files.clear();
        let root = PathBuf::from("python/src");
        collect_python_files(&root, &mut self.files);
        self.files.sort();
    }

    fn select_file(&mut self, path: PathBuf) {
        match fs::read_to_string(&path) {
            Ok(contents) => {
                self.selected = Some(path);
                self.editor_text = contents;
                self.dirty = false;
            }
            Err(e) => {
                self.output.push_str(&format!("Failed to open file: {e}\n"));
            }
        }
    }

    fn save_selected_file(&mut self) {
        let Some(path) = &self.selected else {
            return;
        };
        match fs::write(path, &self.editor_text) {
            Ok(()) => {
                self.output.push_str(&format!("Saved {}\n", path.display()));
                self.dirty = false;
            }
            Err(e) => {
                self.output
                    .push_str(&format!("Failed to save {}: {e}\n", path.display()));
            }
        }
    }

    fn run_uv_selected(&mut self) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        self.spawn_uv(vec![
            "run".to_string(),
            "python".to_string(),
            path.display().to_string(),
        ]);
    }

    fn run_uv_sync(&mut self) {
        self.spawn_uv(vec!["sync".to_string()]);
    }

    fn spawn_uv(&mut self, args: Vec<String>) {
        let (tx, rx) = mpsc::channel();
        self.running = true;
        self.run_rx = Some(rx);
        self.output.push_str(&format!("$ uv {}\n", args.join(" ")));

        std::thread::spawn(move || {
            let mut cmd = Command::new("uv");
            cmd.args(args).current_dir("python");
            let text = match cmd.output() {
                Ok(out) => {
                    let mut s = String::new();
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                    if !out.status.success() {
                        s.push_str(&format!("\nCommand exited with status {}\n", out.status));
                    }
                    s
                }
                Err(e) => format!("Failed to run uv command: {e}\n"),
            };
            let _ = tx.send(text);
        });
    }

    fn poll_background_output(&mut self) {
        let Some(rx) = &self.run_rx else {
            return;
        };
        if let Ok(text) = rx.try_recv() {
            self.output.push_str(&text);
            self.running = false;
            self.run_rx = None;
        }
    }
}

fn collect_python_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for ent in entries.flatten() {
        let path = ent.path();
        if path.is_dir() {
            collect_python_files(&path, out);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) == Some("py") {
            out.push(path);
        }
    }
}
