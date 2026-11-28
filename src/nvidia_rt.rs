//! The Windows source build of the NVIDIA fork has its own `play` entry point.
use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Stdio},
};

use crate::{Launch, OutputResolution, EXE_NAME};


pub(super) fn supported(root: &Path) -> bool {
    root.join("Get-NVIDIA-DLSS.cmd").is_file()
        && root.join("PLAY.cmd").is_file()
        && root.join("apps/victory/src/play.rs").is_file()
}

/// NVIDIA's original fork ships the F1-aware native command without Kemerd's Fast wrapper.
pub(super) fn has_overlay_player(root: &Path) -> bool {
    root.join("Get-NVIDIA-DLSS.cmd").is_file()
        && root.join("scripts/play-foxhound.ps1").is_file()
        && root.join("apps/victory/src/play.rs").is_file()
}

// Read only the literal --set pairs from the upstream recommended Fast preset.
// No batch commands or substitutions are evaluated.
fn fast_switches(script: &str) -> Result<BTreeMap<String, String>, String> {
    let mut lines = script.lines().map(str::trim);
    if !lines.any(|line| line.eq_ignore_ascii_case(":fast")) {
        return Err("The installed PLAY.cmd has no recognized Fast preset.".into());
    }
    let mut settings = BTreeMap::new();
    for line in lines.take_while(|s| !s.starts_with(':') && !s.eq_ignore_ascii_case("exit /b 0")) {
        if line.is_empty() || line.starts_with("rem ") {
            continue;
        }
        let args = line
            .strip_prefix("set \"ARGS=!ARGS! ")
            .and_then(|s| s.strip_suffix('"'))
            .ok_or("Unrecognized command in the installed PLAY.cmd Fast preset.")?;
        let mut words = args.split_whitespace();
        while let Some(flag) = words.next() {
            let pair = words.next().and_then(|s| s.split_once('='));
            let Some((key, value)) = pair.filter(|(k, v)| {
                flag == "--set"
                    && k.starts_with("VICTORY_")
                    && k.bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                    && !v.is_empty()
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            }) else {
                return Err("Unrecognized switch in the installed PLAY.cmd Fast preset.".into());
            };
            settings.insert(key.to_owned(), value.to_owned());
        }
    }
    if settings.is_empty() {
        return Err("The installed PLAY.cmd Fast preset is empty.".into());
    }
    Ok(settings)
}

