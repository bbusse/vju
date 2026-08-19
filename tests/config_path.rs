use vju::find_config_path;

// This test changes the process's current directory, which is global state shared by every
// thread in the process. Integration test files each run as their own binary/process, so this
// doesn't race against other test files -- but keep this file to a single #[test] so it can't
// race against itself.
#[test]
fn finds_config_in_current_directory() {
    let dir = std::env::temp_dir().join(format!("vju_test_cfg_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    std::fs::write(dir.join("vju-config.toml"), "# test config\n").unwrap();
    let found = find_config_path();

    std::env::set_current_dir(&original_cwd).unwrap();
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(found.as_deref(), Some("vju-config.toml"));
}
