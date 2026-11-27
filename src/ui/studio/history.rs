use super::*;
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(Default)]
pub(super) struct History {
    entries: Vec<PathBuf>,
    checked: Option<Instant>,
    open: Option<(PathBuf, String)>,
    message: String,
}
impl History {
    pub(super) fn close(&mut self) {
        self.open = None;
        self.message.clear();
    }
    fn refresh(&mut self) {
        if self
            .checked
            .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        self.checked = Some(Instant::now());
        let dir = crate::default_workspace().join("session-logs");
        self.entries = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "log"))
            .collect();
        self.entries.sort_unstable_by(|a, b| b.cmp(a));
    }
    fn read(&mut self, path: &Path) {
        const MAX: u64 = 2 * 1024 * 1024;
        let result = (|| -> io::Result<String> {
            let mut buffer = Vec::new();
            fs::File::open(path)?
                .take(MAX + 1)
                .read_to_end(&mut buffer)?;
            let mut text =
                String::from_utf8_lossy(&buffer[..buffer.len().min(MAX as usize)]).to_string();
            if buffer.len() > MAX as usize {
                text.push_str("\n… Export for the complete log.");
            }
            Ok(text)
        })();
        match result {
            Ok(text) => {
                self.open = Some((path.to_owned(), text));
                self.message.clear();
            }
            Err(e) => self.message = e.to_string(),
        }
    }
    fn show(&mut self, ui: &mut Ui) {
        self.refresh();
        if let Some((path, content)) = &self.open {
            let mut close = false;
            ui.horizontal_wrapped(|ui| {
                close = ui.button("‹  All sessions").clicked();
                if ui.button("Copy visible log").clicked() {
                    ui.ctx().copy_text(content.clone());
                }
                if ui.button("Export log…").clicked() {
                    if let Some(dest) = rfd::FileDialog::new()
                        .set_file_name("palmvi.log")
                        .save_file()
                    {
                        if &dest != path {
                            self.message = fs::copy(path, dest)
                                .map_or_else(|e| e.to_string(), |_| "Log exported.".into());
                        }
                    }
                }
            });
            ui.label(&self.message);
            let mut text = content.as_str();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .font(theme::mono(12.0))
                        .desired_width(f32::INFINITY),
                );
            });
            if close {
                self.close();
            }
            return;
        }
        if self.entries.is_empty() {
            note(ui, "Completed sessions will appear here.");
        }
        let mut selected = None;
        egui::ScrollArea::vertical()
            .id_salt("history")
            .show(ui, |ui| {
                for p in &self.entries {
                    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                    let (stamp, name) = stem.split_once(" - ").unwrap_or(("", &stem));
                    let when = stamp
                        .split('-')
                        .next()
                        .and_then(|t| t.parse::<u64>().ok())
                        .and_then(|t| crate::build_activity::local_time(t / 1000))
                        .unwrap_or_else(|| stamp.to_owned());
                    if ui.button(format!("{when}    {name}")).clicked() {
                        selected = Some(p.clone());
                    }
                }
            });
        if let Some(path) = selected {
            self.read(&path);
        }
        if !self.message.is_empty() {
            note(ui, &self.message);
        }
    }
}
impl App {
    pub(super) fn activity_page(&mut self, ui: &mut Ui) {
        theme::title(ui, "Activity", 42.0, ui.available_width());
        if self.screenshot_projects.is_some() {
            note(ui, "Logs are hidden in sharing mode.");
            return;
        }
        let active: Vec<_> = self
            .project_jobs
            .0
            .values()
            .filter_map(|job| {
                let j = job.lock().unwrap();
                if j.running {
                    j.repo.clone().map(|r| (r, j.stage.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (repo, stage) in active {
            if ui.button(format!("{repo} · {stage}")).clicked() {
                self.select_profile(&repo);
                self.switch_tab(ui.ctx(), &repo, 3);
            }
        }
        if self.job.lock().unwrap().running {
            self.activity(ui, "");
        }
        self.studio.history.show(ui);
    }
}
