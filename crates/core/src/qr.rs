//! QR code decoding from an image (PNG / JPEG), locally — no network.
//! Errors never contain the decoded text (it is a share link with secrets).

use std::io::Cursor;

use image::{DynamicImage, ImageReader, Limits};

use crate::error_code::ErrorCode;

/// Largest accepted image file.
pub const MAX_IMAGE_BYTES: usize = 15 * 1024 * 1024;
/// Largest accepted image dimension (decompression-bomb guard).
const MAX_DIMENSION: u32 = 10_000;
/// Images are downscaled to at most this many pixels on the longest side
/// before detection (keeps detection fast on full-resolution photos).
const DETECT_MAX_SIDE: u32 = 2_400;

/// Decode exactly one QR code from `bytes` and return its text.
pub fn decode_qr(bytes: &[u8]) -> Result<String, ErrorCode> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err(ErrorCode::QrImageUnreadable);
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ErrorCode::QrImageUnreadable)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|_| ErrorCode::QrImageUnreadable)?;
    decode_image(img)
}

fn decode_image(img: DynamicImage) -> Result<String, ErrorCode> {
    let img = if img.width().max(img.height()) > DETECT_MAX_SIDE {
        img.resize(DETECT_MAX_SIDE, DETECT_MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let luma = img.to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(luma);
    let mut texts: Vec<String> = prepared
        .detect_grids()
        .iter()
        .filter_map(|g| g.decode().ok().map(|(_, text)| text))
        .collect();
    texts.sort();
    texts.dedup();
    match texts.len() {
        0 => Err(ErrorCode::QrNotFound),
        1 => Ok(texts.remove(0)),
        _ => Err(ErrorCode::QrTooManyCodes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImage, ImageBuffer, ImageFormat, Luma};

    const FAKE_LINK: &str = "hy2://FAKE-pass@proxy.example.com:24443/?sni=proxy.example.com#QR-Test";

    fn qr_image(text: &str) -> ImageBuffer<Luma<u8>, Vec<u8>> {
        qrcode::QrCode::new(text.as_bytes()).unwrap().render::<Luma<u8>>().module_dimensions(6, 6).build()
    }

    fn encode(img: &DynamicImage, fmt: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, fmt).unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_png_and_jpeg() {
        let img = DynamicImage::ImageLuma8(qr_image(FAKE_LINK));
        assert_eq!(decode_qr(&encode(&img, ImageFormat::Png)).unwrap(), FAKE_LINK);
        let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
        assert_eq!(decode_qr(&encode(&rgb, ImageFormat::Jpeg)).unwrap(), FAKE_LINK);
    }

    #[test]
    fn decodes_small_code_in_large_screenshot() {
        let code = qr_image(FAKE_LINK);
        let mut canvas = ImageBuffer::from_pixel(3000, 2000, Luma([255u8]));
        canvas.copy_from(&code, 1200, 700).unwrap();
        let png = encode(&DynamicImage::ImageLuma8(canvas), ImageFormat::Png);
        assert_eq!(decode_qr(&png).unwrap(), FAKE_LINK);
    }

    #[test]
    fn two_different_codes_are_rejected_same_code_twice_is_fine() {
        let a = qr_image(FAKE_LINK);
        let b = qr_image("vless://00000000-0000-4000-8000-0000000000aa@203.0.113.7:443?security=reality");
        let w = a.width() + b.width() + 60;
        let h = a.height().max(b.height()) + 40;
        let mut canvas = ImageBuffer::from_pixel(w, h, Luma([255u8]));
        canvas.copy_from(&a, 20, 20).unwrap();
        canvas.copy_from(&b, a.width() + 40, 20).unwrap();
        assert_eq!(decode_qr(&encode(&DynamicImage::ImageLuma8(canvas), ImageFormat::Png)), Err(ErrorCode::QrTooManyCodes));

        let mut twice = ImageBuffer::from_pixel(a.width() * 2 + 60, a.height() + 40, Luma([255u8]));
        twice.copy_from(&a, 20, 20).unwrap();
        twice.copy_from(&a, a.width() + 40, 20).unwrap();
        assert_eq!(decode_qr(&encode(&DynamicImage::ImageLuma8(twice), ImageFormat::Png)).unwrap(), FAKE_LINK);
    }

    #[test]
    fn blank_garbage_and_oversized() {
        let blank = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(400, 400, Luma([255u8])));
        assert_eq!(decode_qr(&encode(&blank, ImageFormat::Png)), Err(ErrorCode::QrNotFound));
        assert_eq!(decode_qr(b"definitely not an image"), Err(ErrorCode::QrImageUnreadable));
        assert_eq!(decode_qr(&[]), Err(ErrorCode::QrImageUnreadable));
        assert_eq!(decode_qr(&vec![0u8; MAX_IMAGE_BYTES + 1]), Err(ErrorCode::QrImageUnreadable));
    }
}
