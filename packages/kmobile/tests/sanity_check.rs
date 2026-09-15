use kmobile::utils::{parse_coordinates, sanitize_filename};

#[test]
fn test_sanitize_filename_replaces_invalid_chars() {
    assert_eq!(sanitize_filename("a/b:c"), "a_b_c");
}

#[test]
fn test_parse_coordinates_valid_input() {
    let result = parse_coordinates("100,200").unwrap();
    assert_eq!(result, (100, 200));
}

#[test]
fn test_parse_coordinates_negative_values() {
    let result = parse_coordinates("-50,-75").unwrap();
    assert_eq!(result, (-50, -75));
}
