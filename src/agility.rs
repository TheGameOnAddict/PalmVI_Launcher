use crate::{run, stage, Job};
use eframe::egui;
use object::{read::pe::PeFile64, Architecture, Object};
use serde_json::Value;
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

const SDK: u32 = 620;
const HASH: &[u8; 32] = &[
    0x70, 0xa6, 0xd4, 0x7d, 0x33, 0x5b, 0x4a, 0x15, 0x18, 0x05, 0x44, 0xc0, 0x1a, 0x35, 0xb8, 0x44,
    0x74, 0x52, 0x2a, 0x36, 0x9d, 0xc5, 0x82, 0xd8, 0x5f, 0xe0, 0x57, 0x31, 0x3a, 0x3a, 0xea, 0xa0,
];
fn pinned(dll: &[u8]) -> bool {
    ring::digest::digest(&ring::digest::SHA256, dll).as_ref() == HASH
}

fn download_core() -> Result<Vec<u8>, String> {
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
    if !pinned(&dll) {
        return Err("Microsoft D3D12 runtime failed the upstream SHA-256 check.".into());
    }
    Ok(dll)
}

#[cfg(windows)]
pub(super) fn windows_10() -> Result<bool, String> {
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
    if build >= 22000 {
        return Ok(false);
    }
    let minimum_revision = match build {
        18363 => 1350,
        19041 | 19042 => 789,
        19043.. => return Ok(true),
        _ => return Err("This Windows build predates Direct3D 12 Agility support. Update Windows 10 and reinstall the project.".into()),
    };
    use windows::Win32::System::Registry::RRF_RT_REG_DWORD;
    let mut revision = 0u32;
    let mut bytes = 4;
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("UBR"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut revision as *mut u32).cast()),
            Some(&mut bytes),
        )
        .ok()
        .map_err(|e| format!("Could not read the Windows update revision: {e}"))?;
    }
    if revision < minimum_revision {
        return Err(format!("Windows {build}.{revision} needs revision {minimum_revision} or later for D3D12 Agility. Update Windows and reinstall the project."));
    }
    Ok(true)
}

#[cfg(not(windows))]
pub(super) fn windows_10() -> Result<bool, String> {
    Ok(false)
}

pub(super) fn native_feature(metadata: &Value) -> bool {
    metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|pkg| pkg["name"] == "victory" && pkg["features"].get("agility").is_some())
}

// Limit the fallback to recognized VIctory workspaces with a Windows D3D12 renderer.
pub(super) fn compatible(metadata: &Value) -> bool {
    native_feature(metadata)
        || metadata["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|pkg| {
                pkg["name"] == "victory-gpu"
                    && pkg["dependencies"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|dep| {
                            dep["features"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|feature| feature == "Win32_Graphics_Direct3D12")
                        })
            })
}

#[derive(Debug, PartialEq)]
struct Declaration {
    version: u32,
    directory: PathBuf,
}

struct Exports<'a> {
    file: PeFile64<'a>,
    bytes: &'a [u8],
}

impl<'a> Exports<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let file = PeFile64::parse(bytes).map_err(|e| e.to_string())?;
        if file.architecture() != Architecture::X86_64 {
            return Err("PalmVI's Agility runtime requires an x64 Windows executable.".into());
        }
        Ok(Self { file, bytes })
    }

    fn data(&self, rva: u32) -> Result<&'a [u8], String> {
        self.file
            .section_table()
            .pe_data_at(self.bytes, rva)
            .ok_or_else(|| "Invalid SDK export address".into())
    }

    fn symbol(&self, name: &[u8]) -> Result<Option<&'a [u8]>, String> {
        let Some(table) = self.file.export_table().map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        for (pointer, index) in table.name_iter() {
            if table
                .name_from_pointer(pointer)
                .map_err(|e| e.to_string())?
                == name
            {
                let target = table.target_by_index(index).map_err(|e| e.to_string())?;
                let object::read::pe::ExportTarget::Address(rva) = target else {
                    return Err("Expected a local D3D12 data export".into());
                };
                return self.data(rva).map(Some);
            }
        }
        Ok(None)
    }

    fn version(&self) -> Result<Option<u32>, String> {
        self.symbol(b"D3D12SDKVersion")?
            .map(|data| {
                let value = data.first_chunk().ok_or("Truncated D3D12SDKVersion")?;
                Ok(u32::from_le_bytes(*value))
            })
            .transpose()
    }

    fn declaration(&self) -> Result<Option<Declaration>, String> {
        match (self.version()?, self.symbol(b"D3D12SDKPath")?) {
            (None, None) => Ok(None),
            (Some(version), Some(path)) if version > 0 => {
                let address = u64::from_le_bytes(*path.first_chunk().ok_or("Truncated D3D12SDKPath")?);
                let rva = address.checked_sub(self.file.relative_address_base())
                    .and_then(|v| u32::try_from(v).ok()).ok_or("Invalid D3D12SDKPath pointer")?;
                let path = std::ffi::CStr::from_bytes_until_nul(self.data(rva)?)
                    .map_err(|_| "Unterminated D3D12SDKPath")?.to_str().map_err(|_| "Invalid D3D12SDKPath")?;
                Ok(Some(Declaration { version, directory: relative_directory(path)? }))
            }
            _ => Err("The executable has an incomplete Agility declaration. Its project must export both D3D12SDKVersion and D3D12SDKPath.".into()),
        }
    }
}

