use super::*;
use crate::Saved;

#[derive(Clone)]
struct Draft {
    original: Option<String>,
    preset: LaunchPreset,
}

#[cfg(test)]
#[test]
fn renaming_and_deleting_a_preset_preserves_applied_project_values() {
    let mut saved = Saved::default();
    let initial = snapshot(
        LaunchMethod::Victory,
        "First",
        &ProjectLaunch::default(),
        "--original",
        "",
        saved.default_graphics(),
    );
    let untouched = snapshot(
        LaunchMethod::Victory,
        "Second",
        &ProjectLaunch::default(),
        "--second",
        "",
        saved.default_graphics(),
    );
    store(&mut saved.launch_presets, untouched).unwrap();
    for repo in ["example/a", "example/b"] {
        let launch = saved.project_launch.entry(repo.into()).or_default();
        apply_to_profile(&mut saved.launch_presets, launch, repo, initial.clone()).unwrap();
    }
    let mut draft = Draft {
        original: Some("First".into()),
        preset: initial,
    };
    draft.preset.name = "second".into();
    assert!(save_draft(&mut saved, &draft).is_err());
    draft.preset.name = "Renamed".into();
    draft.preset.args = "--new".into();
    save_draft(&mut saved, &draft).unwrap();
    assert!(saved
        .project_launch
        .values()
        .all(|p| p.selected_preset.as_deref() == Some("Renamed")
            && p.args.as_deref() == Some("--original")));
    remove(&mut saved, "Second");
    assert_eq!(saved.launch_presets.len(), 1);
    remove(&mut saved, "Renamed");
    assert!(saved
        .project_launch
        .values()
        .all(|p| p.selected_preset.is_none() && p.args.as_deref() == Some("--original")));
}

#[derive(Clone, Default)]
struct Manager {
    draft: Option<Draft>,
    scroll_to_editor: bool,
    delete: Option<String>,
    paste: Option<String>,
    status: Option<Result<String, String>>,
}

pub(super) fn method_label(method: Option<LaunchMethod>) -> &'static str {
    match method {
        Some(LaunchMethod::Program) => "Standalone program",
        Some(LaunchMethod::Nvidia) => "NVIDIA native launcher",
        Some(LaunchMethod::Victory) => "VIctory runtime",
        None => "Shared launch arguments",
    }
}

fn save_draft(saved: &mut Saved, draft: &Draft) -> Result<String, String> {
    let mut preset = draft.preset.clone();
    preset.name = preset.name.trim().into();
    if preset.method == Some(LaunchMethod::Program) {
        preset.switches = None;
        preset.script_options = None;
    }
    if preset.method.is_some_and(|m| m != LaunchMethod::Nvidia) {
        preset.nvidia = None;
    }
    encode(&preset)?;
    if let Some(original) = &draft.original {
        let index = saved
            .launch_presets
            .iter()
            .position(|p| &p.name == original && p.repo_script.is_none())
            .ok_or("This preset no longer exists.")?;
        if saved
            .launch_presets
            .iter()
            .enumerate()
            .any(|(i, p)| i != index && p.name.eq_ignore_ascii_case(&preset.name))
        {
            return Err("That name is already in use. Choose another name.".into());
        }
        preset
            .projects
            .clone_from(&saved.launch_presets[index].projects);
        for profile in saved.project_launch.values_mut() {
            if profile.selected_preset.as_ref() == Some(original) {
                profile.selected_preset = Some(preset.name.clone());
            }
        }
        saved.launch_presets[index] = preset;
        Ok(saved.launch_presets[index].name.clone())
    } else {
        store(&mut saved.launch_presets, preset).map(|i| saved.launch_presets[i].name.clone())
    }
}

fn remove(saved: &mut Saved, name: &str) {
    saved
        .launch_presets
        .retain(|p| p.repo_script.is_some() || p.name != name);
    for profile in saved.project_launch.values_mut() {
        if profile.selected_preset.as_deref() == Some(name) {
            profile.selected_preset = None;
        }
    }
}

fn import(saved: &mut Saved, preset: Result<LaunchPreset, String>) -> Result<String, String> {
    let mut preset = preset?;
    preset.repo_script = None;
    preset.projects.clear();
    let i = store(&mut saved.launch_presets, preset)?;
    Ok(format!("Imported “{}”.", saved.launch_presets[i].name))
}

