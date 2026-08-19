use image::{DynamicImage, GenericImageView};
use vju::resize_image_to_fit;

#[test]
fn downscales_to_fit_width() {
    let img = DynamicImage::new_rgb8(2000, 1000);
    let (resized, size) = resize_image_to_fit(&img, 1000, u32::MAX);
    assert_eq!(size, [1000, 500]); // half-scale, aspect preserved
    assert_eq!(resized.dimensions(), (1000, 500));
}

#[test]
fn downscales_to_fit_height() {
    let img = DynamicImage::new_rgb8(1000, 2000);
    let (resized, size) = resize_image_to_fit(&img, u32::MAX, 1000);
    assert_eq!(size, [500, 1000]);
    assert_eq!(resized.dimensions(), (500, 1000));
}

#[test]
fn never_upscales_smaller_images() {
    let img = DynamicImage::new_rgb8(100, 50);
    let (resized, size) = resize_image_to_fit(&img, 1000, 1000);
    assert_eq!(size, [100, 50]);
    assert_eq!(resized.dimensions(), (100, 50));
}

#[test]
fn constrained_by_the_tighter_dimension() {
    // 2000x100 image, max 1000x1000: width needs to shrink by half, height already fits.
    // The tighter (width) constraint should win, keeping aspect ratio.
    let img = DynamicImage::new_rgb8(2000, 100);
    let (_, size) = resize_image_to_fit(&img, 1000, 1000);
    assert_eq!(size, [1000, 50]);
}
