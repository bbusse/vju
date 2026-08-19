use clap::Parser;
use vju::Args;

#[test]
fn defaults_when_no_flags() {
    let args = Args::parse_from(["vju"]);
    assert_eq!(args.r#type, None);
    assert!(!args.image);
    assert!(!args.hint);
    assert!(!args.debug);
    assert_eq!(args.font_scale, 1.0);
    assert_eq!(args.font_scale_highlight, 1.6);
}

#[test]
fn type_select_flag_parses() {
    let args = Args::parse_from(["vju", "--type", "select"]);
    assert_eq!(args.r#type.as_deref(), Some("select"));
}

#[test]
fn boolean_flags_parse() {
    let args = Args::parse_from(["vju", "--hint", "--debug", "--show-title", "--center-text"]);
    assert!(args.hint);
    assert!(args.debug);
    assert!(args.show_title);
    assert!(args.center_text);
}

#[test]
fn return_keys_takes_a_value() {
    let args = Args::parse_from(["vju", "--return-keys", "r,t,escape"]);
    assert_eq!(args.return_keys.as_deref(), Some("r,t,escape"));
}

#[test]
fn unknown_flag_is_rejected() {
    let result = Args::try_parse_from(["vju", "--not-a-real-flag"]);
    assert!(result.is_err());
}

#[test]
fn verbose_short_flag_counts_repeats() {
    let args = Args::parse_from(["vju", "-v", "-v"]);
    assert_eq!(args.verbose, 2);
}
