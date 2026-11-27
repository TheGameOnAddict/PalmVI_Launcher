use super::*;
use std::fs;

// Record what the child actually receives, including on a cache hit or failure.
const RECORDER: &str = r#"@echo off
if "%~2"=="--help" (
  echo --used
  exit /b 0
)
>>"%~dp0order.txt" echo %~1
>"%~dp0%~1.txt" echo cwd=%cd%
>>"%~dp0%~1.txt" echo arg1=%~1
>>"%~dp0%~1.txt" echo arg2=%~2
>>"%~dp0%~1.txt" echo arg3=%~3
>>"%~dp0%~1.txt" set VICTORY_
if "%~1"=="shaders" echo victory_gpu::host: 100.0 fps, 10.0 ms/frame
if "%~1"=="run" echo victory_gpu::host: 20.0 fps, 50.0 ms/frame
if "%~1"=="run" echo victory_gpu::host: 30.0 fps, 33.3 ms/frame 1>&2
if "%~1"=="warm" echo 0 pipelines known, 0 made before (fixture), 0 bad; 0 to make, 1 processes
if "%~1"=="shaders" (
  if exist "%~dp0fail" (
    echo 0 shaders in the cache, 2 do not translate, 1 s
    exit /b 0
  )
  if not exist "%VICTORY_CACHE_DIR%\shaders" mkdir "%VICTORY_CACHE_DIR%\shaders"
  echo fixture>"%VICTORY_CACHE_DIR%\shaders\fixture"
  echo 3 shaders in the cache, 1 do not translate, 1 s
)
exit /b 0
"#;

#[test]
fn global_warmup_and_play_share_the_launch_context() {
    check_launch(None);
}

