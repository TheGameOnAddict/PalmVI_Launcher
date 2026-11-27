use super::{note, section, wrapped};
use crate::script_preset;
use crate::theme::{self, Look};
use crate::{LaunchPreset, Project, ProjectLaunch};
use eframe::egui::{self, Ui};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_PRESET_BYTES: usize = 128 * 1024;

#[derive(Clone, Default)]
struct Picker {
    selected: Option<String>,
    detected: Option<LaunchPreset>,
    last_applied: Option<String>,
    confirm_delete: bool,
    show_save: bool,
    show_paste: bool,
    name: String,
    pasted: String,
    args_only: bool,
    save_status: Option<Result<String, String>>,
    status: Option<Result<String, String>>,
}

pub(super) fn show(
    ui: &mut Ui,
    presets: &mut Vec<LaunchPreset>,
    launch: &mut ProjectLaunch,
    global_args: &str,
    global_switches: &str,
    project: &Project,
) {
    let standalone = project.runs_standalone();
    let project_key = crate::project_key(&project.repo);
    let id = ui.make_persistent_id("launch-preset-picker");
    let mut picker = ui
        .ctx()
        .data(|data| data.get_temp::<Picker>(id))
        .unwrap_or_else(|| Picker {
            detected: (!standalone)
                .then(|| super::play_preset_picker::detected_preset(project))
                .flatten(),
            ..Picker::default()
        });
    let compatible = |preset: &LaunchPreset| !standalone || preset.switches.is_none();
    if picker.last_applied != launch.selected_preset {
        picker.last_applied = launch.selected_preset.clone();
        picker.select(launch.selected_preset.clone());
    }
    if picker.selected.as_ref().is_some_and(|name| {
        !presets
            .iter()
            .any(|preset| preset.name == *name && compatible(preset))
    }) {
        picker.select(None);
    }

    section(ui, "Project launch settings", None, "", |ui| {
        let inherited_args = if standalone { "" } else { global_args };
        ui.horizontal_wrapped(|ui| {
            ui.strong("Launch arguments");
            if !standalone {
                reset_to_global(ui, &mut launch.args);
            }
        });
        let mut args = launch.args.as_deref().unwrap_or(inherited_args).to_owned();
        let hint = if standalone {
            "Paste program arguments"
        } else {
            "Paste launch arguments, e.g. --strict"
        };
        if theme::field(ui, &mut args, hint, ui.available_width(), None).changed() {
            launch.args = Some(args);
        }
        if !standalone {
            note(ui, "Put game arguments after --.");
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.strong("Launch switches");
                if reset_to_global(ui, &mut launch.switches) {
                    launch.script_options = None;
                }
            });
            let mut switches = launch
                .switches
                .as_deref()
                .unwrap_or(global_switches)
                .to_owned();
            if let Some(options) = launch.script_options {
                show_script_options(ui, &mut switches, options);
            }
            egui::ScrollArea::vertical()
                .id_salt("switches")
                .max_height(140.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut switches)
                            .id_salt("switches-text")
                            .font(theme::mono(12.5))
                            .hint_text("Paste switches here, one VICTORY_NAME=value per line")
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                });
            if switches != launch.switches.as_deref().unwrap_or(global_switches) {
                launch.switches = Some(switches);
            }
        }

        ui.add_space(8.0);
        if !picker.show_save
            && theme::small(ui, Look::Primary, None, "Save as preset…", true).clicked()
        {
            picker.show_save = true;
            picker.save_status = None;
        }
        if picker.show_save {
            ui.horizontal_wrapped(|ui| {
                theme::field(ui, &mut picker.name, "Preset name", 200.0, None);
                if !standalone {
                    ui.checkbox(&mut picker.args_only, "Arguments only")
                        .on_hover_text("Presets include launch switches by default.");
                }
                let include_switches = !standalone && !picker.args_only;
                let save_label = if include_switches {
                    "Save arguments and switches"
                } else {
                    "Save arguments"
                };
                if theme::small(
                    ui,
                    Look::Primary,
                    None,
                    save_label,
                    !picker.name.trim().is_empty(),
                )
                .clicked()
                {
                    let preset = LaunchPreset {
                        name: picker.name.trim().to_owned(),
                        args: launch.args.as_deref().unwrap_or(inherited_args).to_owned(),
                        switches: include_switches.then(|| {
                            launch
                                .switches
                                .as_deref()
                                .unwrap_or(global_switches)
                                .to_owned()
                        }),
                        script_options: if include_switches {
                            launch.script_options
                        } else {
                            None
                        },
                        projects: vec![project_key.clone()],
                        repo_script: None,
                    };
                    picker.save_status = Some(store(presets, preset).map(|index| {
                        let name = presets[index].name.clone();
                        picker.select(Some(name.clone()));
                        picker.name.clear();
                        picker.show_save = false;
                        format!("Saved “{name}” to your presets.")
                    }));
                }
                if theme::small(ui, Look::Ghost, None, "Cancel", true).clicked() {
                    picker.show_save = false;
                    picker.save_status = None;
                }
            });
        }
        show_status(ui, &picker.save_status);
    });

    ui.add_space(12.0);
    section(ui, "Saved presets", None, "", |ui| {
        ui.horizontal_wrapped(|ui| {
            let detected = picker
                .detected
                .as_ref()
                .filter(|candidate| {
                    !presets.iter().any(|saved| {
                        saved.repo_script == candidate.repo_script
                            && saved.projects.contains(&project_key)
                    })
                })
                .cloned();
            let label = picker
                .selected
                .clone()
                .or_else(|| {
                    detected
                        .as_ref()
                        .map(|preset| format!("{} · Detected script", preset.name))
                })
                .unwrap_or_else(|| "Choose a preset".into());
            egui::ComboBox::from_id_salt("preset")
                .selected_text(label)
                .width(185.0)
                .show_ui(ui, |ui| {
                    if let Some(detected) = &detected {
                        if ui
                            .selectable_label(
                                picker.selected.is_none(),
                                format!("{} · Detected script", detected.name),
                            )
                            .clicked()
                        {
                            picker.select(None);
                        }
                    } else if !presets.iter().any(compatible) {
                        ui.label("No saved presets for this project type.");
                    }
                    for preset in presets.iter().filter(|preset| compatible(preset)) {
                        if ui
                            .selectable_label(
                                picker.selected.as_deref() == Some(&preset.name),
                                &preset.name,
                            )
                            .clicked()
                        {
                            picker.select(Some(preset.name.clone()));
                        }
                    }
                });
            let selected = picker.preset(presets).or(detected.as_ref());
            let apply = theme::small(
                ui,
                Look::Primary,
                None,
                "Apply to project",
                selected.is_some(),
            )
            .clicked();
            if let Some(preset) = selected {
                ui.menu_button("Share", |ui| {
                    if ui.button("Export JSON file…").clicked() {
                        ui.close_menu();
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Launch preset", &["json"])
                            .set_file_name("launch-preset.json")
                            .save_file()
                        {
                            picker.status = Some(encode(preset).and_then(|json| {
                                std::fs::write(path, json)
                                    .map_err(|error| format!("Could not export preset: {error}"))?;
                                Ok("Preset exported.".into())
                            }));
                        }
                    }
                    if ui.button("Copy preset text").clicked() {
                        ui.close_menu();
                        picker.status = Some(encode(preset).map(|json| {
                            ui.ctx().copy_text(json);
                            "Preset text copied.".into()
                        }));
                    }
                });
            }
            if theme::small(ui, Look::Ghost, None, "Delete…", picker.selected.is_some()).clicked()
            {
                picker.confirm_delete = true;
                picker.show_paste = false;
            }
            if apply {
                if let Some(preset) = selected.cloned() {
                    let result = apply_to_profile(presets, launch, &project_key, preset);
                    if let Ok(name) = &result {
                        picker.select(Some(name.clone()));
                        picker.last_applied = Some(name.clone());
                        ui.ctx().request_repaint();
                    }
                    picker.status =
                        Some(result.map(|name| format!("Applied “{name}” to this profile.")));
                }
            }
        });

        ui.horizontal_wrapped(|ui| {
            if theme::small(ui, Look::Secondary, None, "Import file…", true)
                .on_hover_text("Choose a preset JSON file or a PowerShell launch script.")
                .clicked()
            {
                let mut dialog =
                    rfd::FileDialog::new().add_filter("Preset or launch script", &["json", "ps1"]);
                let scripts = crate::repo_dir(&project.repo).join("scripts");
                if scripts.is_dir() {
                    dialog = dialog.set_directory(scripts);
                }
                if let Some(path) = dialog.pick_file() {
                    picker.accept_import(presets, import_file(&path), compatible, &project_key);
                }
            }
            if !picker.show_paste
                && theme::small(ui, Look::Secondary, None, "Paste preset text…", true).clicked()
            {
                picker.show_paste = true;
                picker.confirm_delete = false;
                picker.status = None;
            }
        });

        if picker.show_paste {
            ui.add(
                egui::TextEdit::multiline(&mut picker.pasted)
                    .hint_text("Paste the JSON from Copy preset text")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal_wrapped(|ui| {
                if theme::small(
                    ui,
                    Look::Primary,
                    None,
                    "Import preset",
                    !picker.pasted.trim().is_empty(),
                )
                .clicked()
                {
                    let imported = decode(&picker.pasted);
                    if picker.accept_import(presets, imported, compatible, &project_key) {
                        picker.pasted.clear();
                        picker.show_paste = false;
                    }
                }
                if theme::small(ui, Look::Ghost, None, "Cancel", true).clicked() {
                    picker.show_paste = false;
                    picker.status = None;
                }
            });
        }

        if picker.confirm_delete {
            if let Some(name) = picker.selected.clone() {
                wrapped(ui, &format!("Delete “{name}” from your preset library? Projects keep the settings already applied."), theme::BLUSH);
                ui.horizontal_wrapped(|ui| {
                    if theme::small(ui, Look::Danger, None, "Delete preset", true).clicked() {
                        presets.retain(|preset| preset.name != name);
                        if launch.selected_preset.as_deref() == Some(&name) {
                            launch.selected_preset = None;
                            picker.last_applied = None;
                        }
                        picker.select(None);
                        picker.status = Some(Ok(format!("Deleted “{name}”.")));
                    }
                    if theme::small(ui, Look::Ghost, None, "Cancel", true).clicked() {
                        picker.confirm_delete = false;
                    }
                });
            }
        }
        show_status(ui, &picker.status);
    });
    ui.ctx().data_mut(|data| data.insert_temp(id, picker));
}

