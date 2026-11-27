use super::{button_row, note};
use crate::{theme, GraphicsSettings, OutputResolution, Project, ProjectLaunch};
use eframe::egui::{self, Ui};

pub(crate) struct NativeSetup {
    pub project: Project,
    pub draft: ProjectLaunch,
    pub revision: Option<String>,
}

// Keep the form and advanced text on the same source of truth, including imported presets.
struct Arguments(Vec<String>);

impl Arguments {
    fn take(&mut self, flag: &str, value: bool) -> Option<String> {
        let mut result = None;
        let mut kept = Vec::new();
        let mut args = std::mem::take(&mut self.0).into_iter();
        while let Some(arg) = args.next() {
            if arg == "--" {
                kept.push(arg);
                kept.extend(args);
                break;
            }
            if arg == flag {
                result = Some(if value {
                    args.next().unwrap_or_default()
                } else {
                    String::new()
                });
            } else if let Some((_, v)) = arg.split_once('=').filter(|(name, _)| *name == flag) {
                result = Some(v.to_owned());
            } else {
                kept.push(arg);
            }
        }
        self.0 = kept;
        result
    }
    fn get(&self, flag: &str, value: bool) -> Option<String> {
        Self(self.0.clone()).take(flag, value)
    }
    fn put(&mut self, flag: &str, value: Option<&str>) {
        self.take(flag, true);
        if let Some(v) = value {
            self.0.splice(0..0, [flag.to_owned(), v.to_owned()]);
        }
    }
    fn text(&self) -> String {
        self.0
            .iter()
            .map(|s| {
                if s.is_empty() || s.contains(char::is_whitespace) {
                    format!("\"{s}\"")
                } else {
                    s.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    // --set takes precedence over environment lines in an imported configuration.
    fn remove_switch(&mut self, key: &str) -> Option<String> {
        let mut last = None;
        let mut kept = Vec::new();
        let mut args = std::mem::take(&mut self.0).into_iter().peekable();
        while let Some(arg) = args.next() {
            if arg == "--" {
                kept.push(arg);
                kept.extend(args);
                break;
            }
            let separate = arg == "--set";
            let pair = if separate {
                args.peek().map(String::as_str)
            } else {
                arg.strip_prefix("--set=")
            };
            if let Some((_, value)) = pair
                .and_then(|p| p.split_once('='))
                .filter(|(k, _)| *k == key)
            {
                last = Some(value.to_owned());
                if separate {
                    args.next();
                }
            } else {
                kept.push(arg);
            }
        }
        self.0 = kept;
        last
    }
}

fn choice(ui: &mut Ui, label: &str, current: &str, options: &[(&str, &str)]) -> Option<String> {
    ui.label(egui::RichText::new(label).font(theme::strong(14.0)));
    let mut pick = None;
    let title = options
        .iter()
        .find(|(v, _)| *v == current)
        .map(|(_, s)| *s)
        .unwrap_or(current);
    egui::ComboBox::from_id_salt(label)
        .width(ui.available_width().min(460.0))
        .height(250.0)
        .selected_text(title)
        .show_ui(ui, |ui| {
            for (value, title) in options {
                if ui.selectable_label(*value == current, *title).clicked() {
                    pick = Some((*value).to_owned());
                }
            }
        });
    pick
}

fn active_switch(
    args: &Arguments,
    profile: &ProjectLaunch,
    inherited: &str,
    key: &str,
    default: &str,
) -> String {
    let text = profile.switches.as_deref().unwrap_or(inherited);
    let current = text
        .lines()
        .filter_map(|s| s.split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .last()
        .map(|(_, v)| v.trim())
        .unwrap_or(default);
    Arguments(args.0.clone())
        .remove_switch(key)
        .unwrap_or_else(|| current.into())
}

fn write_switch(
    args: &mut Arguments,
    profile: &mut ProjectLaunch,
    inherited: &str,
    key: &str,
    value: Option<&str>,
) {
    args.remove_switch(key);
    let text = profile.switches.as_deref().unwrap_or(inherited);
    let mut lines: Vec<_> = text
        .lines()
        .filter(|s| !s.split_once('=').is_some_and(|(k, _)| k.trim() == key))
        .map(str::to_owned)
        .collect();
    if let Some(v) = value {
        lines.push(format!("{key}={v}"));
    }
    if lines.join("\n") != text {
        profile.switches = Some(lines.join("\n"));
    }
}

fn switch(
    ui: &mut Ui,
    args: &mut Arguments,
    profile: &mut ProjectLaunch,
    inherited: &str,
    label: &str,
    key: &str,
    default: &str,
    values: &[(&str, &str)],
) {
    let current = active_switch(args, profile, inherited, key, default);
    if let Some(value) = choice(ui, label, &current, values) {
        write_switch(args, profile, inherited, key, Some(&value));
    }
}

pub(super) fn controls(
    ui: &mut Ui,
    profile: &mut ProjectLaunch,
    global: GraphicsSettings,
    inherited_args: &str,
    inherited_switches: &str,
) {
    let mut graphics = profile.graphics.unwrap_or(global);
    let old_graphics = graphics;
    let mut args = Arguments(crate::split_args(
        profile.args.as_deref().unwrap_or(inherited_args),
    ));
    let old_args = args.0.clone();
    ui.columns(2, |col| {
        let series_s = graphics.series_s || args.get("--series-s", false).is_some();
        if let Some(mode) = choice(
            &mut col[0],
            "Console profile",
            if series_s { "s" } else { "x" },
            &[("x", "Series X"), ("s", "Series S")],
        ) {
            args.take("--series-s", false);
            graphics.series_s = mode == "s";
            if !graphics.series_s && graphics.output == OutputResolution::P720 {
                graphics.output = OutputResolution::Default;
            }
        }
        if let Some(mode) = choice(
            &mut col[1],
            "Performance tuning",
            if profile.nvidia_plain { "base" } else { "opt" },
            &[("opt", "Recommended optimizations"), ("base", "Baseline")],
        ) {
            profile.nvidia_plain = mode == "base";
        }
    });
    note(ui,"Recommended enables the project’s rendering tweaks. Baseline uses its standard settings. Both work with Series X or S.");
    ui.add_space(9.0);
    ui.columns(2, |col| {
        let default_res = match graphics.output {
            OutputResolution::P1080 => "1080to1440",
            OutputResolution::P2160 => "4k",
            _ => "1440",
        };
        let res = args.get("--res", true).unwrap_or(default_res.into());
        let res = active_switch(
            &args,
            profile,
            inherited_switches,
            "VICTORY_DISPLAY_WIDE",
            &res,
        );
        let series_s = graphics.series_s || args.get("--series-s", false).is_some();
        if series_s {
            col[0].label(egui::RichText::new("Resolution").font(theme::strong(14.0)));
            col[0].label("720p · Series S");
        } else if let Some(v) = choice(
            &mut col[0],
            "Resolution",
            &res,
            &[
                ("1440", "1440p"),
                ("1080to1440", "1080p upscaled to 1440p"),
                ("4k", "4K"),
                ("wide", "5120 × 1440 ultrawide"),
                ("3440x1440", "3440 × 1440 ultrawide"),
                ("2560x1080", "2560 × 1080 ultrawide"),
            ],
        ) {
            args.put("--res", (v == "wide").then_some("wide"));
            write_switch(
                &mut args,
                profile,
                inherited_switches,
                "VICTORY_DISPLAY_WIDE",
                matches!(v.as_str(), "3440x1440" | "2560x1080").then_some(v.as_str()),
            );
            graphics.output = match v.as_str() {
                "1080to1440" => OutputResolution::P1080,
                "4k" => OutputResolution::P2160,
                _ => OutputResolution::P1440,
            };
        }
        let dlss = args.get("--dlss", true).unwrap_or("quality".into());
        if let Some(v) = choice(
            &mut col[1],
            "DLSS upscaling",
            &dlss,
            &[
                ("quality", "Quality"),
                ("balanced", "Balanced"),
                ("performance", "Performance"),
                ("dlaa", "DLAA"),
                ("off", "Off"),
            ],
        ) {
            args.put("--dlss", Some(&v));
        }
    });
    ui.add_space(9.0);
    ui.columns(2, |col| {
        let fg = args.get("--framegen", true).unwrap_or_else(|| {
            if args.get("--no-framegen-ready", false).is_some() {
                "off".into()
            } else {
                "ready".into()
            }
        });
        if let Some(v) = choice(
            &mut col[0],
            "Frame generation",
            &fg,
            &[
                ("ready", "Ready · enable with F1"),
                ("off", "Off"),
                ("2", "2×"),
                ("3", "3×"),
                ("4", "4×"),
                ("5", "5×"),
                ("6", "6×"),
            ],
        ) {
            args.take("--no-framegen-ready", false);
            args.put(
                "--framegen",
                (!matches!(v.as_str(), "off" | "ready")).then_some(v.as_str()),
            );
            if v == "off" {
                args.0.insert(0, "--no-framegen-ready".into());
            }
        }
        let fps = args.get("--fps", true).unwrap_or("1000".into());
        if let Some(v) = choice(
            &mut col[1],
            "Frame rate limit",
            &fps,
            &[
                ("1000", "Uncapped"),
                ("30", "30 FPS"),
                ("60", "60 FPS"),
                ("120", "120 FPS"),
                ("144", "144 FPS"),
            ],
        ) {
            args.put("--fps", Some(&v));
        }
    });
    ui.add_space(8.0);
    egui::CollapsingHeader::new("Display and rendering options").show(ui,|ui|{
        ui.columns(2,|col|{
            let window=active_switch(&args,profile,inherited_switches,"VICTORY_FULLSCREEN",if graphics.fullscreen{"1"}else{"0"});
            if let Some(v)=choice(&mut col[0],"Window mode",&window,&[("1","Borderless fullscreen"),("0","Windowed")]){
                graphics.fullscreen=v=="1";
                write_switch(&mut args,profile,inherited_switches,"VICTORY_FULLSCREEN",None);
            }
            switch(&mut col[1],&mut args,profile,inherited_switches,"NVIDIA Reflex","VICTORY_REFLEX","boost",&[("boost","On + boost"),("on","On"),("off","Off")]);
        });
        ui.columns(2,|col|{
            switch(&mut col[0],&mut args,profile,inherited_switches,"HDR10","VICTORY_HDR","0",&[("0","Off"),("1","On")]);
            switch(&mut col[1],&mut args,profile,inherited_switches,"Native ray tracing","VICTORY_RT_NATIVE","0",&[("0","Off"),("1","On · experimental")]);
        });
        switch(ui,&mut args,profile,inherited_switches,"Ultrawide interface","VICTORY_UI_LETTERBOX","1",&[("1","Centered at 16:9"),("0","Stretch to fit")]);
        note(ui,"DLSS requires NVIDIA files from Installation. Frame generation requires RTX 40 or newer, and 3×–6× requires RTX 50. Saved F1 choices can override launch defaults.");
    });
    if args.0 != old_args {
        profile.args = Some(args.text());
    }
    if graphics != old_graphics {
        profile.graphics = Some(graphics);
    }
}

impl NativeSetup {
    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        t0: f64,
        graphics: GraphicsSettings,
        args: &str,
        switches: &str,
    ) -> Option<bool> {
        let (action, dismissed) = theme::modal(ctx, "native-project-setup", t0, 540.0, |ui| {
            theme::caps(ui, &self.project.name, 12.0, theme::lilac());
            theme::title(ui, "Project launch settings", 34.0, 540.0);
            note(ui,"PalmVI detected custom launch settings in PLAY.cmd. Choose how this project starts. You can change these later in Launch settings.");
            ui.add_space(16.0);
            controls(ui, &mut self.draft, graphics, args, switches);
            ui.add_space(12.0);
            let mut result = None;
            button_row(ui, |ui| {
                if theme::button(ui, theme::Look::Primary, None, "Save choices").clicked() {
                    result = Some(true);
                }
                if theme::button(ui, theme::Look::Ghost, None, "Decide later").clicked() {
                    result = Some(false);
                }
            });
            result
        });
        if dismissed {
            Some(false)
        } else {
            action
        }
    }
}

#[cfg(test)]
#[test]
fn menu_edits_handle_imported_flags_and_preserve_other_arguments() {
    let mut a = Arguments(crate::split_args(
        r#"--res=4k --set VICTORY_HDR=0 --set=VICTORY_HDR=1 --custom "folder with spaces" -- --res wide"#,
    ));
    let mut launch = ProjectLaunch::default();
    assert_eq!(active_switch(&a, &launch, "", "VICTORY_HDR", "0"), "1");
    write_switch(&mut a, &mut launch, "", "VICTORY_HDR", Some("0"));
    a.put("--res", Some("wide"));
    assert_eq!(active_switch(&a, &launch, "", "VICTORY_HDR", "1"), "0");
    assert_eq!(
        crate::split_args(&a.text()),
        [
            "--res",
            "wide",
            "--custom",
            "folder with spaces",
            "--",
            "--res",
            "wide"
        ]
    );
}
