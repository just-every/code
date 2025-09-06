// shared examples functionality

use image::ImageFormat;
use std::{
    fs::File,
    io::{BufReader, Read, Write},
    path::Path,
};

/// Loads a PNG file and converts it to RGB888 format.
pub fn load_png_as_rgb888(file_path: &str) -> (Vec<u8>, u32, u32) {
    let file = File::open(file_path).expect(&format!(
        "Error loading `{}`. Please run the example from the root of the crate.",
        file_path
    ));
    let reader = BufReader::new(file);
    let img = image::load(reader, ImageFormat::Png).expect("Failed to load PNG image");
    let img_rgb8 = img.to_rgb8(); // Discard alpha channel if present
    let (width, height) = img_rgb8.dimensions();
    let img_rgb888 = img_rgb8.into_raw();
    (img_rgb888, width, height)
}

/// Saves SIXEL data to a specified file.
pub fn save_sixel_to_file(sixel_data: &str, file_path: &str) {
    let mut file = File::create(Path::new(file_path)).expect("Failed to create SIXEL file");
    file.write_all(sixel_data.as_bytes())
        .expect("Failed to write SIXEL data to file");
    println!("File saved as `{}`", file_path);
}

/// Loads a reference SIXEL file for comparison and returns its contents.
pub fn load_reference_sixel(file_path: &str) -> String {
    let file = File::open(file_path).expect(&format!(
        "Error loading reference SIXEL file `{}`",
        file_path
    ));
    let mut reader = BufReader::new(file);
    let mut sixel_data = String::new();
    reader
        .read_to_string(&mut sixel_data)
        .expect("Reference file is not valid UTF-8");
    sixel_data
}