fn reset_to_global(ui: &mut Ui, value: &mut Option<String>) -> bool {
    if value.is_none() {
        note(ui, "Using global settings");
        false
    } else if theme::small(ui, Look::Ghost, None, "Use global settings", true).clicked() {
        *value = None;
        true
    } else {
        false
    }
}

fn show_status(ui: &mut Ui, status: &Option<Result<String, String>>) {
    match status {
        Some(Ok(message)) => wrapped(ui, message, theme::MUTED),
        Some(Err(error)) => wrapped(ui, error, theme::BAD),
        None => {}
    }
}

impl Picker {
    fn select(&mut self, name: Option<String>) {
        self.selected = name;
        self.confirm_delete = false;
    }

    fn preset<'a>(&self, presets: &'a [LaunchPreset]) -> Option<&'a LaunchPreset> {
        let name = self.selected.as_deref()?;
        presets.iter().find(|preset| preset.name == name)
    }

    fn accept_import(
        &mut self,
        presets: &mut Vec<LaunchPreset>,
        imported: Result<LaunchPreset, String>,
        compatible: impl FnOnce(&LaunchPreset) -> bool,
        project_key: &str,
    ) -> bool {
        self.status = Some(
            imported
                .and_then(|mut preset| {
                    preset.projects = vec![project_key.to_owned()];
                    preset.repo_script = None;
                    store(presets, preset)
                })
                .map(|index| {
                    if compatible(&presets[index]) {
                        self.select(Some(presets[index].name.clone()));
                    }
                    format!("Imported {} into your saved presets.", presets[index].name)
                }),
        );
        self.status.as_ref().is_some_and(Result::is_ok)
    }
}

