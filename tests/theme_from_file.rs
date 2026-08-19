use vju::Theme;

#[test]
fn parses_toml_theme_file_and_fills_missing_fields_from_default() {
    let dir = std::env::temp_dir().join(format!("vju_test_theme_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("theme.toml");
    let toml = "background_color = \"#112233\"\ndefault_width = 700\nlow_contrast = true\n";
    std::fs::write(&path, toml).unwrap();

    let theme = Theme::from_file(path.to_str().unwrap());
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(theme.background_color.as_deref(), Some("#112233"));
    assert_eq!(theme.default_width, Some(700));
    assert_eq!(theme.low_contrast, Some(true));
    // Fields absent from the file fall back to Theme::default()'s values (#[serde(default)]).
    assert_eq!(theme.default_height, Some(500));
}

#[test]
fn falls_back_to_default_when_file_is_missing() {
    let theme = Theme::from_file("/nonexistent/path/vju-does-not-exist.toml");
    assert_eq!(theme, Theme::default());
}

#[test]
fn falls_back_to_default_when_file_is_invalid_toml() {
    let dir = std::env::temp_dir().join(format!("vju_test_theme_bad_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.toml");
    std::fs::write(&path, "this is not valid toml {{{").unwrap();

    let theme = Theme::from_file(path.to_str().unwrap());
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(theme, Theme::default());
}