pub(super) fn command(
    exe: &Path,
    root: &Path,
    repo: &str,
    launch: &Launch,
) -> Result<Command, String> {
    let mut settings = BTreeMap::new();
    if supported(root) {
        if !launch.nvidia_plain {
            let script = std::fs::read_to_string(root.join("PLAY.cmd"))
                .map_err(|e| format!("Reading PLAY.cmd: {e}"))?;
            settings = fast_switches(&script)?;
        }
        for (key, value) in [("VICTORY_PIPELINE_SAMPLER_TABLE", "1"), ("VICTORY_HDR", "0"), ("VICTORY_REFLEX", "boost")] {
            settings.insert(key.into(), value.into());
        }
    }
    settings.insert("VICTORY_FULLSCREEN".into(), if launch.fullscreen { "1" } else { "0" }.into());
    let mut cmd = Command::new(exe);
    cmd.current_dir(root)
        .stdin(Stdio::null())
        .arg("play")
        .arg(launch.game_dir.join(EXE_NAME));
    let ws = crate::workspace();
    cmd.env("VICTORY_GAME_DIR", &launch.game_dir)
        .env(
            "VICTORY_CACHE_DIR",
            ws.join("cache").join(crate::slug(repo)),
        )
        .env("VICTORY_DATA_DIR", ws.join("data").join(crate::slug(repo)));
    let has = |flag: &str| {
        launch
            .args
            .iter()
            .any(|arg| arg.split('=').next() == Some(flag))
    };
    if launch.series_s && !has("--series-s") && !has("--console") {
        cmd.arg("--series-s");
    }
    if !has("--res") {
        match launch.output {
            OutputResolution::P1080 => { cmd.args(["--res", "1080to1440"]); }
            OutputResolution::P2160 => { cmd.args(["--res", "4k"]); }
            OutputResolution::P720 if !launch.series_s => return Err("For 720p select the Series S profile in Graphics. This build has no Series X 720p mode.".into()),
            OutputResolution::P936 => return Err("This build has no 936p mode. Choose Game default, 1080p, 1440p or 4K in Graphics.".into()),
            _ => {}
        }
    }
    let sdk = root.join("nvidia-sdk/Streamline/bin/x64");
    if std::env::var_os("VICTORY_STREAMLINE_DIR").is_none()
        && sdk.join("sl.interposer.dll").is_file()
    {
        cmd.env("VICTORY_STREAMLINE_DIR", sdk);
    }
    // Native play supplies the script baseline before reading F1 choices. Only
    // locally changed values belong in --set, which takes priority over F1.
    let script_defaults = if launch.foxhound_defaults && has_overlay_player(root) {
        let source = std::fs::read_to_string(root.join("scripts/play-foxhound.ps1")).map_err(|e| e.to_string())?;
        crate::script_preset::parse(&source)?.0
    } else {
        String::new()
    };
    let defaults: BTreeMap<_, _> = script_defaults.lines().filter_map(|line| line.split_once('=')).collect();
    for line in launch.extra.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if key.starts_with("VICTORY_") && defaults.get(key).copied() != Some(value.trim()) {
                settings.insert(key.into(), value.trim().into());
            }
        }
    }
    // Normalize native --set overrides too, so warm-up sees exactly the same switches
    // and a selected save can still override VICTORY_DATA_DIR at the final launch step.
    let mut native_args = Vec::new();
    let mut args = launch.args.iter();
    while let Some(arg) = args.next() {
        if arg == "--console" || arg.starts_with("--console=") {
            let console = arg.strip_prefix("--console=").or_else(|| args.next().map(String::as_str));
            match console {
                Some("series-s") if !has("--series-s") => native_args.push("--series-s"),
                Some("series-x" | "series-s") => {}
                _ => return Err("Expected --console series-s or series-x.".into()),
            }
            continue;
        }
        let value = if arg == "--set" {
            Some(
                args.next()
                    .ok_or("--set needs a VICTORY_NAME=value pair.")?
                    .as_str(),
            )
        } else {
            arg.strip_prefix("--set=")
        };
        if let Some(value) = value {
            let (key, value) = value
                .split_once('=')
                .ok_or("--set needs a VICTORY_NAME=value pair.")?;
            if !key.starts_with("VICTORY_")
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(format!("Invalid VIctory switch name: {key}"));
            }
            settings.insert(key.into(), value.into());
        } else {
            native_args.push(arg.as_str());
        }
    }
    for (key, value) in settings {
        cmd.env(&key, &value); // Also applied to the separate warm-up commands.
        if !matches!(
            key.as_str(),
            "VICTORY_CONFIG"
                | "VICTORY_GAME_DIR"
                | "VICTORY_CACHE_DIR"
                | "VICTORY_DATA_DIR"
                | "VICTORY_FIDELITYFX_DIR"
                | "VICTORY_STREAMLINE_DIR"
                | "VICTORY_LOG"
        ) {
            cmd.arg("--set").arg(format!("{key}={value}"));
        }
    }
    cmd.args(native_args);
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_nvidia_source_uses_native_play_without_the_fast_wrapper() {
        let root = std::env::temp_dir().join(format!("palmvi-overlay-source-{}", std::process::id()));
        for folder in ["scripts", "apps/victory/src"] { std::fs::create_dir_all(root.join(folder)).unwrap(); }
        for marker in ["Get-NVIDIA-DLSS.cmd", "apps/victory/src/play.rs", "scripts/play-foxhound.ps1"] {
            std::fs::write(root.join(marker), []).unwrap();
        }
        let p = crate::Project { name: "Overlay fixture".into(), repo: "local/overlay-test".into(), branch: None,
            gpu: vec![], fps: None, render: None, verified: false, summary: String::new(), custom: true,
            local_exe: Some(root.join("victory.exe").display().to_string()), archive_exe: None };
        let mut l = crate::Saved::default().launch(&p.repo, false);
        l.args = vec!["--console".into(), "series-s".into(), "--dlss".into(), "balanced".into()];
        let command = crate::game_command(&root.join("victory.exe"), &p, &l).unwrap();
        let args: Vec<_> = command.get_args().map(|s| s.to_string_lossy()).collect();
        assert_eq!(args[0], "play");
        assert!(args.iter().any(|s| s == "--series-s"));
        assert!(args.iter().any(|s| s == "balanced"));
        assert!(!args.iter().any(|s| s.contains("VICTORY_REFLEX") || s == "--console"));
        assert_eq!(crate::detect_build_profile(&root), "release");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "set PALMVI_TEST_OVERLAY_EXE to the reviewed NVIDIA executable"]
    fn native_overlay_reads_saved_dlss_on_the_next_start() {
        let exe = std::path::absolute(std::env::var_os("PALMVI_TEST_OVERLAY_EXE").unwrap()).unwrap();
        let scratch = std::env::temp_dir().join(format!("palmvi overlay reload {}", std::process::id()));
        let game = scratch.join("game");
        let sdk = scratch.join("sdk");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&sdk).unwrap();
        std::fs::write(game.join(EXE_NAME), []).unwrap();
        // Native settings resolution checks SDK presence before attempting to load the empty title.
        std::fs::write(sdk.join("sl.interposer.dll"), []).unwrap();
        let p = crate::Project { name: "NVIDIA packaged fixture".into(), repo: "local/nvidia-f1-reload".into(), branch: None,
            gpu: vec![], fps: None, render: None, verified: false, summary: String::new(), custom: true,
            local_exe: Some(exe.display().to_string()), archive_exe: None };
        let mut launch = crate::Saved::default().launch(&p.repo, false);
        launch.game_dir = game;
        launch.args = vec!["--dlss".into(), "quality".into()];
        assert_eq!(crate::game_command(&exe, &p, &launch).unwrap().get_args().next().unwrap(), "play");
        let root = exe.parent().unwrap().parent().unwrap().parent().unwrap();
        let source = std::fs::read_to_string(root.join("scripts/play-foxhound.ps1")).unwrap();
        let (defaults, options) = crate::script_preset::parse(&source).unwrap();
        assert!(options.foxhound);
        let mut saved = crate::Saved::default();
        saved.game_dir = launch.game_dir.display().to_string();
        saved.project_launch.insert(p.repo.clone(), crate::ProjectLaunch {
            switches: Some(format!("{defaults}\nVICTORY_WC_NO_BACK_MIN_KB=1536")), script_options: Some(options),
            args: Some("--dlss quality".into()), ..Default::default() });
        launch = saved.launch(&p.repo, false);
        let p = crate::Project { local_exe: Some(root.join("victory.exe").display().to_string()), ..p };
        let menu = scratch.join("victory").join("victory-settings.txt");
        for expected in ["quality", "balanced"] {
            let mut cmd = crate::game_command(&exe, &p, &launch).unwrap();
            assert_eq!(cmd.get_args().next().unwrap(), "play");
            let result = cmd.env("LOCALAPPDATA", &scratch).env("VICTORY_STREAMLINE_DIR", &sdk)
                .env("VICTORY_DATA_DIR", scratch.join("save")).env("VICTORY_CACHE_DIR", scratch.join("cache")).output().unwrap();
            assert!(!result.status.success(), "empty test title should fail to load");
            let log = std::fs::read_dir(scratch.join("victory/logs")).unwrap().flatten()
                .find(|f| f.path().extension().is_some_and(|ext| ext == "log")).unwrap().path();
            let text = std::fs::read_to_string(&log).unwrap();
            assert!(text.contains(&format!("VICTORY_MENU_FILE={}", menu.display())), "{text}");
            assert!(text.contains(&format!("VICTORY_DLSS_MODE={expected}")), "{text}");
            assert!(text.contains("VICTORY_WC_NO_BACK_MIN_KB=1536"));
            if expected == "balanced" { assert!(text.contains("VICTORY_AMBIENT=0.5")); }
            std::fs::remove_file(log).unwrap();
            std::fs::write(&menu, "VICTORY_MENU_DLSS=balanced\r\nVICTORY_AMBIENT=0.5\r\n").unwrap();
        }
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn reads_only_fast_literal_pairs() {
        let script = "set \"ARGS=!ARGS! --set VICTORY_OTHER=1\"\n:fast\nset \"ARGS=!ARGS! --set VICTORY_A=1 --set VICTORY_B=graphics\"\nexit /b 0";
        let result = fast_switches(script).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result["VICTORY_B"], "graphics");
        assert!(fast_switches(&script.replace("graphics", "%PATH%")).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "set PALMVI_TEST_NVIDIA_ROOT to a built checkout of the NVIDIA RT fork"]
    fn native_play_accepts_generated_command_and_keeps_profile_paths() {
        let root = std::path::PathBuf::from(std::env::var_os("PALMVI_TEST_NVIDIA_ROOT").unwrap());
        let exe = root.join("target/release/victory.exe");
        assert!(supported(&root));
        assert_eq!(crate::detect_build_profile(&root), "release");
        let scratch =
            std::env::temp_dir().join(format!("palmvi nvidia smoke {}", std::process::id()));
        std::fs::create_dir_all(&scratch).unwrap();
        let mut launch = crate::Saved::default().launch("test/rt", false);
        launch.game_dir = scratch.join("missing game");
        launch.fullscreen = true;
        launch.extra = "VICTORY_REFLEX=on".into();
        launch.args = vec!["--dlss".into(), "off".into()];
        let mut cmd = command(&exe, &root, "test/rt", &launch).unwrap();
        let args: Vec<_> = cmd
            .get_args()
            .map(|v| v.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "play");
        assert_eq!(Path::new(&args[1]), launch.game_dir.join(EXE_NAME));
        assert!(args.iter().any(|v| v == "VICTORY_SEND_THREAD=1"));
        assert!(args.iter().any(|v| v == "VICTORY_REFLEX=on"));
        assert!(crate::launch_folder(&cmd, "VICTORY_DATA_DIR")
            .unwrap()
            .ends_with("data/test_rt"));
        // The real CLI only reaches its missing-game check after accepting every argument.
        let out = cmd.env("LOCALAPPDATA", &scratch).output().unwrap();
        let message = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !out.status.success()
                && message.contains("missing game")
                && !message.contains("unexpected argument"),
            "{message}"
        );
        // An empty title reaches native settings resolution, then fails the upstream PE loader.
        std::fs::create_dir_all(&launch.game_dir).unwrap();
        std::fs::write(launch.game_dir.join(EXE_NAME), []).unwrap();
        let out = cmd
            .env("VICTORY_CACHE_DIR", scratch.join("cache"))
            .env("VICTORY_DATA_DIR", scratch.join("save"))
            .output()
            .unwrap();
        assert!(!out.status.success());
        let log = std::fs::read_dir(scratch.join("victory/logs"))
            .unwrap()
            .flatten()
            .find(|f| f.file_name().to_string_lossy().starts_with("play-"))
            .unwrap();
        let text = std::fs::read_to_string(log.path()).unwrap();
        for setting in [
            "console=series-x",
            "VICTORY_SEND_THREAD=1",
            "VICTORY_MENU_DLSS=off",
            "VICTORY_REFLEX=on",
        ] {
            assert!(text.contains(setting), "{setting} missing from native log");
        }
        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