fn editor(ui: &mut Ui, draft: &mut Draft, graphics: crate::GraphicsSettings) {
    let preset = &mut draft.preset;
    ui.label("Preset name");
    theme::field(
        ui,
        &mut preset.name,
        "Name your preset",
        ui.available_width().min(360.0),
        None,
    );
    ui.label("Launch method");
    if draft.original.is_none() {
        let mut method = preset.method.unwrap_or(LaunchMethod::Victory);
        ui.horizontal_wrapped(|ui| {
            for option in [
                LaunchMethod::Victory,
                LaunchMethod::Nvidia,
                LaunchMethod::Program,
            ] {
                ui.selectable_value(&mut method, option, method_label(Some(option)));
            }
        });
        if Some(method) != preset.method {
            preset.method = Some(method);
            if method == LaunchMethod::Nvidia {
                preset.nvidia.get_or_insert(crate::NvidiaPreset {
                    graphics,
                    baseline: false,
                });
            }
            if method != LaunchMethod::Program {
                preset.switches.get_or_insert_with(String::new);
            }
        }
    } else {
        note(
            ui,
            method_label(
                preset
                    .method
                    .or(preset.nvidia.as_ref().map(|_| LaunchMethod::Nvidia)),
            ),
        );
    }
    if let Some(native) = preset
        .nvidia
        .as_mut()
        .filter(|_| preset.method.is_none_or(|m| m == LaunchMethod::Nvidia))
    {
        let mut profile = ProjectLaunch {
            args: Some(preset.args.clone()),
            switches: preset.switches.clone(),
            graphics: Some(native.graphics),
            nvidia_plain: native.baseline,
            ..Default::default()
        };
        super::super::native_settings::controls(ui, &mut profile, graphics, "", "");
        native.graphics = profile.graphics.unwrap_or(graphics);
        native.baseline = profile.nvidia_plain;
        preset.args = profile.args.unwrap_or_default();
        preset.switches = profile.switches;
    }
    ui.label("Launch arguments");
    theme::field(
        ui,
        &mut preset.args,
        "Enter launch arguments",
        ui.available_width(),
        None,
    );
    if let Some(switches) = preset
        .switches
        .as_mut()
        .filter(|_| preset.method != Some(LaunchMethod::Program))
    {
        ui.label("Launch switches");
        if let Some(options) = preset.script_options {
            show_script_options(ui, switches, options);
        }
        ui.add(
            egui::TextEdit::multiline(switches)
                .font(theme::mono(12.5))
                .hint_text("One NAME=value per line")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
    }
}

pub(in crate::ui) fn show(ui: &mut Ui, saved: &mut Saved) {
    let id = egui::Id::new("personal-preset-manager");
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<Manager>(id))
        .unwrap_or_default();
    egui::ScrollArea::vertical().id_salt("manager-page").show(ui, |ui| {
        section(ui, "Presets", None, "", |ui| {
            ui.horizontal_wrapped(|ui| {
                if theme::small(ui, Look::Primary, None, "New preset", state.draft.is_none()).clicked() {
                    state.draft = Some(Draft { original: None, preset: snapshot(LaunchMethod::Victory, "",
                        &ProjectLaunch::default(), "", "", saved.default_graphics()) });
                    state.status = None;
                }
                if theme::small(ui, Look::Secondary, None, "Import file…", true).clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Preset or PowerShell script", &["json","ps1"]).pick_file() {
                        state.status = Some(import(saved, import_file(&path)));
                    }
                }
                if theme::small(ui, Look::Ghost, None, "Paste preset text", state.paste.is_none()).clicked() { state.paste = Some(String::new()); }
            });
            if let Some(paste) = &mut state.paste {
                ui.add(egui::TextEdit::multiline(paste).hint_text("Paste shared preset JSON").desired_width(f32::INFINITY));
                let mut close = false;
                ui.horizontal(|ui| {
                    if ui.button("Import preset").clicked() {
                        let result = import(saved, decode(paste)); close = result.is_ok(); state.status = Some(result);
                    }
                    close |= ui.button("Cancel paste").clicked();
                });
                if close { state.paste = None; }
            }
            show_status(ui, &state.status);
        });
        ui.add_space(8.0);
        if let Some(draft) = &mut state.draft {
            let mut close = false;
            if std::mem::take(&mut state.scroll_to_editor) { ui.scroll_to_cursor(Some(egui::Align::TOP)); }
            section(ui, if draft.original.is_some() { "Edit preset" } else { "Create preset" }, None, "", |ui| {
                editor(ui, draft, saved.default_graphics());
                ui.horizontal_wrapped(|ui| {
                    if theme::small(ui, Look::Primary, None, "Save preset", !draft.preset.name.trim().is_empty()).clicked() {
                        let result = save_draft(saved, draft);
                        close = result.is_ok(); state.status = Some(result.map(|name| format!("Saved “{name}”.")));
                    }
                    close |= theme::small(ui, Look::Ghost, None, "Cancel", true).clicked();
                });
            });
            if close { state.draft = None; }
            ui.add_space(8.0);
        }
        ui.vertical(|ui| {
            if !saved.launch_presets.iter().any(|p| p.repo_script.is_none()) { note(ui, "Your first saved preset will appear here."); }
            for preset in saved.launch_presets.iter().filter(|p| p.repo_script.is_none()) {
                preset_card(ui, preset, false, |ui| ui.horizontal_wrapped(|ui| {
                    if theme::small(ui, Look::Secondary, None, "Edit", state.draft.is_none()).clicked() {
                        state.draft = Some(Draft { original:Some(preset.name.clone()), preset:preset.clone() });
                        state.status = None;
                        state.scroll_to_editor = true;
                    }
                    ui.menu_button("Share", |ui| {
                        if ui.button("Copy preset text").clicked() {
                            state.status = Some(encode(preset).map(|json| { ui.ctx().copy_text(json); "Copied preset.".into() })); ui.close_menu();
                        }
                        if ui.button("Export JSON file…").clicked() {
                            ui.close_menu();
                            if let Some(path) = rfd::FileDialog::new().add_filter("Launch preset",&["json"]).set_file_name("launch-preset.json").save_file() {
                                state.status = Some(encode(preset).and_then(|json| std::fs::write(path,json).map(|_|"Exported preset.".into()).map_err(|e|e.to_string())));
                            }
                        }
                    });
                    if theme::small(ui, Look::Ghost, None, "Delete…", state.draft.is_none()).clicked() { state.delete = Some(preset.name.clone()); }
                }));
                ui.add_space(6.0);
            }
            if let Some(name) = state.delete.clone() {
                wrapped(ui, &format!("Delete “{name}”? Projects keep their current settings."), theme::blush());
                ui.horizontal_wrapped(|ui| {
                    if theme::small(ui, Look::Danger, None, "Delete preset", true).clicked() {
                        remove(saved, &name); state.delete = None; state.status = Some(Ok(format!("Deleted “{name}”.")));
                    }
                    if ui.button("Keep preset").clicked() { state.delete = None; }
                });
            }
        });
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, state));
}
