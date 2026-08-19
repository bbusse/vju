use vju::parse_hex_color;

#[test]
fn six_digit_hex_with_hash() {
    let c = parse_hex_color("#ff9900").expect("valid hex");
    assert_eq!((c.r(), c.g(), c.b(), c.a()), (0xff, 0x99, 0x00, 0xff));
}

#[test]
fn six_digit_hex_without_hash() {
    let c = parse_hex_color("ff9900").expect("valid hex");
    assert_eq!((c.r(), c.g(), c.b(), c.a()), (0xff, 0x99, 0x00, 0xff));
}

#[test]
fn eight_digit_hex_with_alpha() {
    let c = parse_hex_color("#11223344").expect("valid hex");
    assert_eq!((c.r(), c.g(), c.b(), c.a()), (0x11, 0x22, 0x33, 0x44));
}

#[test]
fn invalid_length_is_none() {
    assert!(parse_hex_color("#fff").is_none());
    assert!(parse_hex_color("#ffffffff0").is_none());
    assert!(parse_hex_color("").is_none());
}

#[test]
fn invalid_hex_digits_is_none() {
    assert!(parse_hex_color("#zzzzzz").is_none());
}
