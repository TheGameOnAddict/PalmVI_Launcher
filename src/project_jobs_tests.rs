use super::*;

#[test]
fn independent_builds_share_neither_reservations_nor_logs() {
    let mut jobs = ProjectJobs::default();
    let first = jobs.start("Owner/first", true, "Building").unwrap();
    let second = jobs.start("Owner/second", true, "Updating").unwrap();
    assert!(jobs.start("owner/FIRST", true, "Reinstalling").is_none());
    assert!(jobs.start("Owner/second", false, "Playing").is_none());
    assert!(jobs.start("Owner/third", false, "Playing").is_some());
    assert!(jobs.start("Owner/fourth", false, "Playing").is_none());
    assert!(jobs.start("Owner/fourth", true, "Building").is_some());
    first.lock().unwrap().log.push("first build output".into());
    assert!(!second.lock().unwrap().log.preview().contains("first build output"));
    first.lock().unwrap().running = false;
    assert!(jobs.start("Owner/first", true, "Retry").is_some());
    assert!(second.lock().unwrap().running);
}

#[test]
#[ignore = "offline integration test requiring Git and Cargo on PATH"]
fn install_and_update_two_local_git_projects() {
    use std::fs;
    // Isolate the process-wide install folder from other tests, including on failure.
    const FIXTURE: &str = "MANDYHUB_TEST_PARALLEL_ROOT";
    let Some(directory) = std::env::var_os(FIXTURE) else {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("mandyhub-parallel-{nonce}"));
        fs::create_dir(&root).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        hide_window(&mut child);
        let result = child.env(FIXTURE, &root).env("LOCALAPPDATA", &root)
            .args(["--exact", "project_jobs_tests::install_and_update_two_local_git_projects", "--ignored", "--nocapture"]).output();
        fs::remove_dir_all(&root).unwrap();
        let result = result.unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    };
    let root = PathBuf::from(directory);
    let install_root = root.join("installed");
    set_install_dir(install_root.to_str().unwrap());
    let git = |dir: &Path, args: &[&str]| {
        let mut git = tools::git();
        hide_window(&mut git);
        let output = git.current_dir(dir)
            .args(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                   "-c", "commit.gpgsign=false", "-c", "core.hooksPath=.git/no-hooks"])
            .args(args).output().unwrap();
        assert!(output.status.success(), "git {args:?} in {dir:?}: {}\n{}\n{}", output.status,
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    let fixture = |name: &str| {
        let dir = root.join(name);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("Cargo.toml"), format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n")).unwrap();
        fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
        git(&dir, &["init", "-q"]);
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-qm", "Initial"]);
        Project { repo: url::Url::from_directory_path(&dir).unwrap().to_string(), name: name.into(), branch: None,
            gpu: vec![], fps: None, render: None, verified: false, summary: String::new(), custom: true,
            local_exe: None, archive_exe: None }
    };
    let first = fixture("fixture_a");
    let mut second = fixture("fixture_b");
    // PortWatch also lists standalone workspaces without a victory package.
    second.custom = false;
    let mut jobs = ProjectJobs::default();
    let ctx = egui::Context::default();
    ui::click_library_build(&first, "Install");
    let before = installed_revision(&first.repo).unwrap();
    git(&root.join(&first.name), &["commit", "--allow-empty", "-qm", "Update"]);
    let mut run_pair = |second_is_update| {
        let a = jobs.start(&first.repo, true, "Update").unwrap();
        let b = jobs.start(&second.repo, true, "Install or update").unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|threads| {
            let update = threads.spawn(|| { barrier.wait(); install(&a, &ctx, &first, true) });
            barrier.wait();
            install(&b, &ctx, &second, second_is_update).unwrap_or_else(|e| panic!("{e}\n{}", b.lock().unwrap().log.preview()));
            update.join().unwrap().unwrap_or_else(|e| panic!("{e}\n{}", a.lock().unwrap().log.preview()));
        });
        a.lock().unwrap().running = false;
        b.lock().unwrap().running = false;
        (a, b)
    };
    let (updating, installing) = run_pair(false);
    assert_ne!(installed_revision(&first.repo).unwrap(), before);
    assert!(updating.lock().unwrap().log.preview().contains("Downloading the new version"));
    assert!(installing.lock().unwrap().log.preview().contains("Downloading the project"));
    assert!(!installing.lock().unwrap().log.preview().contains("Downloading the new version"));
    for project in [&first, &second] {
        git(&root.join(&project.name), &["commit", "--allow-empty", "-qm", "Next"]);
    }
    run_pair(true);
    git(&root.join(&first.name), &["commit", "--allow-empty", "-qm", "Library update"]);
    ui::click_library_build(&first, "Update");
    for project in [&first, &second] {
        assert_eq!(installed_revision(&project.repo).unwrap(), git(&root.join(&project.name), &["rev-parse", "HEAD"]));
        let program = rust_install(&project.repo).unwrap().executable.unwrap();
        assert!(target_dir(&project.repo).join(program).is_file());
    }
}
