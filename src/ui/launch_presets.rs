use super::{note, section, wrapped};
use crate::script_preset;
use crate::theme::{self, Look};
use crate::{LaunchMethod, LaunchPreset, Project, ProjectLaunch};
use eframe::egui::{self, Ui};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub(super) mod manager;

const MAX_PRESET_BYTES: usize = 128 * 1024;

#[derive(Clone, Default)]
struct Picker {
    detected: Vec<LaunchPreset>,
    name: String,
    status: Option<Result<String, String>>,
}

pub(super) fn show(
    ui: &mut Ui,
    presets: &mut Vec<LaunchPreset>,
    launch: &mut ProjectLaunch,
    global_args: &str,
    global_switches: &str,
    project: &Project,
    revision: Option<&str>,
    can_rescan: bool,
    native_graphics: Option<crate::GraphicsSettings>,
    global_graphics: crate::GraphicsSettings,
) -> bool {
    let standalone = project.runs_standalone();
    let project_key = crate::project_key(&project.repo);
    let id = ui.make_persistent_id(("launch-preset-picker", revision));
    let mut open_manager = false;
    let mut picker = ui
        .ctx()
        .data(|data| data.get_temp::<Picker>(id))
        .unwrap_or_else(|| Picker {
            detected: if standalone { vec![] } else { super::play_preset_picker::detected_scripts(project) },
            ..Picker::default()
        });
    let method = LaunchMethod::for_project(project, native_graphics.is_some());
    let compatible = |preset: &LaunchPreset| method.accepts(preset) && preset.repo_script.is_none();
    for detected in &picker.detected {
        restore_probe_control(presets, launch, &project_key, detected);
    }
    let inherited_args = if standalone { "" } else { global_args };
    let advanced = |ui: &mut Ui, launch: &mut ProjectLaunch| {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Launch arguments");
            if !standalone {
                reset_to_global(ui, &mut launch.args);
            }
        });
        let mut args = launch.args.as_deref().unwrap_or(inherited_args).to_owned();
        let hint = if standalone {
            "Paste program arguments"
        } else if native_graphics.is_some() {
            "Additional native options"
        } else {
            "Paste launch arguments, e.g. --strict"
        };
        if theme::field(ui, &mut args, hint, ui.available_width(), None).changed() {
            launch.args = Some(args);
        }
        if !standalone {
            if native_graphics.is_none() {note(ui, "Put game arguments after --.");}
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

    };
    let current = presets.iter().find(|p| launch.selected_preset.as_ref() == Some(&p.name));
    let active_script = current.filter(|p| p.projects.contains(&project_key)).and_then(|p| p.repo_script.clone());
    let title = setup_name(current, launch, inherited_args, global_switches, global_graphics);
    theme::card(ui, 16.0, |ui| {
        ui.horizontal_wrapped(|ui| {
            theme::icon(ui, theme::glyph::PLAY, 19.0, theme::pink());
            theme::caps(ui, "Next launch", 11.0, theme::pink());
            theme::badge(ui, manager::method_label(Some(method)), theme::lilac());
        });
        ui.label(egui::RichText::new(&title).font(theme::strong(23.0)).color(theme::white()));
        ui.horizontal_wrapped(|ui| {
            let field = theme::field(ui, &mut picker.name, "Name your preset", 195.0, None);
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let named = !picker.name.trim().is_empty();
            if theme::small(ui, Look::Primary, None, "Save profile as preset", named).clicked() || (enter && named) {
                let mut preset = snapshot(method, picker.name.trim(), launch, inherited_args, global_switches, global_graphics);
                if let Some(source) = presets.iter().find(|p| Some(&p.name) == launch.selected_preset.as_ref() && p.projects.contains(&project_key))
                    .and_then(|p| p.repo_script.as_ref()).and_then(|source| picker.detected.iter().find(|p| p.repo_script.as_ref()==Some(source))) {
                    if let (Some(old), Some(new)) = (preset.script_options.as_mut(), source.script_options) { old.shader_pass = new.shader_pass; }
                }
                picker.status = Some(store(presets, preset).map(|i| {
                    picker.name.clear();
                    format!("Saved “{}” to My presets.", presets[i].name)
                }));
            }
        });
    });
    ui.add_space(8.0);
    let mut chosen_script = None;
    let mut copy_script = None;
    let mut chosen_preset = None;
    let mut sources = |ui: &mut Ui| {
        theme::card(ui, 14.0, |ui| {
            ui.horizontal_wrapped(|ui| {
                theme::icon(ui, theme::glyph::FOLDER, 16.0, theme::lilac());
                ui.label(egui::RichText::new("Project sources").font(theme::strong(18.0)));
                if method == LaunchMethod::Victory && theme::small(ui, Look::Ghost, Some(theme::glyph::SYNC), "Rescan", can_rescan).clicked() {
                    picker.detected = super::play_preset_picker::detected_scripts(project);
                    picker.status = Some(Ok(format!("Found {} supported launch scripts.", picker.detected.len())));
                }
            });
            match method {
                LaunchMethod::Program => note(ui, "Play starts the project’s executable."),
                LaunchMethod::Nvidia => { theme::badge(ui, "PLAY.cmd", theme::good()); note(ui, "Native launch options are available below."); }
                LaunchMethod::Victory if picker.detected.is_empty() => note(ui, "No supported script found. Enter launch arguments below."),
                LaunchMethod::Victory => for script in &picker.detected {
                    let active = active_script == script.repo_script;
                    ui.push_id(&script.name, |ui| {
                        egui::Frame::none().fill(theme::deep()).rounding(8.0).inner_margin(10.0).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new(format!("{}.ps1",script.name)).font(theme::strong(14.0)));
                                if active { theme::badge(ui,"In use",theme::good()); }
                                note(ui, &format!("{} switches",script.switches.as_deref().unwrap_or("").lines().count()));
                            });
                            if !active {
                                ui.horizontal_wrapped(|ui| {
                                    if theme::small(ui, Look::Secondary, None, "Use script settings", true).clicked() { chosen_script = Some(script.clone()); }
                                    if theme::small(ui, Look::Ghost, Some(theme::glyph::COPY), "Save a copy", true).clicked() { copy_script = Some(script.clone()); }
                                });
                            }
                        });
                    });
                }
            }
        });
    };
    let mut shelf = |ui: &mut Ui| {
        theme::card(ui, 14.0, |ui| {
            ui.horizontal_wrapped(|ui| {
                theme::icon(ui, theme::glyph::SETTINGS, 16.0, theme::pink());
                ui.label(egui::RichText::new("My presets").font(theme::strong(18.0)));
                if theme::small(ui, Look::Ghost, Some(theme::glyph::OPEN), "Manage", true).clicked() { open_manager = true; }
            });
            if !presets.iter().any(compatible) { note(ui, "Save this profile to add your first compatible preset."); }
            for p in presets.iter().filter(|p| compatible(p)) {
                let using = launch.selected_preset.as_ref() == Some(&p.name) && p.matches_profile(launch, inherited_args, global_switches, global_graphics);
                preset_card(ui, p, using, |ui| {
                    if theme::small(ui, Look::Primary, None, "Use for this project", !using).clicked() { chosen_preset = Some(p.clone()); }
                });
            }
        });
    };
    if ui.available_width() >= 740.0 {
        ui.columns(2, |columns| { sources(&mut columns[0]); shelf(&mut columns[1]); });
    } else { sources(ui); ui.add_space(6.0); shelf(ui); }
    if let Some(mut script) = copy_script {
        script.repo_script = None;
        script.projects.clear();
        picker.status = Some(store(presets, script).map(|i|format!("Saved “{}” to My presets.",presets[i].name)));
    }
    if let Some(script) = chosen_script {
        picker.status = Some(apply_to_profile(presets, launch, &project_key, script).map(|_| {
            launch.reviewed_script_revision = revision.map(str::to_owned);
            "Script settings applied to this project.".into()
        }));
    }
    if let Some(preset) = chosen_preset {
        picker.status = Some(apply_to_profile(presets, launch, &project_key, preset).map(|name| format!("Using “{name}”.")));
    }
    show_status(ui,&picker.status);
    ui.add_space(8.0);
    section(ui, "Launch options", None, "", |ui| {
        if let Some(graphics) = native_graphics {
            super::native_settings::controls(ui, launch, graphics, global_args, global_switches);
        } else if let Some(options) = launch.script_options {
            let mut switches = launch.switches.as_deref().unwrap_or(global_switches).to_owned();
            show_script_options(ui, &mut switches, options);
            if switches != launch.switches.as_deref().unwrap_or(global_switches) { launch.switches = Some(switches); }
        }
        advanced(ui, launch);
    });
    ui.ctx().data_mut(|data| data.insert_temp(id, picker));
    open_manager
}

