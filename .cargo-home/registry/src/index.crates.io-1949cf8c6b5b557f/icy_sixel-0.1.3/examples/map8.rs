mod _shared;
use icy_sixel::{
    sixel_string, DiffusionMethod, MethodForLargest, MethodForRep, PixelFormat, Quality,
};

fn main() {
    // Load and decode the PNG file
    let (img_rgb888, width, height) = _shared::load_png_as_rgb888("examples/map8.png");

    // Encode as SIXEL data
    let sixel_data = sixel_string(
        &img_rgb888,
        width as i32,
        height as i32,
        PixelFormat::RGB888,
        DiffusionMethod::None,
        MethodForLargest::Auto,
        MethodForRep::Auto,
        Quality::AUTO,
    )
    .expect("Failed to encode image to SIXEL format");

    // Save to file and output to terminal
    _shared::save_sixel_to_file(&sixel_data, "examples/map8.six");
    print!("Sixel data output (if your terminal supports it):\n{sixel_data}");

    // Compare with reference SIXEL data
    let reference_sixel = _shared::load_reference_sixel("examples/map8_libsixel.six");
    print!("Compared with the same map8 image converted using libsixel:\n{reference_sixel}");
}