pub(super) fn apply_to_profile(
    presets: &mut Vec<LaunchPreset>,
    launch: &mut ProjectLaunch,
    project_key: &str,
    mut preset: LaunchPreset,
) -> Result<String, String> {
    let existing = if preset.repo_script.is_some() {
        presets.iter().position(|saved| {
            saved.repo_script == preset.repo_script
                && saved.projects.iter().any(|key| key == project_key)
        })
    } else {
        presets.iter().position(|saved| saved.name == preset.name)
    };
    let index = match existing {
        Some(index) => index,
        None => {
            preset.projects = vec![project_key.to_owned()];
            store(presets, preset)?
        }
    };
    let saved = &mut presets[index];
    if !saved.projects.iter().any(|key| key == project_key) {
        saved.projects.push(project_key.to_owned());
    }
    saved.apply(launch);
    launch.selected_preset = Some(saved.name.clone());
    launch.preset_choice_confirmed = true;
    Ok(saved.name.clone())
}

fn store(presets: &mut Vec<LaunchPreset>, mut preset: LaunchPreset) -> Result<usize, String> {
    preset.name = preset.name.trim().to_owned();
    encode(&preset)?;
    if presets
        .iter()
        .any(|saved| saved.name.eq_ignore_ascii_case(&preset.name))
    {
        let base = preset.name.clone();
        for number in 2.. {
            let candidate = format!("{base} ({number})");
            if !presets
                .iter()
                .any(|saved| saved.name.eq_ignore_ascii_case(&candidate))
            {
                preset.name = candidate;
                break;
            }
        }
    }
    presets.push(preset);
    Ok(presets.len() - 1)
}