/// Both the project and the workspace use the same read-only preview.
fn preset_card<R>(ui: &mut Ui, preset: &LaunchPreset, active: bool, actions: impl FnOnce(&mut Ui) -> R) {
    ui.push_id(("preset-card", &preset.name), |ui| {
        theme::card(ui, 10.0, |ui| {
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(), ui.make_persistent_id("expanded"), false,
            );
            let (rect, hit) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::click());
            let hit = hit.on_hover_cursor(egui::CursorIcon::PointingHand);
            if hit.clicked() { state.toggle(ui); }
            let accent = if active { theme::good() } else { theme::lilac() };
            let painter = ui.painter();
            if hit.hovered() || hit.has_focus() { painter.rect_filled(rect, 7.0, theme::raised()); }
            let tile = egui::Rect::from_center_size(egui::pos2(rect.left()+23.0, rect.center().y), egui::vec2(36.0,36.0));
            painter.rect_filled(tile, 8.0, accent.gamma_multiply(0.18));
            let center = tile.center();
            for (y, x) in [(-5.0,-3.0),(0.0,4.0),(5.0,-1.0)] {
                painter.hline(center.x-8.0..=center.x+8.0,center.y+y,egui::Stroke::new(1.4_f32,accent));
                painter.circle_filled(center+egui::vec2(x,y),2.1,accent);
            }
            let left = rect.left()+52.0;
            let width = rect.right()-left-32.0;
            let title = theme::one_line(ui, &preset.name, theme::strong(16.0), theme::white(), width);
            painter.galley(egui::pos2(left,rect.top()+4.0),title,theme::white());
            let method = manager::method_label(preset.method.or(preset.nvidia.as_ref().map(|_| LaunchMethod::Nvidia)));
            let caption = if active { format!("{method} · In use") } else { method.to_string() };
            let subtitle = theme::one_line(ui,&caption,theme::regular(12.0),theme::muted(),width);
            painter.galley(egui::pos2(left,rect.top()+27.0), subtitle, theme::muted());
            let arrow = egui::pos2(rect.right()-14.0,rect.center().y);
            let dy = if state.is_open() { -2.5 } else { 2.5 };
            painter.add(egui::Shape::line(vec![arrow+egui::vec2(-5.0,-dy),arrow+egui::vec2(0.0,dy),arrow+egui::vec2(5.0,-dy)], egui::Stroke::new(1.7_f32,accent)));
            hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button,true,format!("{} settings",preset.name)));
            state.show_body_unindented(ui, |ui| {
                ui.separator();
                if let Some(native) = &preset.nvidia {
                    let g = native.graphics;
                    ui.horizontal_wrapped(|ui| {
                        for value in [if g.series_s { "Series S" } else { "Series X" }, g.output.label(),
                            if native.baseline { "Baseline" } else { "Recommended optimizations" }] {
                            theme::badge(ui, value, accent);
                        }
                    });
                    note(ui,&format!("Render scale {:.0}% · {} · 8 GB fix {} · Bright shadows {}",g.render_scale*100.0,
                        if g.fullscreen {"Fullscreen"} else {"Windowed"}, if g.fix_8gb {"on"} else {"off"}, if g.bright_shadows {"on"} else {"off"}));
                }
                theme::caps(ui,"Arguments",10.0,theme::muted());
                ui.add(egui::Label::new(egui::RichText::new(if preset.args.is_empty() { "None" } else { &preset.args }).font(theme::mono(12.5))).wrap());
                if preset.method != Some(LaunchMethod::Program) {
                    theme::caps(ui,"Switches",10.0,theme::muted());
                    let switches = match preset.switches.as_deref() { None => "Keep project switches", Some("") => "None", Some(value) => value };
                    egui::ScrollArea::vertical().id_salt("switch-preview").max_height(160.0).show(ui,|ui| {
                        ui.add(egui::Label::new(egui::RichText::new(switches).font(theme::mono(12.5))).wrap());
                    });
                }
                if preset.script_options.is_some_and(|opts| opts.shader_pass) { note(ui, "Script shader warm-up enabled"); }
                ui.add_space(4.0);
                actions(ui);
            });
        });
    });
}

