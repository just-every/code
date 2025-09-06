mod _shared;
use icy_sixel::{
    sixel_string, DiffusionMethod, MethodForLargest, MethodForRep, PixelFormat, Quality,
};

fn main() {
    // Load and decode the PNG file
    let (img_rgb888, width, height) = _shared::load_png_as_rgb888("examples/snake.png");

    // Encode as SIXEL data
    let sixel_data = sixel_string(
        &img_rgb888,
        width as i32,
        height as i32,
        PixelFormat::RGB888,
        DiffusionMethod::Auto, // Auto, None, Atkinson, FS, JaJuNi, Stucki, Burkes, ADither, XDither
        MethodForLargest::Auto, // Auto, Norm, Lum
        MethodForRep::Auto,    // Auto, CenterBox, AverageColors, Pixels
        Quality::HIGH,         // AUTO, HIGH, LOW, FULL, HIGHCOLOR
    )
    .expect("Failed to encode image to SIXEL format");

    // Save to file and output to terminal
    _shared::save_sixel_to_file(&sixel_data, "examples/snake.six");
    print!("Sixel data output (if your terminal supports it):\n{sixel_data}");

    // Compare with reference SIXEL data
    let reference_sixel = _shared::load_reference_sixel("examples/snake_libsixel.six");
    print!("Compared with the same snake image converted using libsixel:\n{reference_sixel}");
}