fn read_file(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|error| format!("Could not open preset: {error}"))?;
    let mut text = String::new();
    file.take((MAX_PRESET_BYTES + 1) as u64)
        .read_to_string(&mut text)
        .map_err(|error| format!("Could not read preset: {error}"))?;
    Ok(text)
}

pub(super) fn import_file(path: &Path) -> Result<LaunchPreset, String> {
    let text = read_file(path)?;
    if text.len() > MAX_PRESET_BYTES {
        return Err("Preset or script exceeds 128 KiB.".into());
    }
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
    {
        return decode(&text);
    }
    let (switches, options) = script_preset::parse(&text)?;
    Ok(LaunchPreset {
        name: path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Launch script")
            .into(),
        args: String::new(),
        switches: Some(switches),
        script_options: Some(options),
        projects: Vec::new(),
        repo_script: None,
    })
}

pub(super) fn show_script_options(
    ui: &mut Ui,
    switches: &mut String,
    options: script_preset::Options,
) {
    use script_preset::{enabled, set_enabled, HOOKS, PROBE_RAYS, RAY_TRACING};
    ui.horizontal_wrapped(|ui| {
        let mut ray_tracing = enabled(switches, RAY_TRACING);
        if options.ray_tracing && ui.checkbox(&mut ray_tracing, "Ray tracing").changed() {
            set_enabled(switches, RAY_TRACING, ray_tracing);
            if ray_tracing && options.probe_rays {
                set_enabled(switches, PROBE_RAYS, false);
            }
        }
        if options.probe_rays {
            let mut probe_rays = !ray_tracing && enabled(switches, PROBE_RAYS);
            if ui
                .add_enabled(
                    !ray_tracing,
                    egui::Checkbox::new(&mut probe_rays, "Probe rays"),
                )
                .on_hover_text("Available with ray tracing off.")
                .changed()
            {
                set_enabled(switches, PROBE_RAYS, probe_rays);
            }
        }
        if options.hooks {
            let mut hooks = enabled(switches, HOOKS);
            if ui.checkbox(&mut hooks, "Hooks").changed() {
                set_enabled(switches, HOOKS, hooks);
            }
        }
    });
}

