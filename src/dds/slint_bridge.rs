use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

pub fn raw_rgba_to_slint(width: u32, height: u32, rgba_bytes: &[u8]) -> Image {
    let mut pixel_buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    pixel_buffer.make_mut_bytes().copy_from_slice(rgba_bytes);
    Image::from_rgba8(pixel_buffer)
}
