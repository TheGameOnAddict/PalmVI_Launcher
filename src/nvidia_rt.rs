//! The Windows source build of the NVIDIA fork has its own `play` entry point.
use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Stdio},
};

use crate::{Launch, OutputResolution, EXE_NAME};

#[cfg(windows)]
pub(super) fn needs_agility() -> Result<bool, String> {
    use windows::{
        core::w,
        Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ},
    };
    let mut buffer = [0u16; 32];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .map_err(|e| format!("Could not read the Windows build number: {e}"))?;
    }
    let end = buffer.iter().position(|n| *n == 0).unwrap_or(buffer.len());
    let build: u32 = String::from_utf16_lossy(&buffer[..end])
        .parse()
        .map_err(|_| "Invalid Windows build number")?;
    Ok(build < 22000)
}

#[cfg(not(windows))]
pub(super) fn needs_agility() -> Result<bool, String> {
    Ok(false)
}

// PLAY.cmd pins this exact x64 Microsoft runtime and checks the extracted DLL.
pub(super) fn install_agility(exe_dir: &Path) -> Result<(), String> {
    use std::io::{Cursor, Read};
    const HASH: &[u8; 32] = &[
        0x70, 0xa6, 0xd4, 0x7d, 0x33, 0x5b, 0x4a, 0x15, 0x18, 0x05, 0x44, 0xc0, 0x1a, 0x35, 0xb8,
        0x44, 0x74, 0x52, 0x2a, 0x36, 0x9d, 0xc5, 0x82, 0xd8, 0x5f, 0xe0, 0x57, 0x31, 0x3a, 0x3a,
        0xea, 0xa0,
    ];
    let matches = |dll: &[u8]| ring::digest::digest(&ring::digest::SHA256, dll).as_ref() == HASH;
    let folder = exe_dir.join("D3D12");
    let file = folder.join("D3D12Core.dll");
    if std::fs::read(&file).is_ok_and(|dll| matches(&dll)) {
        return Ok(());
    }
    let response = ureq::get("https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.d3d12/1.620.2/microsoft.direct3d.d3d12.1.620.2.nupkg")
        .call().map_err(|e| format!("Downloading Microsoft's Windows 10 runtime: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(64 << 20)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let mut package = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut dll = Vec::new();
    package
        .by_name("build/native/bin/x64/D3D12Core.dll")
        .map_err(|e| e.to_string())?
        .take(16 << 20)
        .read_to_end(&mut dll)
        .map_err(|e| e.to_string())?;
    if !matches(&dll) {
        return Err("Microsoft D3D12 runtime failed the upstream SHA-256 check.".into());
    }
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let temp = folder.join(format!("D3D12Core-{}.tmp", std::process::id()));
    std::fs::write(&temp, dll)
        .and_then(|_| std::fs::rename(temp, &file))
        .map_err(|e| e.to_string())
}

pub(super) fn supported(root: &Path) -> bool {
    root.join("Get-NVIDIA-DLSS.cmd").is_file()
        && root.join("PLAY.cmd").is_file()
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
    let script = std::fs::read_to_string(root.join("PLAY.cmd"))
        .map_err(|e| format!("Reading PLAY.cmd: {e}"))?;
    let mut settings = if launch.nvidia_plain {
        BTreeMap::new()
    } else {
        fast_switches(&script)?
    };
    for (key, value) in [
        ("VICTORY_PIPELINE_SAMPLER_TABLE", "1"),
        ("VICTORY_HDR", "0"),
        ("VICTORY_REFLEX", "boost"),
        (
            "VICTORY_FULLSCREEN",
            if launch.fullscreen { "1" } else { "0" },
        ),
    ] {
        settings.insert(key.into(), value.into());
    }
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
    if launch.series_s && !has("--series-s") {
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
    for line in launch.extra.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if key.starts_with("VICTORY_") {
                settings.insert(key.into(), value.trim().into());
            }
        }
    }
    // Normalize native --set overrides too, so warm-up sees exactly the same switches
    // and a selected save can still override VICTORY_DATA_DIR at the final launch step.
    let mut native_args = Vec::new();
    let mut args = launch.args.iter();
    while let Some(arg) = args.next() {
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
            native_args.push(arg);
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