/// Execute a locally reviewed upstream script against a recorder instead of a game.
#[test]
#[ignore = "set PALMVI_TEST_FPS_SCRIPT to a trusted local Scarlet-FPS or 3070 script"]
fn compare_3070_script_with_play() {
    let source = fs::read(std::env::var_os("PALMVI_TEST_FPS_SCRIPT").expect("script path")).unwrap();
    let root = std::env::temp_dir().join(format!("palmvi 3070 parity {}", std::process::id()));
    let game = root.join("game files");
    let release = root.join("target/release");
    let scripts = root.join("scripts");
    for folder in [&game, &release, &scripts] { fs::create_dir_all(folder).unwrap(); }
    let script = scripts.join("scarlet-fps.ps1");
    fs::write(&script, &source).unwrap();
    let (defaults, options) = script_preset::parse(std::str::from_utf8(&source).unwrap()).unwrap();
    assert!(options.scarlet_fps && options.hooks && options.ray_tracing && options.probe_rays);
    fs::write(game.join(EXE_NAME), []).unwrap();
    fs::write(game.join("shaders.rpf"), []).unwrap();
    let exe = release.join("victory.exe");
    let trace = release.join("trace.txt");
    let recorder = root.join("Recorder.cs");
    fs::write(&recorder, r#"using System;
using System.IO;
using System.Linq;
using System.Collections;
class Recorder {
    static void Main(string[] args) {
        var file = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "trace.txt");
        using (var writer = File.AppendText(file)) {
            writer.WriteLine("COMMAND");
            foreach (var arg in args) writer.WriteLine("arg=" + arg);
            foreach (var pair in Environment.GetEnvironmentVariables().Cast<DictionaryEntry>()
                     .Where(p => p.Key.ToString().StartsWith("VICTORY_", StringComparison.OrdinalIgnoreCase))
                     .OrderBy(p => p.Key.ToString(), StringComparer.OrdinalIgnoreCase))
                writer.WriteLine(pair.Key.ToString().ToUpperInvariant() + "=" + pair.Value);
        }
    }
}"#).unwrap();
    let csc = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("Microsoft.NET/Framework64/v4.0.30319/csc.exe");
    assert!(Command::new(csc).arg("/nologo").arg(format!("/out:{}", exe.display())).arg(recorder).status().unwrap().success());
    let project = Project { name: "Recorder".into(), repo: "local/3070-parity".into(), branch: None,
        gpu: vec![], fps: None, render: None, verified: false, summary: String::new(), custom: true,
        local_exe: Some(exe.display().to_string()), archive_exe: None };
    let cache = root.join("cache");
    let data = root.join("save");
    for flags in [&[][..], &["-Hooks"], &["-ProbeRays"], &["-RayTracing"], &["-ProbeRays", "-RayTracing"],
        &["-Console", "series-x"], &["-Prewarm"], &["-MipDrop", "0", "-CardBuffersMB", "768", "-CardWindowsMB", "384", "-AutoA", "20"]] {
        let _ = fs::remove_file(&trace);
        let result = Command::new("powershell.exe").current_dir(&root)
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script)
            .arg("-Game").arg(game.join(EXE_NAME)).args(flags)
            .env("VICTORY_CACHE_DIR", &cache).env("VICTORY_DATA_DIR", &data)
            .env("VICTORY_UNUSED_PARENT_FLAG", "1").output().unwrap();
        assert!(result.status.success(), "{flags:?} {} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        let reference = fs::read_to_string(&trace).unwrap();
        fs::remove_file(&trace).unwrap();
        let mut switches = format!("{defaults}\nVICTORY_CACHE_DIR={}\nVICTORY_DATA_DIR={}", cache.display(), data.display());
        for (flag, keys) in [("-Hooks", script_preset::HOOKS), ("-ProbeRays", options.probe_keys()), ("-RayTracing", script_preset::RAY_TRACING)] {
            if flags.contains(&flag) { script_preset::set_enabled(&mut switches, keys, true); }
        }
        if flags.contains(&"-MipDrop") {
            switches.push_str("\nVICTORY_TEXTURE_MIP_DROP=0\nVICTORY_CARD_BUFFERS_MB=768\nVICTORY_CARD_WINDOWS_MB=384\nVICTORY_AUTO_A=20");
        }
        let mut saved = Saved { game_dir: game.display().to_string(), warm_before_play: flags.contains(&"-Prewarm"),
            warm_pipelines: true, ..Saved::default() };
        saved.project_launch.insert(project.repo.clone(), ProjectLaunch { switches: Some(switches),
            args: flags.contains(&"-Console").then(|| "--console series-x".into()), script_options: Some(options), ..Default::default() });
        let job = Arc::new(Mutex::new(Job::default()));
        play(&job, &egui::Context::default(), &project, &saved.launch(&project.repo, false), &saves::LaunchSave::Continue).unwrap();
        assert_eq!(fs::read_to_string(&trace).unwrap(), reference, "script vs Play with {flags:?}");
        println!("Matched {flags:?} across {} child invocation(s)", reference.matches("COMMAND").count());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "set MANDYHUB_TEST_SCARLETT_SCRIPT to a trusted play-scarlett.ps1 to compare its child invocations"]
fn compare_installed_scarlett_script() {
    check_launch(Some(PathBuf::from(std::env::var_os("MANDYHUB_TEST_SCARLETT_SCRIPT").unwrap())));
}

#[test]
#[ignore = "requires MANDYHUB_TEST_VICTORY, MANDYHUB_TEST_GAME and MANDYHUB_TEST_SCARLETT_SCRIPT"]
fn warm_real_scarlett_cache() {
    let var = |key| PathBuf::from(std::env::var_os(key).unwrap());
    let script = var("MANDYHUB_TEST_SCARLETT_SCRIPT");
    let game = var("MANDYHUB_TEST_GAME");
    let exe = var("MANDYHUB_TEST_VICTORY");
    let root = std::env::temp_dir().join(format!("mandyhub-real-warm-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let mut command = Command::new(exe);
    command.current_dir(script.parent().unwrap().parent().unwrap());
    let (switches, _) = script_preset::parse(&fs::read_to_string(script).unwrap()).unwrap();
    for line in switches.lines() {
        let (key, value) = line.split_once('=').unwrap();
        command.env(key, value);
    }
    command.env("VICTORY_GAME_DIR", &game).env("VICTORY_CACHE_DIR", root.join("cache"))
        .env("VICTORY_DATA_DIR", root.join("save"));
    let launch = Saved { game_dir: game.display().to_string(), warm_before_play: true, warm_pipelines: true,
        ..Default::default() }.launch("fixture", true);
    let job = Arc::new(Mutex::new(Job::default()));
    warm(&job, &egui::Context::default(), &command, &launch).unwrap();
    let first = job.lock().unwrap().log.preview();
    println!("{}", first.lines().find(|s| s.starts_with("Shader totals:")).expect("VIctory shader totals"));
    println!("{}", first.lines().find(|s| s.starts_with("Pipeline inventory:")).expect("VIctory pipeline inventory"));
    warm(&job, &egui::Context::default(), &command, &launch).unwrap();
    assert!(job.lock().unwrap().log.preview().contains("cache current for this build"));
    fs::remove_dir_all(root).unwrap();
}

fn check_launch(direct_script: Option<PathBuf>) {
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("mandyhub launch comparison {id}"));
    let game = root.join("game files");
    let cache = root.join("cache");
    let data = root.join("save");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join(EXE_NAME), []).unwrap();
    fs::write(game.join("shaders.rpf"), []).unwrap();
    let recorder = root.join("victory.cmd");
    fs::write(&recorder, RECORDER.replace('\n', "\r\n")).unwrap();

    let mut defaults = "VICTORY_PROBE_RAYS=1".to_owned();
    let snapshot = |name: &str| fs::read_to_string(root.join(name)).unwrap();
    let reference = direct_script.map(|script| {
        let source = fs::read_to_string(script).unwrap();
        defaults = script_preset::parse(&source).unwrap().0;
        let script = root.join("scripts/play-scarlett.ps1");
        fs::create_dir(script.parent().unwrap()).unwrap();
        fs::write(&script, source).unwrap();
        let output = Command::new("powershell.exe").current_dir(&root)
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(script)
            .arg("-Game").arg(&game).arg("-Victory").arg(&recorder)
            .arg("-Cache").arg(&cache).arg("-Data").arg(&data).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["shaders", "run"]);
        fs::remove_file(root.join("order.txt")).unwrap();
        (snapshot("shaders.txt"), snapshot("run.txt"))
    });
    let p = Project {
        name: "fixture".into(), repo: format!("local/fixture-{id}"), branch: None, gpu: vec![],
        fps: None, render: None, verified: false, summary: String::new(), custom: true,
        local_exe: Some(recorder.to_str().unwrap().into()), archive_exe: None,
    };
    let mut saved = Saved {
        game_dir: game.to_str().unwrap().into(), warm_before_play: true, warm_pipelines: true,
        ..Saved::default()
    };
    saved.project_launch.insert(p.repo.clone(), ProjectLaunch {
        switches: Some(format!("{defaults}\nVICTORY_CACHE_DIR={}\nVICTORY_DATA_DIR={}", cache.display(), data.display())),
        script_options: Some(Default::default()), ..Default::default()
    });
    let launch = |saved: &Saved| {
        let job = Arc::new(Mutex::new(Job { play_stats: Some(saved.play_stats.clone()), ..Job::default() }));
        play(&job, &egui::Context::default(), &p, &saved.launch(&p.repo, true), &saves::LaunchSave::Continue).unwrap();
        let log = job.lock().unwrap().log.preview();
        log
    };
    let first = launch(&saved);
    assert!(first.contains("25.0 FPS average across 2 renderer readings"), "{first}");
    assert!(first.contains("Shader totals: 3 cached, 1 could not translate"));
    assert!(first.contains("Pipeline inventory: 0 pipelines known"));
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["shaders", "warm", "run"]);
    if let Some((shaders, run)) = reference {
        assert_eq!(snapshot("shaders.txt"), shaders);
        assert_eq!(snapshot("run.txt"), run);
    }
    fs::remove_file(root.join("order.txt")).unwrap();
    assert!(launch(&saved).contains("cache current for this build"));
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["warm", "run"]);
    fs::remove_file(root.join("order.txt")).unwrap();
    saved.project_launch.get_mut(&p.repo).unwrap().switches.as_mut().unwrap().push_str("\nVICTORY_PROBE_RAYS=0");
    fs::write(root.join("fail"), []).unwrap();
    let failed = launch(&saved);
    assert!(failed.contains("Warm-up failed"), "{failed}");
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["shaders", "run"]);
    fs::remove_file(root.join("order.txt")).unwrap();
    saved.warm_before_play = false;
    assert!(launch(&saved).contains("Warm-up before Play: off"));
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["run"]);
    fs::remove_dir_all(root).unwrap();
}