fn relative_directory(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.replace('\\', "/"));
    if path.as_os_str().is_empty()
        || !path.components().all(|component| match component {
            Component::CurDir => true,
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                !name.contains(':') && !name.ends_with([' ', '.'])
            }
            _ => false,
        })
    {
        return Err("The project's D3D12SDKPath must be inside its executable folder.".into());
    }
    Ok(path)
}

fn runtime_folder(base: &Path, relative: &Path) -> Result<PathBuf, String> {
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let mut folder = base.clone();
    for component in relative.components() {
        if let Component::Normal(name) = component {
            folder.push(name);
            match fs::create_dir(&folder) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.to_string()),
            }
            folder = folder.canonicalize().map_err(|e| e.to_string())?;
            if !folder.starts_with(&base) || !folder.is_dir() {
                return Err(
                    "The SDK folder points outside the build or is not a directory.".into(),
                );
            }
        }
    }
    Ok(folder)
}

fn save_atomic(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = tempfile::NamedTempFile::new_in(dest.parent().ok_or("No runtime directory")?)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.persist(dest)
        .map_err(|e| format!("Could not replace {}: {e}", dest.display()))?;
    Ok(())
}

fn matches_version(bytes: &[u8], version: u32) -> bool {
    Exports::parse(bytes).is_ok_and(|pe| {
        pe.file.kind() == object::ObjectKind::Dynamic
            && pe.version() == Ok(Some(version))
            && pe.symbol(b"D3D12GetInterface").is_ok_and(|s| s.is_some())
    })
}

fn nuget_runtime(version: u32) -> Option<Vec<u8>> {
    let packages = std::env::var_os("NUGET_PACKAGES")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".nuget/packages"))
        })?;
    let packages = fs::read_dir(packages.join("microsoft.direct3d.d3d12")).ok()?;
    let mut versions: Vec<_> = packages
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let parts: Vec<u32> = name
                .to_str()?
                .split('.')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            (parts.len() == 3 && parts[1] == version).then(|| (parts, entry.path()))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().find_map(|(_, dir)| {
        let dll = fs::read(dir.join("build/native/bin/x64/D3D12Core.dll")).ok()?;
        (matches_version(&dll, version) && (version != SDK || pinned(&dll))).then_some(dll)
    })
}

fn stage_runtime(
    exe: &Path,
    sdk: &Declaration,
    project_dirs: &[&Path],
    cache: &Path,
) -> Result<String, String> {
    let dir = runtime_folder(
        exe.parent().ok_or("Missing executable directory")?,
        &sdk.directory,
    )?;
    let dest = dir.join("D3D12Core.dll");
    // The original NVIDIA RT setup pins this exact Microsoft redistributable.
    let require_pin = sdk.version == SDK
        && project_dirs
            .iter()
            .any(|dir| crate::nvidia_rt::supported(dir));
    let accepts = |dll: &[u8]| matches_version(dll, sdk.version) && (!require_pin || pinned(dll));
    if fs::read(&dest).is_ok_and(|dll| accepts(&dll)) {
        return Ok(format!(
            "Verified existing SDK {} at {}",
            sdk.version,
            dest.display()
        ));
    }
    let supplied = std::env::var_os("VICTORY_D3D12_CORE")
        .map(PathBuf::from)
        .into_iter()
        .chain(
            project_dirs
                .iter()
                .flat_map(|dir| [dir.join("D3D12Core.dll"), dir.join("D3D12/D3D12Core.dll")]),
        )
        .find_map(|path| {
            let dll = fs::read(&path).ok()?;
            accepts(&dll).then_some((dll, path))
        });
    let (dll, source) = if let Some((dll, path)) = supplied {
        (dll, path.display().to_string())
    } else if let Some(dll) = nuget_runtime(sdk.version) {
        (dll, "local NuGet package".to_owned())
    } else if sdk.version == SDK {
        let cached = cache.join("1.620.2-x64/D3D12Core.dll");
        if let Some(bytes) = fs::read(&cached).ok().filter(|dll| pinned(dll)) {
            (bytes, "verified PalmVI cache".to_owned())
        } else {
            let bytes = download_core()?;
            fs::create_dir_all(cached.parent().unwrap()).map_err(|e| e.to_string())?;
            save_atomic(&cached, &bytes)?;
            (bytes, "Microsoft NuGet".to_owned())
        }
    } else {
        return Err(format!("This project declares Agility SDK {}. Install its matching x64 Microsoft.Direct3D.D3D12 NuGet package or place its D3D12Core.dll in {} and reinstall.", sdk.version, dir.display()));
    };
    save_atomic(&dest, &dll)?;
    Ok(format!(
        "Installed SDK {} from {source} at {}",
        sdk.version,
        dest.display()
    ))
}

