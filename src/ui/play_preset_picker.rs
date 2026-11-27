use super::{button_row, launch_presets, note};
use crate::theme::{self, Look};
use crate::{project_key, repo_dir, LaunchPreset, Project, ProjectLaunch};
use eframe::egui;
use std::path::Path;

pub(crate) struct PlayPresetPicker {
    pub(super) project: Project,
    options: Vec<LaunchPreset>,
    recommended: Option<usize>,
    selected: Option<usize>,
}

impl PlayPresetPicker {
    pub(super) fn for_project(
        project: &Project,
        saved: &[LaunchPreset],
        launch: Option<&ProjectLaunch>,
    ) -> Option<Self> {
        if launch.is_some_and(ProjectLaunch::has_confirmed_preset) {
            return None;
        }
        let standalone = project.runs_standalone();
        let repo = project_key(&project.repo);
        let mut options = if standalone {
            Vec::new()
        } else {
            scripts(&repo_dir(&project.repo).join("scripts"), &project.name)
        };
        let recommended = options
            .first()
            .filter(|preset| is_custom_script(&preset.name))
            .map(|_| 0);
        options.extend(
            saved
                .iter()
                .filter(|preset| {
                    preset.projects.contains(&repo)
                        && (!standalone || preset.switches.is_none())
                        && preset.repo_script.is_none()
                })
                .cloned(),
        );
        if recommended.is_none() && options.len() < 2 {
            return None;
        }
        Some(Self {
            project: project.clone(),
            options,
            recommended,
            selected: recommended,
        })
    }

    pub(super) fn show(&mut self, ctx: &egui::Context, opened_at: f64) -> (bool, bool) {
        let mut cancelled = false;
        let (accepted, dismissed) = theme::modal(ctx, "play-preset", opened_at, 460.0, |ui| {
            theme::title(ui, "Choose launch preset", 36.0, 460.0);
            note(
                ui,
                "Confirm once to save your launch settings for this profile.",
            );
            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .max_height(238.0)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.spacing_mut().button_padding = egui::vec2(15.0, 9.0);
                    if choice_card(
                        ui,
                        "Current profile settings",
                        "Keep your existing arguments and switches.",
                        self.selected.is_none(),
                    ) {
                        self.selected = None;
                    }
                    for (index, preset) in self.options.iter().enumerate() {
                        let (title, detail) = match preset.repo_script.as_deref() {
                            Some(script) if is_custom_script(script) => (
                                "Detected custom script",
                                if self.recommended == Some(index) {
                                    format!("{script}.ps1 · Preselected for this build")
                                } else {
                                    format!("{script}.ps1 · Found in the project")
                                },
                            ),
                            Some(script) => (
                                "Standard repo launch",
                                format!("{script}.ps1 · Use the repository’s standard settings"),
                            ),
                            None => (preset.name.as_str(), "Use this saved preset.".into()),
                        };
                        if choice_card(ui, title, &detail, self.selected == Some(index)) {
                            self.selected = Some(index);
                        }
                    }
                });
            ui.add_space(14.0);
            let mut continue_clicked = false;
            let label = if self.project.runs_standalone() {
                if self.selected.is_some() {
                    "Apply preset and launch"
                } else {
                    "Launch with current settings"
                }
            } else if self.selected.is_some() {
                "Apply preset and choose save"
            } else {
                "Choose save with current settings"
            };
            button_row(ui, |ui| {
                continue_clicked = theme::button(ui, Look::Primary, None, label).clicked();
                cancelled = theme::button(ui, Look::Ghost, None, "Cancel").clicked();
            });
            continue_clicked
        });
        (accepted, dismissed || cancelled)
    }

    pub(super) fn into_choice(self) -> (Project, Option<LaunchPreset>) {
        let preset = self
            .selected
            .and_then(|index| self.options.into_iter().nth(index));
        (self.project, preset)
    }
}

pub(super) fn detected_preset(project: &Project) -> Option<LaunchPreset> {
    scripts(&repo_dir(&project.repo).join("scripts"), &project.name)
        .into_iter()
        .next()
        .filter(|preset| is_custom_script(&preset.name))
}