pub(super) fn setup_name(current: Option<&LaunchPreset>, profile: &ProjectLaunch, args: &str, switches: &str, graphics: crate::GraphicsSettings) -> String {
    let Some(preset) = current else { return "Project settings".into(); };
    let mut name = preset.repo_script.as_ref().map_or_else(|| preset.name.clone(), |script| format!("Settings from {script}.ps1"));
    if !preset.matches_profile(profile, args, switches, graphics) { name.push_str(" · edited"); }
    name
}

fn snapshot(method: LaunchMethod, name: &str, profile: &ProjectLaunch, args: &str, switches: &str, graphics: crate::GraphicsSettings) -> LaunchPreset {
    LaunchPreset {
        method: Some(method),
        name: name.into(),
        args: profile.args.as_deref().unwrap_or(args).into(),
        switches: (method != LaunchMethod::Program).then(|| profile.switches.as_deref().unwrap_or(switches).into()),
        script_options: if method == LaunchMethod::Program { None } else { profile.script_options },
        nvidia: (method == LaunchMethod::Nvidia).then(|| crate::NvidiaPreset {
            graphics: profile.graphics.unwrap_or(graphics), baseline: profile.nvidia_plain,
        }),
        projects: Vec::new(), repo_script: None,
    }
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
        Some(Ok(message)) => wrapped(ui, message, theme::muted()),
        Some(Err(error)) => wrapped(ui, error, theme::bad()),
        None => {}
    }
}

