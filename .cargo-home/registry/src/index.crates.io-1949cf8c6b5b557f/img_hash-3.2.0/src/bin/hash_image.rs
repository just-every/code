//! Hash an image and print the Base64 value

extern crate img_hash;
extern crate image;

use img_hash::HasherConfig;

use std::env;

fn main() -> Result<(), String> {
    let args = env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 2);

    let image = image::open(&args[1]).map_err(|e| {
        format!("failed to open {}: {}", &args[1], e)
    })?;

    let hash = HasherConfig::new().hash_size(8, 8).to_hasher().hash_image(&image);

    let hash_str = hash.as_bytes().iter().map(|b| format!("{:02x}", b)).collect::<String>();

    println!("{}: {}", &args[1], hash_str);

    Ok(())
}