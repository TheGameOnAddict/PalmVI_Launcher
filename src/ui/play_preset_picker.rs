use super::{button_row, launch_presets, note};
use crate::theme::{self, Look};
use crate::{project_key, project_script_root, LaunchPreset, Project, ProjectLaunch};
use eframe::egui;
use std::path::Path;

pub(crate) struct PlayPresetPicker {
    pub(super) project: Project,
    options: Vec<LaunchPreset>,
    recommended: Option<usize>,
    selected: Option<usize>,
    missing_script: Option<String>,
    pub(super) settings_only: bool,
    after_update: bool,
    pub(super) revision: Option<String>,
}

impl PlayPresetPicker {
    pub(super) fn for_project(
        project: &Project,
        saved: &[LaunchPreset],
        launch: Option<&ProjectLaunch>,
    ) -> Option<Self> {
        Self::for_installed(project, saved, launch, crate::installed_revision(&project.repo),
            &project_script_root(project).join("scripts"))
    }

    fn for_installed(project: &Project, saved: &[LaunchPreset], launch: Option<&ProjectLaunch>,
        revision: Option<String>, folder: &Path) -> Option<Self> {
        let updated = launch.is_some_and(|l| l.scripts_need_review(revision.as_deref())) && !project.runs_standalone();
        if !updated && launch.is_some_and(ProjectLaunch::has_confirmed_preset) {
            return None;
        }
        let picker = Self::scan(project, saved, launch, revision, false, updated, folder);
        let had_script = picker.previous_script(saved, launch).is_some();
        if updated {
            if !had_script && !picker.options.iter().any(|p| p.repo_script.is_some()) { return None; }
        } else if picker.recommended.is_none() && picker.options.len() < 2 {
            return None;
        }
        Some(picker)
    }

    pub(super) fn rescan(project: &Project, saved: &[LaunchPreset], launch: &ProjectLaunch) -> Self {
        Self::scan(project, saved, Some(launch), crate::installed_revision(&project.repo), true, false,
            &project_script_root(project).join("scripts"))
    }

    pub(crate) fn after_project_update(project: &Project, saved: &[LaunchPreset], launch: Option<&ProjectLaunch>) -> Option<Self> {
        let mut picker = Self::for_project(project, saved, launch).filter(|p| p.after_update)?;
        picker.settings_only = true;
        Some(picker)
    }

    fn previous_script<'a>(&self, saved: &'a [LaunchPreset], launch: Option<&'a ProjectLaunch>) -> Option<&'a str> {
        let launch = launch?;
        saved.iter().find(|preset| Some(&preset.name) == launch.selected_preset.as_ref()
            && preset.projects.contains(&project_key(&self.project.repo)))
            .and_then(|p| p.repo_script.as_deref()).or(launch.confirmed_repo_script.as_deref())
    }

    pub(super) fn recommended_script(&self) -> Option<LaunchPreset> {
        self.recommended.map(|i| self.options[i].clone())
    }

    fn scan(project: &Project, saved: &[LaunchPreset], launch: Option<&ProjectLaunch>, revision: Option<String>,
        settings_only: bool, after_update: bool, folder: &Path) -> Self {
        let standalone = project.runs_standalone();
        let repo = project_key(&project.repo);
        let mut options = if standalone {
            Vec::new()
        } else {
            scripts(folder, &project.name)
        };
        let recommended = options
            .first()
            .filter(|preset| is_custom_script(&preset.name))
            .map(|_| 0);
        options.extend(
            saved
                .iter()
                .filter(|preset| {
                    !settings_only && preset.projects.contains(&repo)
                        && (!standalone || preset.switches.is_none())
                        && preset.repo_script.is_none()
                })
                .cloned(),
        );
        let mut picker = Self {
            project: project.clone(),
            options,
            recommended,
            selected: recommended,
            missing_script: None,
            settings_only, after_update, revision,
        };
        if let Some(script) = picker.previous_script(saved, launch) {
            // A removed script leaves "Keep current values" selected until the player chooses a replacement.
            picker.selected = picker.options.iter().position(|p| p.repo_script.as_deref() == Some(script));
            if picker.selected.is_none() { picker.missing_script = Some(script.to_owned()); }
        } else if let Some(name) = launch.and_then(|l| l.selected_preset.as_deref()) {
            picker.selected = picker.options.iter().position(|p| p.repo_script.is_none() && p.name == name);
        }
        picker
    }

