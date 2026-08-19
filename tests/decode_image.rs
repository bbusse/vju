use vju::decode_image;

fn red_rect_svg(w: u32, h: u32) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"#ff0000\"/></svg>",
        w = w,
        h = h
    )
}

#[test]
fn decodes_svg_and_scales_to_fit() {
    let svg = red_rect_svg(200, 100);
    let (color_image, size) = decode_image(svg.as_bytes(), 100, u32::MAX).expect("valid svg");
    assert_eq!(size, [100, 50]); // half-scale, aspect preserved
    assert_eq!(color_image.size, size);
    // Solid red rect: every pixel should be pure red, fully opaque.
    for pixel in &color_image.pixels {
        assert_eq!((pixel.r(), pixel.g(), pixel.b(), pixel.a()), (0xff, 0, 0, 0xff));
    }
}

#[test]
fn svg_is_never_upscaled_past_intrinsic_size() {
    let svg = red_rect_svg(50, 25);
    let (_, size) = decode_image(svg.as_bytes(), 1000, 1000).expect("valid svg");
    assert_eq!(size, [50, 25]);
}

#[test]
fn decodes_raster_png() {
    // A blank 20x10 PNG, encoded in-memory via the `image` crate (same path App::new relies on).
    let img = image::DynamicImage::new_rgb8(20, 10);
    let mut bytes: Vec<u8> = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();

    let (color_image, size) = decode_image(&bytes, 10, u32::MAX).expect("valid png");
    assert_eq!(size, [10, 5]); // downscaled by half, aspect preserved
    assert_eq!(color_image.size, size);
}

#[test]
fn garbage_bytes_decode_to_none() {
    assert!(decode_image(b"not an image or svg", 100, 100).is_none());
}