pub(super) fn apply_to_profile(
    presets: &mut Vec<LaunchPreset>,
    launch: &mut ProjectLaunch,
    project_key: &str,
    mut preset: LaunchPreset,
) -> Result<String, String> {
    if preset.nvidia.is_some() && !launch.nvidia_defaults_initialized {
        return Err("This preset needs a project with the NVIDIA native launcher.".into());
    }
    let existing = if preset.repo_script.is_some() {
        presets.iter().position(|saved| {
            saved.repo_script == preset.repo_script
                && saved.projects.iter().any(|key| key == project_key)
        })
    } else {
        presets.iter().position(|saved| saved.name == preset.name)
    };
    // A fresh scan supplies the current file, even when the library already has its name.
    // Re-selecting the same script carries the player's changes onto the new defaults.
    let mut keep_profile_edits = false;
    if let Some(saved) = existing.map(|i| &mut presets[i]) {
        if preset.repo_script.is_some() && preset.projects.is_empty() {
            keep_profile_edits = launch.selected_preset.as_ref() == Some(&saved.name) && launch.switches.is_some();
            if keep_profile_edits {
                if let (Some(old), Some(new), Some(local)) = (&saved.switches, &preset.switches, &mut launch.switches) {
                    if old != new { *local = script_preset::refresh_defaults(old, new, local); }
                }
                if launch.switches.is_some() { launch.script_options = preset.script_options; }
            }
            saved.switches = preset.switches.clone();
            saved.args.clone_from(&preset.args);
            saved.script_options = preset.script_options;
        }
    }
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
    if !keep_profile_edits { saved.apply(launch); }
    launch.selected_preset = Some(saved.name.clone());
    launch.preset_choice_confirmed = true;
    launch.confirmed_repo_script = None;
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
        method: Some(LaunchMethod::Victory),
        nvidia: None,
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
    use script_preset::{enabled, set_enabled, HOOKS, RAY_TRACING};
    let probe_keys = options.probe_keys();
    ui.horizontal_wrapped(|ui| {
        let mut ray_tracing = enabled(switches, RAY_TRACING);
        if options.ray_tracing && ui.checkbox(&mut ray_tracing, "Ray tracing").changed() {
            set_enabled(switches, RAY_TRACING, ray_tracing);
            if ray_tracing && options.probe_rays {
                set_enabled(switches, probe_keys, false);
            }
        }
        if options.probe_rays {
            let mut probe_rays = !ray_tracing && enabled(switches, probe_keys);
            if ui
                .add_enabled(
                    !ray_tracing,
                    egui::Checkbox::new(&mut probe_rays, "Probe rays"),
                )
                .on_hover_text("Available with ray tracing off.")
                .changed()
            {
                set_enabled(switches, probe_keys, probe_rays);
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

// v0.2.3 imported newer Scarlett scripts without a Probe rays control.
// Restore the control only for the matching repo preset and keep the user's launch values.
fn restore_probe_control(
    presets: &mut [LaunchPreset],
    launch: &mut ProjectLaunch,
    project_key: &str,
    detected: &LaunchPreset,
) {
    if !detected
        .script_options
        .is_some_and(|options| options.probe_rays_single_switch)
    {
        return;
    }
    let Some(saved) = presets.iter_mut().find(|preset| {
        preset.repo_script == detected.repo_script
            && preset.projects.iter().any(|key| key == project_key)
    }) else {
        return;
    };
    for options in saved.script_options.iter_mut().chain(
        launch
            .script_options
            .iter_mut()
            .filter(|_| launch.selected_preset.as_deref() == Some(&saved.name)),
    ) {
        if !options.probe_rays {
            options.probe_rays = true;
            options.probe_rays_single_switch = true;
        }
    }
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
fn native_preset_export_restores_graphics_tuning_and_native_arguments() {
    let graphics = crate::Saved::default().default_graphics();
    let preset = LaunchPreset {
        method: None,
        nvidia: Some(crate::NvidiaPreset{graphics,baseline:true}),
        name:"Quiet evening".into(),args:"--dlss balanced --fps 60".into(),
        switches:Some("VICTORY_REFLEX=boost".into()),projects:vec!["example/one".into()],
        repo_script:None,script_options:None,
    };
    let json=encode(&preset).unwrap();
    let mut saved=Vec::new();
    let mut profile=ProjectLaunch::default();
    assert!(apply_to_profile(&mut saved,&mut profile,"another/project",decode(&json).unwrap()).is_err());
    profile.nvidia_defaults_initialized=true;
    apply_to_profile(&mut saved,&mut profile,"another/project",decode(&json).unwrap()).unwrap();
    assert!(profile.graphics==Some(graphics) && profile.nvidia_plain);
    assert_eq!(profile.args.as_deref(),Some("--dlss balanced --fps 60"));
    assert_eq!(profile.switches.as_deref(),Some("VICTORY_REFLEX=boost"));
}

#[cfg(test)]
#[test]
fn named_script_copy_keeps_warmup_and_only_appears_for_compatible_projects() {
    let mut saved = crate::Saved::default();
    let graphics = saved.default_graphics();
    let (_, options) = script_preset::parse("$set = @{\nVICTORY_LOG = 'info'\n}\n& $Victory shaders $rpf").unwrap();
    let profile = saved.project_launch.entry("example/test".into()).or_default();
    profile.switches = Some("VICTORY_LOG=info".into());
    profile.script_options = Some(options);
    let copy = snapshot(LaunchMethod::Victory, "Warm setup", profile, "", "", graphics);
    let portable = decode(&encode(&copy).unwrap()).unwrap();
    assert!(LaunchMethod::Victory.accepts(&portable));
    assert!(!LaunchMethod::Nvidia.accepts(&portable) && !LaunchMethod::Program.accepts(&portable));
    apply_to_profile(&mut saved.launch_presets, profile, "example/test", portable).unwrap();
    assert!(saved.launch("example/test", false).scarlett_shader_pass);
    let old: LaunchPreset = decode(r#"{"name":"Classic","args":"--old","switches":null}"#).unwrap();
    assert!(LaunchMethod::Program.accepts(&old));
}

#[cfg(test)]
#[test]
fn shared_presets_preserve_quotes_empty_overrides_and_duplicate_names() {
    let original = LaunchPreset {
        method: None,
        nvidia: None,
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
fn accepting_a_detected_preset_saves_it_to_the_profile_without_duplicates() {
    let mut presets = Vec::new();
    let mut launch = ProjectLaunch::default();
    let preset = LaunchPreset {
        method: None,
        nvidia: None,
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

#[cfg(test)]
#[test]
fn restoring_probe_control_keeps_existing_profile_switches() {
    let (switches, options) = script_preset::parse(
        "$set = @{\nVICTORY_LOG = 'warn'\n}\n\
if (-not $NoProbeRays -and -not $RayTracing) { $set.VICTORY_PROBE_RAYS = '1' }",
    )
    .unwrap();
    let detected = LaunchPreset {
        method: None,
        nvidia: None,
        name: "play-scarlett".into(),
        args: String::new(),
        switches: Some(switches),
        script_options: Some(options),
        projects: vec!["owner/scarlett".into()],
        repo_script: Some("play-scarlett".into()),
    };
    let old_switches = Some("VICTORY_LOG=info".to_owned());
    let old_options = Some(script_preset::Options::default());
    let mut saved = vec![LaunchPreset {
        switches: old_switches.clone(),
        script_options: old_options,
        ..detected.clone()
    }];
    let mut launch = ProjectLaunch {
        selected_preset: Some(detected.name.clone()),
        script_options: old_options,
        switches: old_switches.clone(),
        ..ProjectLaunch::default()
    };
    restore_probe_control(&mut saved, &mut launch, "owner/scarlett", &detected);
    assert_eq!(
        launch.script_options.unwrap().probe_keys(),
        ["VICTORY_PROBE_RAYS"]
    );
    assert!(launch.script_options.unwrap().probe_rays);
    assert_eq!(launch.switches, old_switches);
    assert_eq!(saved[0].switches, old_switches);
}