fn decode(json: &str) -> Result<LaunchPreset, String> {
    if json.len() > MAX_PRESET_BYTES {
        return Err("Preset exceeds 128 KiB.".into());
    }
    serde_json::from_str(json.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("Invalid preset JSON: {error}"))
}

fn encode(preset: &LaunchPreset) -> Result<String, String> {
    if preset.name.trim().is_empty() || preset.name.chars().any(char::is_control) {
        return Err("Enter a preset name on one line.".into());
    }
    if preset.args.contains('\0')
        || preset
            .switches
            .as_deref()
            .is_some_and(|switches| switches.contains('\0'))
    {
        return Err("Launch settings contain a null character.".into());
    }
    let mut portable = preset.clone();
    portable.projects.clear();
    portable.repo_script = None;
    let json = serde_json::to_string_pretty(&portable).map_err(|error| error.to_string())?;
    if json.len() > MAX_PRESET_BYTES {
        return Err("Preset exceeds 128 KiB.".into());
    }
    Ok(json)
}

#[cfg(test)]
#[test]
fn shared_presets_preserve_quotes_empty_overrides_and_duplicate_names() {
    let original = LaunchPreset {
        name: "Scarlett".into(),
        args: r#"--strict -- -name "two words""#.into(),
        switches: Some(String::new()),
        script_options: None,
        projects: vec!["owner/private-repo".into()],
        repo_script: None,
    };
    let mut saved = vec![original.clone()];
    let imported = decode(&encode(&original).unwrap()).unwrap();
    let index = store(&mut saved, imported).unwrap();
    assert_eq!(saved[index].name, "Scarlett (2)");
    assert_eq!(saved[index].args, original.args);
    assert_eq!(saved[index].switches.as_deref(), Some(""));
    assert!(saved[index].projects.is_empty());
    assert!(decode("just raw arguments").is_err());
}

#[cfg(test)]
#[test]
fn deleting_another_preset_keeps_the_selected_switches() {
    let mut presets = Vec::new();
    for name in ["First", "Scarlett", "Third"] {
        store(
            &mut presets,
            LaunchPreset {
                name: name.into(),
                args: String::new(),
                switches: Some("VICTORY_ASYNC_SUBMIT=1\nVICTORY_AMBIENT=".into()),
                script_options: None,
                projects: Vec::new(),
                repo_script: None,
            },
        )
        .unwrap();
    }
    let mut picker = Picker::default();
    picker.select(Some("Scarlett".into()));
    presets.retain(|preset| preset.name != "First");
    let json = encode(picker.preset(&presets).unwrap()).unwrap();
    assert_eq!(
        decode(&json).unwrap().switches.as_deref(),
        Some("VICTORY_ASYNC_SUBMIT=1\nVICTORY_AMBIENT=")
    );
    presets.retain(|preset| preset.name != "Scarlett");
    assert!(picker.preset(&presets).is_none());
}

#[cfg(test)]
#[test]
fn accepting_a_detected_preset_saves_it_to_the_profile_without_duplicates() {
    let mut presets = Vec::new();
    let mut launch = ProjectLaunch::default();
    let preset = LaunchPreset {
        name: "play-scarlett".into(),
        args: String::new(),
        switches: Some("VICTORY_ASYNC_SUBMIT=1".into()),
        script_options: Some(script_preset::Options::default()),
        projects: Vec::new(),
        repo_script: Some("play-scarlett".into()),
    };
    for _ in 0..2 {
        apply_to_profile(&mut presets, &mut launch, "owner/scarlett", preset.clone()).unwrap();
    }
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].projects, ["owner/scarlett"]);
    assert_eq!(launch.selected_preset.as_deref(), Some("play-scarlett"));
    assert!(launch.has_confirmed_preset());
    assert_eq!(launch.switches, preset.switches);
    assert!(decode(&encode(&presets[0]).unwrap())
        .unwrap()
        .repo_script
        .is_none());
}
