use super::*;
use crate::{RepoState, Saved};
use egui::{Event, PointerButton, Pos2, RawInput};
use std::collections::BTreeMap;

// Exercise the production widgets with pointer events. No game process or network job is started.
struct Harness {
    app: App,
    ctx: egui::Context,
    time: f64,
    labels: BTreeMap<String, Vec<Pos2>>,
    size: egui::Vec2,
    copied: String,
    opened: Option<String>,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        theme::apply(&ctx);
        let project = Project {
            name: "Ocean Drive".into(),
            repo: "example/ocean".into(),
            custom: true,
            branch: None,
            gpu: vec![],
            fps: None,
            render: None,
            verified: false,
            summary: String::new(),
            local_exe: None,
            archive_exe: None,
        };
        let legacy = serde_json::json!({"custom":[project],"game_dir":"X:/Retail","defaults_set":true,"series_s":true,
            "render_scale":0.75,"warm_before_play":true,"launch_args":"--old","project_launch":{"example/ocean":{"notes":"Existing notes"}}});
        let saved: Saved = serde_json::from_value(legacy).unwrap();
        let mut app = App::from_saved(saved, slides::Slides::default());
        app.files = vec![("fixture", true)];
        app.repos.lock().unwrap().insert(
            "example/ocean".into(),
            RepoState {
                installed: true,
                ..Default::default()
            },
        );
        *app.projects.lock().unwrap() = Some(Ok(vec![]));
        Self {
            app,
            ctx,
            time: 1.0,
            labels: BTreeMap::new(),
            size: vec2(1440.0, 900.0),
            copied: String::new(),
            opened: None,
        }
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.2;
        let input = RawInput {
            time: Some(self.time),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            ..Default::default()
        };
        let output = self.ctx.run(input, |ctx| {
            self.app.draw_workspace(ctx);
            self.app.dialogs(ctx);
        });
        self.labels.clear();
        if !output.platform_output.copied_text.is_empty() {
            self.copied = output.platform_output.copied_text;
        }
        if let Some(link) = output.platform_output.open_url {
            self.opened = Some(link.url);
        }
        for item in output.shapes {
            if let egui::Shape::Text(text) = item.shape {
                let center = text.pos + text.galley.size() * 0.5;
                if item.clip_rect.contains(center) {
                    self.labels
                        .entry(text.galley.text().to_string())
                        .or_default()
                        .push(center);
                }
            }
        }
    }
    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }
    fn at(&self, label: &str) -> Pos2 {
        *self
            .labels
            .get(label)
            .unwrap_or_else(|| panic!("Missing {label:?}: {:?}", self.labels.keys()))
            .last()
            .unwrap()
    }
    fn click_at(&mut self, pos: Pos2) {
        self.frame(vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }]);
        }
        self.settle();
    }
    fn click(&mut self, label: &str) {
        let pos = self.at(label);
        self.click_at(pos);
    }
}
/// Runs inside the isolated local-Git integration fixture.
pub(crate) fn click_library_build(project: &Project, action: &str) {
    let mut h = Harness::new();
    h.app.saved.custom = vec![project.clone()];
    h.app.tools = Some(tools::detect());
    let state = crate::repo_state(project);
    h.app
        .repos
        .lock()
        .unwrap()
        .insert(project.repo.clone(), state);
    h.settle();
    if action == "Update" {
        h.click(&project.name);
        h.click("Installation");
        assert!(h.at("Update now").y < h.at("Installation").y);
        h.click("Library");
    }
    h.click(action);
    assert!(h.app.view == View::Library);
    let job = h.app.project_jobs.get(&project.repo).unwrap().clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while job.lock().unwrap().running {
        assert!(
            std::time::Instant::now() < deadline,
            "Timed out building the fixture"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        !job.lock().unwrap().failed,
        "{}",
        job.lock().unwrap().log.preview()
    );
    h.settle();
    assert!(!h.labels.contains_key(action));
}

#[test]
fn queued_setup_keeps_choices_separate_and_cancel_preserves_profile() {
    let mut h = Harness::new();
    let root = std::env::temp_dir().join(format!("palmvi-setup-ui-{}",std::process::id()));
    std::fs::create_dir_all(root.join("apps/victory/src")).unwrap();
    for f in ["Get-NVIDIA-DLSS.cmd","PLAY.cmd","apps/victory/src/play.rs"] {
        std::fs::write(root.join(f), []).unwrap();
    }
    let mut p=h.app.saved.custom[0].clone();
    p.local_exe=Some(root.join("victory.exe").display().to_string());
    let second=Project {repo:"example/second".into(),name:"Second fixture".into(),..p.clone()};
    h.app.saved.custom=vec![p.clone(),second.clone()];
    for project in [&p,&second] {h.app.saved.queue_project_setup(project);}
    h.settle();
    let first=h.app.native_setup.as_ref().unwrap().project.repo.clone();
    h.click("Recommended optimizations");
    h.click("Baseline");
    assert!(h.app.native_setup.as_ref().unwrap().draft.nvidia_plain);
    h.click("Save choices");
    assert!(h.app.saved.project_launch[&first].nvidia_plain);
    let next=h.app.native_setup.as_ref().unwrap().project.repo.clone();
    assert_ne!(first,next);
    h.app.native_setup.as_mut().unwrap().draft.nvidia_plain=true;
    h.click("Decide later");
    assert!(!h.app.saved.project_launch[&next].nvidia_plain);
    assert!(h.app.native_setup.is_none());
    assert!(h.app.saved.project_launch.values().all(|l|!l.pending_setup));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn library_profile_quick_choices_and_legacy_settings_round_trip() {
    let mut h = Harness::new();
    h.settle();
    h.click("☆");
    assert_eq!(h.app.saved.pinned, ["example/ocean"]);
    h.click("Ocean Drive");
    assert!(h.app.view == View::Projects);
    let repo = "example/ocean";
    let url = h.app.selected_project().unwrap().web_url();
    let copy = if theme::has_icons() {
        h.at(&glyph::COPY.to_string())
    } else {
        h.at("Copy repo link")
    };
    let link = pos2(
        copy.x - if theme::has_icons() { 38.0 } else { 83.0 },
        copy.y,
    );
    assert!(!h.labels.contains_key(&url));
    h.frame(vec![Event::PointerMoved(link)]);
    h.settle();
    assert!(h.labels.contains_key(&url));
    let fps_label = h.at("AVERAGE FPS");
    assert!(h.at(&url).x < copy.x && copy.x - fps_label.x < 204.0);
    assert!(copy.y < fps_label.y);
    h.click_at(link);
    assert_eq!(h.opened.as_deref(), Some(url.as_str()));
    h.click_at(copy);
    assert_eq!(h.copied, url);
    assert!(!h.labels.contains_key(&url));
    // The arrow edits in place. The body goes to the dedicated pane.
    let graphics = h.at("Graphics  ›");
    let preset = h.at("Next launch  ›");
    let arrow = pos2(preset.x - 89.0, graphics.y);
    h.click_at(arrow);
    h.click("Fast");
    assert_eq!(h.app.saved.graphics_for(repo).render_scale, 0.5);
    assert!(h.labels.contains_key("Open save picker"));
    h.click("Graphics  ›");
    assert!(h.labels.contains_key("Output resolution"));
    h.click("Use global settings");
    assert_eq!(h.app.saved.graphics_for(repo).render_scale, 0.75);
    h.click("Notes");
    assert_eq!(h.app.saved.project_launch[repo].notes, "Existing notes");
    h.click("Overview");
    h.click("Open save picker");
    assert!(h.app.save_picker.as_ref().unwrap().settings_only);
    h.click("Use this save");
    assert!(h.app.save_picker.is_none() && !h.app.project_jobs.running());
    h.click("Play project");
    assert!(h.app.save_picker.as_ref().is_some_and(|p| !p.settings_only));
    h.click("Cancel");
    let saved: Saved = serde_json::from_value(serde_json::to_value(&h.app.saved).unwrap()).unwrap();
    assert_eq!(saved.game_dir, "X:/Retail");
    assert_eq!(saved.launch_args, "--old");
    assert!(saved.warm_before_play && saved.save_choices.contains_key(repo));
    assert_eq!(saved.pinned, ["example/ocean"]);
}

#[test]
fn save_edit_and_restore_a_named_setup_from_the_launch_page() {
    let mut h = Harness::new();
    let key = "example/ocean";
    h.settle();
    h.click("Ocean Drive");
    h.click("Launch settings");
    h.click("Name your preset");
    h.frame(vec![Event::Text("Weekend setup".into())]);
    h.settle();
    h.click("Save profile as preset");
    assert_eq!(h.app.saved.project_launch[key].selected_preset, None);
    h.click("Weekend setup");
    assert!(h.labels.contains_key("--old"));
    assert_eq!(h.app.saved.project_launch[key].selected_preset, None);
    h.click("Use for this project");
    assert_eq!(h.app.saved.project_launch[key].selected_preset.as_deref(), Some("Weekend setup"));
    h.app.saved.project_launch.get_mut(key).unwrap().args = Some("--edited".into());
    h.settle();
    assert!(h.labels.contains_key("Weekend setup · edited"));
    h.click("Use for this project");
    assert_eq!(h.app.saved.project_launch[key].args.as_deref(), Some("--old"));
    assert_eq!(h.app.saved.launch_presets.len(), 1);
    assert!(!h.labels.contains_key("Weekend setup · edited"));
}

#[test]
fn profile_launch_cards_stack_when_narrow_and_manage_opens_the_presets_tab() {
    let mut h = Harness::new();
    h.settle();
    h.click("Ocean Drive");
    h.click("Launch settings");
    assert!((h.at("Project sources").y - h.at("My presets").y).abs() < 5.0);
    h.size = vec2(960.0, 820.0);
    h.settle();
    assert!(h.at("My presets").y > h.at("Project sources").y + 35.0);
    h.click("Manage");
    assert!(h.app.view == View::Presets);
    h.click("ACTIVITY");
    assert!(h.app.view == View::Activity);
    h.click("PRESETS");
    assert!(h.labels.contains_key("New preset"));
}

#[test]
fn detected_script_can_be_copied_without_applying_or_losing_its_project_link() {
    let mut h = Harness::new();
    let dir = std::env::temp_dir().join(format!("palmvi-profile-scripts-{}",std::process::id()));
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("scripts/play-ocean.ps1"), "$set = @{\nVICTORY_LOG = 'trace'\n}").unwrap();
    h.app.saved.custom[0].local_exe = Some(dir.join("victory.exe").display().to_string());
    h.settle();
    h.click("Ocean Drive");
    h.click("Launch settings");
    h.click("Save a copy");
    let key = "example/ocean";
    assert!(h.app.saved.project_launch[key].switches.is_none());
    assert!(h.app.saved.launch_presets[0].repo_script.is_none());
    h.click("Use script settings");
    assert_eq!(h.app.saved.project_launch[key].switches.as_deref(), Some("VICTORY_LOG=trace"));
    let active = h.app.saved.project_launch[key].selected_preset.clone();
    h.click("Name your preset");
    h.frame(vec![Event::Text("My script setup".into())]);
    h.settle();
    h.click("Save profile as preset");
    assert_eq!(active, h.app.saved.project_launch[key].selected_preset);
    assert_eq!(h.app.saved.launch_presets.iter().filter(|p|p.repo_script.is_none()).count(),2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn manager_creates_multiple_presets_without_a_project_and_edits_independently() {
    let mut h = Harness::new();
    h.settle();
    h.click("Activity / Presets");
    h.click("PRESETS");
    for name in ["Morning", "Evening"] {
        h.click("New preset");
        h.click("Name your preset");
        h.frame(vec![Event::Text(name.into())]);
        h.settle();
        h.click("Save preset");
    }
    assert_eq!(h.app.saved.launch_presets.len(), 2);
    assert!(h.app.saved.project_launch.values().all(|p|p.selected_preset.is_none()));
    h.click("Evening");
    assert!(h.labels.contains_key("Edit"));
    h.click("Evening");
    assert!(!h.labels.contains_key("Edit"));
    h.click("Evening");
    h.click("Edit");
    h.click("Enter launch arguments");
    h.frame(vec![Event::Text("--example".into())]);
    h.settle();
    h.click("Save preset");
    assert_eq!(h.app.saved.launch_presets[1].args, "--example");
    assert_eq!(h.app.saved.launch_presets[0].args, "");
    h.click("Library");
    h.click("Ocean Drive");
    h.click("Launch settings");
    h.click("Evening");
    h.click("Use for this project");
    assert_eq!(h.app.saved.project_launch["example/ocean"].args.as_deref(), Some("--example"));
}

#[test]
fn fps_column_sorts_measured_projects_and_profile_resets_one_average() {
    let mut h = Harness::new();
    h.size = vec2(960.0, 740.0);
    for (name, repo) in [("High", "example/high"), ("Missing", "example/missing")] {
        let mut project = h.app.saved.custom[0].clone();
        project.name = name.into();
        project.repo = repo.into();
        h.app.saved.custom.push(project);
    }
    for (repo, value) in [("example/ocean", 25), ("example/high", 50)] {
        let session = h.app.saved.play_stats.start(repo.into());
        session.observe(&format!("victory_gpu::host: {value} fps, 20 ms/frame"));
    }
    h.settle();
    h.click("AVG FPS");
    assert!(h.at("50.0").y < h.at("25.0").y && h.at("25.0").y < h.at("Missing").y);
    h.click("AVG FPS ↓");
    assert!(h.at("25.0").y < h.at("50.0").y && h.at("50.0").y < h.at("Missing").y);
    h.click("25.0");
    assert!(h.at("AVERAGE FPS").y > h.at(&glyph::COPY.to_string()).y);
    assert!(h.at("PLAYTIME").x > h.at("AVERAGE FPS").x);
    h.click("25.0 FPS");
    h.click("Exclude exactly 60 FPS");
    assert!(h.app.saved.include_exact_60_fps);
    h.click("Reset FPS average");
    assert!(h.labels.contains_key("AVERAGE FPS") && !h.labels.contains_key("25.0 FPS"));
    h.click("Library");
    assert!(h.labels.contains_key("50.0") && !h.labels.contains_key("25.0"));
}

#[test]
fn profile_actions_and_stats_share_the_right_column_at_both_window_sizes() {
    let mut h = Harness::new();
    h.settle();
    h.click("Ocean Drive");
    for size in [vec2(960.0, 620.0), vec2(1440.0, 900.0)] {
        h.size = size;
        h.settle();
        let update = h.at("Check for updates");
        let play = h.at("Play project");
        let fps = h.at("AVERAGE FPS");
        let time = h.at("PLAYTIME");
        let folder = h.at(&glyph::FOLDER.to_string());
        assert!((update.y - play.y).abs() < 2.0 && update.x < play.x);
        assert!(size.x - play.x < 200.0 && size.x - fps.x < 430.0);
        assert!(fps.x > h.at("Ocean Drive").x);
        assert!(update.x - fps.x < 90.0 && play.x - fps.x < 300.0);
        assert!(play.y < folder.y && folder.y < time.y);
        assert!((fps.y - time.y).abs() < 1.0 && time.x < folder.x);
    }
}

#[test]
fn navigation_sharing_and_import_fit_the_minimum_window() {
    let mut h = Harness::new();
    h.size = vec2(960.0, 620.0);
    h.app.saved.gpu_filter = 2;
    let mut community = h.app.saved.custom[0].clone();
    community.repo = "example/nvidia".into();
    community.name = "NVIDIA fixture".into();
    community.custom = false;
    community.gpu = vec!["NVIDIA".into()];
    h.app.saved.pinned.push(community.repo.clone());
    *h.app.projects.lock().unwrap() = Some(Ok(vec![community]));
    h.settle();
    for label in ["CPU", "GPU", "vs PS5", "Sharing mode"] {
        assert!(h.at(label).y < 60.0);
    }
    assert!(h.at("Game files ready").y > h.size.y - 26.0);
    let info = h.at(&'\u{E946}'.to_string());
    let update = h.at(&glyph::SYNC.to_string());
    assert!(update.x - info.x >= 31.0 && update.x < 208.0);
    assert!((update.y - info.y).abs() < 3.0);
    let comparison = h.at("vs PS5");
    assert!(h.at("Sharing mode").x - comparison.x > 80.0);
    h.click_at(comparison);
    assert!(h.app.studio.specs_open);
    h.app.studio.specs_open = false;
    h.settle();
    assert!(!h.labels.contains_key("AMD"));
    // The pinned entry appears once in the rail and once in the unfiltered table.
    assert_eq!(h.labels["NVIDIA fixture"].len(), 2);
    h.click("Discover");
    assert!(h.app.view == View::Discover);
    assert!(h.labels.contains_key("AMD") && h.labels.contains_key("vs PS5"));
    h.click("Activity / Presets");
    assert!(h.app.view == View::Activity);
    h.click("Library");
    h.click("Ocean Drive");
    let logs_tab = h.labels["Live logs"][0];
    h.click_at(logs_tab);
    assert!(h.labels.contains_key("Previous session logs") || h.app.log_expanded);
    h.click("Sharing mode");
    assert!(h.app.screenshot_projects.is_some());
    assert!(!h.labels.contains_key("Ocean Drive") && !h.labels.contains_key("Copy repo link"));
    h.click("Sharing mode");
    h.click("+   Import Repo / ZIP");
    h.click("Build ZIP");
    assert!(h.labels.contains_key("Choose build ZIP…"));
    h.click("Git repository");
    assert!(
        h.app.add_repo.as_ref().is_some_and(|add| !add.zip),
        "Git tab state"
    );
    assert!(h.labels.contains_key("Import repo link"));
    h.click("Cancel");
    assert!(h.app.add_repo.is_none());
}
