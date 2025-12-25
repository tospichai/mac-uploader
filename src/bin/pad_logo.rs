use image::{ImageBuffer, Rgba};
use std::path::Path;

fn main() {
    let input_path = "../frontend/public/logo.png";
    let output_path = "assets/logo_padded.png";

    // Load original image
    let img = image::open(input_path).expect("Failed to open input image");
    let (width, height) = (img.width(), img.height());

    // Calculate new dimensions (add ~25% padding)
    let padding_factor = 1.25;
    let new_width = (width as f32 * padding_factor) as u32;
    let new_height = (height as f32 * padding_factor) as u32;

    // Create new transparent image
    let mut new_img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(new_width, new_height);

    // Calculate offset to center the original image
    let x_offset = (new_width - width) / 2;
    let y_offset = (new_height - height) / 2;

    // Paste original image onto new canvas
    image::imageops::overlay(&mut new_img, &img, x_offset as i64, y_offset as i64);

    // Save
    new_img.save(output_path).expect("Failed to save output image");
    println!("Created padded logo at: {}", output_path);
}
