use vju::{hint_text_for_mode, Mode};

#[test]
fn select_mode_hint_mentions_navigation_and_quit() {
    let hint = hint_text_for_mode(&Mode::Select, 0);
    assert!(hint.contains("Enter select"));
    assert!(hint.contains("quit"));
}

#[test]
fn view_and_single_image_mode_hints_just_mention_quit() {
    assert_eq!(hint_text_for_mode(&Mode::View, 0), "Esc/q quit");
    assert_eq!(hint_text_for_mode(&Mode::Image, 0), "Esc/q quit");
    assert_eq!(hint_text_for_mode(&Mode::Image, 1), "Esc/q quit");
}

#[test]
fn multi_image_mode_hint_mentions_navigation() {
    let hint = hint_text_for_mode(&Mode::Image, 2);
    assert!(hint.contains("prev"));
    assert!(hint.contains("next"));
    assert!(hint.contains("quit"));
}