fn inspect(exe: &Path) -> Result<Option<Declaration>, String> {
    let bytes = fs::read(exe).map_err(|e| format!("Could not read {}: {e}", exe.display()))?;
    Exports::parse(&bytes)?.declaration()
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
fn relink(
    job: &Arc<Mutex<Job>>,
    ctx: &egui::Context,
    build: &Command,
    target: &Path,
) -> Result<(), String> {
    let obj = target.join("palmvi-agility-620.obj");
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/agility.obj"));
    if fs::read(&obj).ok().as_deref() != Some(bytes.as_slice()) {
        save_atomic(&obj, bytes)?;
    }
    // rustc applies the extra linker input to this binary only. Keep Cargo's
    // lockfile, diagnostics features, profile and upstream build scripts intact.
    let mut cmd = Command::new(build.get_program());
    for (key, value) in build.get_envs() {
        if let Some(value) = value {
            cmd.env(key, value);
        } else {
            cmd.env_remove(key);
        }
    }
    cmd.current_dir(build.get_current_dir().ok_or("Missing project directory")?)
        .arg("rustc")
        .args(
            build
                .get_args()
                .skip(1)
                .filter(|arg| *arg != "--message-format=json-render-diagnostics"),
        );
    if !build.get_args().any(|arg| arg == "--bin") {
        cmd.args(["--bin", "victory"]);
    }
    cmd.args(["--", "-C"])
        .arg(format!("link-arg={}", obj.display()));
    run(job, ctx, &mut cmd)
}

#[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
fn relink(_: &Arc<Mutex<Job>>, _: &egui::Context, _: &Command, _: &Path) -> Result<(), String> {
    Err("Adding SDK declarations requires the Windows x64 MSVC build of PalmVI.".into())
}

pub(super) fn configure(
    job: &Arc<Mutex<Job>>,
    ctx: &egui::Context,
    build: &Command,
    exe: &Path,
    target: &Path,
    repo: &Path,
) -> Result<(), String> {
    stage(job, ctx, "Checking Windows 10 Direct3D 12 support");
    let sdk = match inspect(exe)? {
        Some(sdk) => sdk,
        None => {
            stage(
                job,
                ctx,
                "Linking Windows 10 Agility support into victory.exe",
            );
            relink(job, ctx, build, target)?;
            inspect(exe)?.ok_or("The linked executable still lacks the Agility SDK exports.")?
        }
    };
    let workspace = build.get_current_dir().ok_or("Missing project directory")?;
    let message = stage_runtime(
        exe,
        &sdk,
        &[workspace, repo],
        &crate::workspace().join("runtimes/agility"),
    )?;
    job.lock().unwrap().log.push(message);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_renderer_and_native_feature_separately() {
        let plain = serde_json::json!({"packages":[{"name":"victory","features":{}}]});
        let native = serde_json::json!({"packages":[{"name":"victory","features":{"agility":[]}}]});
        let renderer = serde_json::json!({"packages":[{"name":"victory-gpu","dependencies":[{"features":["Win32_Graphics_Direct3D12"]}]}]});
        assert!(!compatible(&plain));
        assert!(compatible(&native) && native_feature(&native));
        assert!(compatible(&renderer) && !native_feature(&renderer));
    }

    #[test]
    fn sdk_paths_stay_inside_the_build() {
        for bad in [
            "",
            "../outside",
            r"D3D12\..\other",
            r"\\server\share",
            "C:/sdk",
            "/sdk",
            "D3D12:stream",
            "folder. ",
        ] {
            assert!(relative_directory(bad).is_err(), "{bad}");
        }
        assert_eq!(relative_directory(".").unwrap(), Path::new("."));
        assert_eq!(
            relative_directory(r".\D3D12\").unwrap().components().last(),
            Some(Component::Normal("D3D12".as_ref()))
        );
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
    #[test]
    fn verifies_real_player_exports_before_and_after_linking() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("nested");
        let src = workspace.join("src");
        fs::create_dir_all(src.join("bin")).unwrap();
        fs::create_dir(workspace.join(".cargo")).unwrap();
        fs::write(
            workspace.join(".cargo/config.toml"),
            "[build]\ntarget='x86_64-pc-windows-msvc'",
        )
        .unwrap();
        let target = root.path().join("build output");
        let profile = target.join("x86_64-pc-windows-msvc/debug");
        let exe = profile.join("victory.exe");
        fs::write(workspace.join("Cargo.toml"), "[package]\nname='victory'\nversion='0.0.1'\nedition='2021'\n[features]\ndiagnostics=[]\nagility=[]\n[lib]\ncrate-type=['cdylib']").unwrap();
        let main = r#"
            #[cfg(not(feature="diagnostics"))] compile_error!("Lost project flags");
            #[cfg(feature="agility")] #[no_mangle] pub static D3D12SDKVersion: u32 = 619;
            #[cfg(feature="agility")] #[no_mangle] pub static D3D12SDKPath: &u8 = &b".\0"[0];
            fn main() {}
        "#;
        fs::write(src.join("main.rs"), main).unwrap();
        fs::write(src.join("bin/helper.rs"), "fn main() {}").unwrap();
        fs::write(src.join("lib.rs"), "#[no_mangle] pub static D3D12SDKVersion: u32 = 619;\n#[no_mangle] pub extern \"C\" fn D3D12GetInterface() {}").unwrap();
        fs::write(
            workspace.join("build.rs"),
            r#"fn main() {
            if std::env::var_os("CARGO_FEATURE_AGILITY").is_some() {
                println!("cargo:rustc-link-arg-bin=victory=/EXPORT:D3D12SDKVersion,DATA");
                println!("cargo:rustc-link-arg-bin=victory=/EXPORT:D3D12SDKPath,DATA");
            }
        }"#,
        )
        .unwrap();
        let mut cargo = Command::new(env!("CARGO"));
        cargo
            .current_dir(crate::cargo_workspace(root.path()).unwrap())
            .args([
                "build",
                "--offline",
                "--features",
                "diagnostics",
                "--target-dir",
            ])
            .arg(&target);
        let job = Arc::new(Mutex::new(Job::default()));
        let ctx = egui::Context::default();
        let run_fixture = |build: &mut Command| {
            run(&job, &ctx, build)
                .unwrap_or_else(|e| panic!("{e}\n{}", job.lock().unwrap().log.preview()));
        };
        let artifacts = crate::cargo_artifacts(&job, &ctx, &mut cargo).unwrap();
        assert!(artifacts.iter().any(|a| a["executable"]
            .as_str()
            .is_some_and(|s| Path::new(s) == exe)));
        assert_eq!(inspect(&exe).unwrap(), None);
        relink(&job, &ctx, &cargo, &target).unwrap();
        let sdk = inspect(&exe).unwrap().unwrap();
        assert_eq!(sdk.version, SDK);
        assert_eq!(sdk.directory, Path::new(r".\D3D12"));
        assert_eq!(inspect(&profile.join("helper.exe")).unwrap(), None);
        assert_eq!(fs::read_to_string(src.join("main.rs")).unwrap(), main);

        // Optional offline smoke check using Microsoft's actual pinned redistributable.
        if let Some(core) = std::env::var_os("PALMVI_TEST_AGILITY_CORE") {
            let bytes = fs::read(core).unwrap();
            assert!(pinned(&bytes) && matches_version(&bytes, SDK));
            let cache = root.path().join("cache");
            fs::create_dir_all(cache.join("1.620.2-x64")).unwrap();
            fs::write(cache.join("1.620.2-x64/D3D12Core.dll"), &bytes).unwrap();
            stage_runtime(&exe, &sdk, &[root.path()], &cache).unwrap();
            assert_eq!(
                fs::read(profile.join("D3D12/D3D12Core.dll")).unwrap(),
                bytes
            );
        }

        cargo.args(["--features", "agility"]);
        run_fixture(&mut cargo);
        let declared = inspect(&exe).unwrap().unwrap();
        assert_eq!(
            declared,
            Declaration {
                version: 619,
                directory: ".".into()
            }
        );
        let dll = fs::read(profile.join("victory.dll")).unwrap();
        assert!(matches_version(&dll, 619) && !matches_version(&dll, 620));
        fs::create_dir(root.path().join("D3D12")).unwrap();
        fs::write(root.path().join("D3D12/D3D12Core.dll"), &dll).unwrap();
        let cache = root.path().join("unused-cache");
        stage_runtime(&exe, &declared, &[&workspace, root.path()], &cache).unwrap();
        assert_eq!(fs::read(profile.join("D3D12Core.dll")).unwrap(), dll);
        assert!(stage_runtime(&exe, &declared, &[root.path()], &cache)
            .unwrap()
            .contains("Verified existing"));
        assert!(!cache.exists());
    }
}