fn choice_card(ui: &mut egui::Ui, title: &str, detail: &str, selected: bool) -> bool {
    let mut text = egui::text::LayoutJob::default();
    text.append(
        title,
        0.0,
        egui::TextFormat {
            font_id: theme::strong(17.0),
            color: theme::WHITE,
            ..Default::default()
        },
    );
    text.append("\n", 0.0, egui::TextFormat::default());
    text.append(
        detail,
        0.0,
        egui::TextFormat {
            font_id: theme::regular(13.0),
            color: theme::MUTED,
            ..Default::default()
        },
    );
    let row = egui::Button::new(text)
        .min_size(egui::vec2(ui.available_width(), 62.0))
        .fill(if selected { theme::RAISED } else { theme::DEEP })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if selected {
                theme::PINK
            } else {
                theme::LINE_STRONG
            },
        ));
    ui.add(row).clicked()
}

fn is_custom_script(stem: &str) -> bool {
    stem.to_ascii_lowercase().starts_with("play-") && stem.len() > "play-".len()
}

fn scripts(folder: &Path, project_name: &str) -> Vec<LaunchPreset> {
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?;
            let supported = path.extension()?.eq_ignore_ascii_case("ps1")
                && (stem.eq_ignore_ascii_case("play") || is_custom_script(stem));
            supported.then_some(path)
        })
        .collect();
    let name = project_name.to_ascii_lowercase();
    let matching = format!("play-{name}");
    let fork = format!("play-{}", name.strip_prefix("victory-").unwrap_or(&name));
    files.sort_by_key(|path| {
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let rank = if stem == matching || stem == fork {
            0
        } else if is_custom_script(&stem) {
            1
        } else {
            2
        };
        (rank, stem)
    });
    files
        .iter()
        .take(32)
        .filter_map(|path| {
            let mut preset = launch_presets::import_file(path).ok()?;
            preset.repo_script = Some(preset.name.clone());
            Some(preset)
        })
        .collect()
}

#[cfg(test)]
#[test]
fn prefills_the_matching_custom_script_ahead_of_the_standard_script() {
    let folder = std::env::temp_dir().join(format!("mandyhub-play-presets-{}", std::process::id()));
    std::fs::create_dir(&folder).unwrap();
    let valid = "$settings = @{\nVICTORY_LOG = 'warn'\n}";
    let fixtures = [
        ("play.ps1", valid),
        ("play-scarlett.ps1", valid),
        ("play-alt.ps1", valid),
        ("play-dynamic.ps1", "Write-Host 'No preset here'"),
        ("build.ps1", valid),
    ];
    for (name, contents) in fixtures {
        std::fs::write(folder.join(name), contents).unwrap();
    }
    let options = scripts(&folder, "victory-scarlett");
    let names: Vec<_> = options.iter().map(|preset| preset.name.as_str()).collect();
    assert_eq!(names, ["play-scarlett", "play-alt", "play"]);
    assert!(is_custom_script(names[0]));
    for (name, _) in fixtures {
        std::fs::remove_file(folder.join(name)).unwrap();
    }
    std::fs::remove_dir(folder).unwrap();
}

#[cfg(test)]
#[test]
fn accepting_current_settings_or_a_preset_skips_future_prompts() {
    let project = Project {
        name: "Fixture".into(),
        repo: "test/preset-picker-fixture".into(),
        branch: None,
        gpu: Vec::new(),
        fps: None,
        render: None,
        verified: false,
        summary: String::new(),
        custom: true,
        local_exe: None,
        archive_exe: None,
    };
    let presets = ["First", "Second"].map(|name| LaunchPreset {
        name: name.into(),
        args: String::new(),
        switches: None,
        script_options: None,
        projects: vec![project.repo.clone()],
        repo_script: None,
    });
    assert!(PlayPresetPicker::for_project(&project, &presets, None).is_some());
    let current = ProjectLaunch {
        preset_choice_confirmed: true,
        ..ProjectLaunch::default()
    };
    assert!(PlayPresetPicker::for_project(&project, &presets, Some(&current)).is_none());
    let previous = ProjectLaunch {
        confirmed_repo_script: Some("play-scarlett".into()),
        ..ProjectLaunch::default()
    };
    assert!(PlayPresetPicker::for_project(&project, &presets, Some(&previous)).is_none());
}
