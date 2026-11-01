use anyhow::{bail, Context, Result};
use image::{imageops::FilterType, GenericImageView};
use std::path::Path;

pub const SKIN_WIDTH: u32 = 64;
pub const SKIN_HEIGHT: u32 = 64;
pub const SKIN_BYTES: usize = (SKIN_WIDTH * SKIN_HEIGHT * 4) as usize;

pub fn blank() -> Vec<u8> {
    vec![0; SKIN_BYTES]
}

pub fn load_png(path: &Path) -> Result<Vec<u8>> {
    let image = image::open(path).with_context(|| format!("Не удалось открыть изображение {}", path.display()))?;
    let (width, height) = image.dimensions();
    if width != 64 || (height != 64 && height != 32) {
        bail!("Ожидается Minecraft skin PNG размером 64×64 (или старый формат 64×32), получено {width}×{height}.");
    }

    let rgba = image.to_rgba8();
    let mut output = blank();
    for y in 0..height {
        for x in 0..SKIN_WIDTH {
            let pixel = rgba.get_pixel(x, y).0;
            let offset = ((y * SKIN_WIDTH + x) * 4) as usize;
            output[offset..offset + 4].copy_from_slice(&pixel);
        }
    }
    Ok(output)
}

pub fn save_png(path: &Path, pixels: &[u8]) -> Result<()> {
    if pixels.len() != SKIN_BYTES {
        bail!("Некорректный буфер текстуры скина.");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer(path, pixels, SKIN_WIDTH, SKIN_HEIGHT, image::ColorType::Rgba8)
        .with_context(|| format!("Не удалось сохранить скин в {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_skin_has_correct_size() {
        assert_eq!(blank().len(), SKIN_BYTES);
    }

    #[test]
    fn png_roundtrip_preserves_pixels() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let path = std::env::temp_dir().join(format!(
            "minty-skin-{}-{}.png",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let mut pixels = blank();
        pixels[0..4].copy_from_slice(&[12, 34, 56, 255]);
        pixels[SKIN_BYTES - 4..].copy_from_slice(&[200, 150, 100, 128]);

        save_png(&path, &pixels).unwrap();
        let loaded = load_png(&path).unwrap();
        assert_eq!(loaded, pixels);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_non_skin_png_dimensions() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let path = std::env::temp_dir().join(format!(
            "minty-invalid-skin-{}-{}.png",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        image::save_buffer(&path, &[0u8; 16], 2, 2, image::ColorType::Rgba8).unwrap();
        assert!(load_png(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
