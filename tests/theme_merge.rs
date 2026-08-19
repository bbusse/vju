use clap::Parser;
use vju::{Args, Theme};

fn parse_args(extra: &[&str]) -> Args {
    let mut argv = vec!["vju"];
    argv.extend_from_slice(extra);
    Args::parse_from(argv)
}

#[test]
fn defaults_are_sane() {
    let theme = Theme::default();
    assert_eq!(theme.default_width, Some(600));
    assert_eq!(theme.default_height, Some(500));
    assert_eq!(theme.highlight_color.as_deref(), Some("#ffffff"));
}

#[test]
fn cli_width_height_override_theme_defaults() {
    let args = parse_args(&["--width", "800", "--height", "400"]);
    let merged = Theme::default().merge_with_args(&args);
    assert_eq!(merged.default_width, Some(800));
    assert_eq!(merged.default_height, Some(400));
}

#[test]
fn theme_value_used_when_cli_not_provided() {
    let mut theme = Theme::default();
    theme.default_width = Some(1234);
    let args = parse_args(&[]);
    let merged = theme.merge_with_args(&args);
    assert_eq!(merged.default_width, Some(1234));
}

#[test]
fn background_color_cli_overrides_theme() {
    let mut theme = Theme::default();
    theme.background_color = Some("#111111".into());
    let args = parse_args(&["--background-color", "#222222"]);
    let merged = theme.merge_with_args(&args);
    assert_eq!(merged.background_color.as_deref(), Some("#222222"));
}

#[test]
fn low_contrast_flag_forces_true_but_never_forces_false() {
    // --low-contrast is a bool flag: it can only turn low_contrast on, never explicitly off,
    // so a theme file that already set it true stays true even without the flag.
    let mut theme = Theme::default();
    theme.low_contrast = Some(true);
    let merged = theme.merge_with_args(&parse_args(&[]));
    assert_eq!(merged.low_contrast, Some(true));

    let merged_with_flag = Theme::default().merge_with_args(&parse_args(&["--low-contrast"]));
    assert_eq!(merged_with_flag.low_contrast, Some(true));
}

#[test]
fn font_scale_highlight_sentinel_quirk() {
    // Known quirk: merge_with_args detects an explicit --font-scale-highlight by checking
    // whether it differs from clap's own default (1.6). Passing exactly 1.6 explicitly is
    // therefore indistinguishable from not passing the flag at all, so the theme's value wins.
    let mut theme = Theme::default();
    theme.font_scale_highlight = Some(2.0);

    let merged = theme.merge_with_args(&parse_args(&["--font-scale-highlight", "1.6"]));
    assert_eq!(merged.font_scale_highlight, Some(2.0), "quirk: explicit 1.6 looks like 'not passed'");

    // A genuinely different value does override, as expected.
    let merged2 = theme.merge_with_args(&parse_args(&["--font-scale-highlight", "2.5"]));
    assert_eq!(merged2.font_scale_highlight, Some(2.5));
}
