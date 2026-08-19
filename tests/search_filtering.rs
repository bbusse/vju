use vju::{clamp_selected, filter_indices};

fn items(strs: &[&str]) -> Vec<String> {
    strs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn empty_query_matches_everything_in_order() {
    let data = items(&["apple", "banana", "cherry"]);
    assert_eq!(filter_indices(&data, ""), vec![0, 1, 2]);
}

#[test]
fn substring_match_is_case_insensitive() {
    let data = items(&["Apple", "banana", "Cherry"]);
    assert_eq!(filter_indices(&data, "AP"), vec![0]);
    assert_eq!(filter_indices(&data, "an"), vec![1]);
}

#[test]
fn no_matches_returns_empty() {
    let data = items(&["apple", "banana"]);
    assert!(filter_indices(&data, "zzz").is_empty());
}

#[test]
fn clamp_keeps_in_range_selection_unchanged() {
    assert_eq!(clamp_selected(1, 3), 1);
}

#[test]
fn clamp_pulls_back_out_of_range_selection_to_last_index() {
    assert_eq!(clamp_selected(5, 3), 2);
}

#[test]
fn clamp_resets_to_zero_when_nothing_filtered() {
    assert_eq!(clamp_selected(2, 0), 0);
}