    pub(super) fn show(&mut self, ctx: &egui::Context, opened_at: f64) -> (bool, bool) {
        let mut cancelled = false;
        let (accepted, dismissed) = theme::modal(ctx, "play-preset", opened_at, 460.0, |ui| {
            theme::title(ui, if self.settings_only || self.after_update { "Review launch scripts" } else { "Choose launch preset" }, 36.0, 460.0);
            note(
                ui,
                if self.options.iter().all(|p| p.repo_script.is_none()) && (self.settings_only || self.after_update) {
                    "No supported launch scripts were found in this build’s scripts folder."
                } else if self.after_update {
                    "This project was updated. Choose a script from the installed build or keep your current values."
                } else if self.settings_only {
                    "Choose a script from the current installation to use for this profile."
                } else { "Confirm once to save your launch settings for this profile." },
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
                        &self.missing_script.as_ref().map_or_else(
                            || "Keep your existing arguments and switches.".to_owned(),
                            |s| format!("{s}.ps1 was not found. Keep the values saved in this profile."),
                        ),
                        self.selected.is_none(),
                    ) {
                        self.selected = None;
                    }
                    for (index, preset) in self.options.iter().enumerate() {
                        let (title, detail) = match preset.repo_script.as_deref() {
                            Some(script) if is_custom_script(script) => (
                                "Detected custom script",
                                if self.recommended == Some(index) {
                                    format!("{script}.ps1 · Recommended for this build")
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
            let label = if self.settings_only {
                if self.selected.is_some() { "Apply to profile" } else { "Keep current values" }
            } else if self.project.runs_standalone() {
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
    scripts(&project_script_root(project).join("scripts"), &project.name)
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
    // The checkout may also ship alternate tuning scripts. Prefer the one its
    // Play CMD actually invokes when the wrappers agree on a single entry point.
    let mut targets = std::collections::BTreeSet::new();
    for entry in folder.parent().and_then(|root| root.read_dir().ok()).into_iter().flatten().flatten() {
        let path = entry.path();
        let wrapper = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !wrapper.starts_with("play") || !wrapper.ends_with(".cmd")
            || !entry.metadata().is_ok_and(|m| m.is_file() && m.len() < 128 * 1024) { continue; }
        if let Ok(cmd) = std::fs::read_to_string(path) {
            for line in cmd.lines() {
                let line = line.trim_start_matches(|ch: char| ch == '@' || ch.is_whitespace()).to_ascii_lowercase();
                if !line.starts_with("powershell ") && !line.starts_with("powershell.exe ") { continue; }
                let Some((_, file)) = line.split_once(" -file ") else { continue };
                let file = file.trim_start().trim_start_matches('"').replace('/', "\\");
                if let Some(target) = file.strip_prefix("%~dp0scripts\\").and_then(|f| f.split_once(".ps1")).map(|p| p.0) {
                    targets.insert(target.to_owned());
                }
            }
        }
    }
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
        (targets.len() != 1 || !targets.contains(&stem), rank, stem)
    });
    files
        .iter()
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
    let root = std::env::temp_dir().join(format!("mandyhub-play-presets-{}", std::process::id()));
    let folder = root.join("scripts");
    std::fs::create_dir_all(&folder).unwrap();
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
    let wrapper = root.join("Play.cmd");
    std::fs::write(&wrapper, r#"powershell -NoProfile -File "%~dp0scripts\play-alt.ps1""#).unwrap();
    assert_eq!(scripts(&folder, "victory-scarlett")[0].name, "play-alt");
    std::fs::remove_file(wrapper).unwrap();
    for (name, _) in fixtures {
        std::fs::remove_file(folder.join(name)).unwrap();
    }
    std::fs::remove_dir(folder).unwrap();
    std::fs::remove_dir(root).unwrap();
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

#[cfg(test)]
#[test]
fn review_reads_the_installed_scripts_and_remembers_the_confirmed_revision() {
    let root = std::env::temp_dir().join(format!("mandyhub-script-review-{}", std::process::id()));
    let folder = root.join("scripts");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("play-custom.ps1");
    let write_script = |v| std::fs::write(&path, format!("$set = @{{\nVICTORY_A = '{v}'\nVICTORY_B = '2'\n}}")).unwrap();
    write_script("1");
    let project = Project { name: "Fixture".into(), repo: "test/script-review".into(),
        branch: None, gpu: vec![], fps: None, render: None, verified: false, summary: String::new(),
        custom: true, local_exe: Some(root.join("victory.exe").display().to_string()), archive_exe: None };
    let mut library = Vec::new();
    let mut launch = ProjectLaunch::default();
    let manual = PlayPresetPicker::rescan(&project, &library, &launch);
    assert!(manual.settings_only);
    let (_, first) = manual.into_choice();
    launch_presets::apply_to_profile(&mut library, &mut launch, &project.repo, first.unwrap()).unwrap();
    launch.reviewed_script_revision = Some("old".into());
    launch.args = Some("--strict".into());
    launch.switches = Some("VICTORY_A=1\nVICTORY_B=9".into());
    write_script("5");
    let review = |revision: &str, lib: &[LaunchPreset], profile: &ProjectLaunch|
        PlayPresetPicker::for_installed(&project, lib, Some(profile), Some(revision.into()), &folder);
    assert!(review("old", &library, &launch).is_none());
    let updated = review("new", &library, &launch).unwrap();
    assert!(updated.after_update);
    let (_, selected) = updated.into_choice();
    assert_eq!(selected.as_ref().unwrap().repo_script.as_deref(), Some("play-custom"));
    launch_presets::apply_to_profile(&mut library, &mut launch, &project.repo, selected.unwrap()).unwrap();
    launch.reviewed_script_revision = Some("new".into());
    assert_eq!(library.len(), 1);
    assert_eq!(launch.switches.as_deref(), Some("VICTORY_A=5\nVICTORY_B=9"));
    assert_eq!(launch.args.as_deref(), Some("--strict"));
    assert!(review("new", &library, &launch).is_none());
    std::fs::rename(&path, folder.join("play-next.ps1")).unwrap();
    let renamed = PlayPresetPicker::rescan(&project, &library, &launch);
    assert_eq!(renamed.options.len(), 1);
    assert_eq!(renamed.options[0].name, "play-next");
    assert!(renamed.selected.is_none());
    std::fs::remove_file(folder.join("play-next.ps1")).unwrap();
    let removed = review("newer", &library, &launch).unwrap();
    assert!(removed.options.is_empty());
    assert!(removed.selected.is_none());
    std::fs::remove_dir(folder).unwrap();
    std::fs::remove_dir(root).unwrap();
}
