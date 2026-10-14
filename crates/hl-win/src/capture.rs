//! «Скриншот области»: copies a screen rectangle (physical pixels) and
//! encodes it as PNG for OCR. The overlay the user drags on is the app's
//! own window; it is hidden before the copy.

use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapEncoder, BitmapPixelFormat};
use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
};

/// BGRA pixels, top-down, of a screen rectangle (GDI BitBlt).
pub fn capture_rect(x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u8>> {
    if w <= 0 || h <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(Some(HWND::default()));
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap.into());
        let copied = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT).is_ok();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        SelectObject(mem, old);
        let rows = if copied {
            GetDIBits(mem, bitmap, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS)
        } else {
            0
        };
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(Some(HWND::default()), screen);
        (rows == h).then_some(px)
    }
}

/// BGRA pixels → PNG bytes (Windows' own encoder).
pub fn encode_png(w: u32, h: u32, bgra: &[u8]) -> Option<Vec<u8>> {
    let run = || -> windows::core::Result<Vec<u8>> {
        let stream = InMemoryRandomAccessStream::new()?;
        let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId()?, &stream)?.join()?;
        encoder.SetPixelData(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Ignore, w, h, 96.0, 96.0, bgra)?;
        encoder.FlushAsync()?.join()?;
        let size = stream.Size()? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        reader.LoadAsync(size)?.join()?;
        let mut out = vec![0u8; size as usize];
        reader.ReadBytes(&mut out)?;
        Ok(out)
    };
    run().ok()
}

/// A screen rectangle as PNG.
pub fn capture_png(x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u8>> {
    let px = capture_rect(x, y, w, h)?;
    encode_png(w as u32, h as u32, &px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_png() {
        let px = [0u8, 0, 255, 255].repeat(4 * 3);
        let png = encode_png(4, 3, &px).unwrap();
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
        // Width and height in the IHDR chunk.
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 4);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 3);
    }

    #[test]
    fn captures_the_screen_corner() {
        // Headless sessions may have no desktop to copy; only check sizes.
        if let Some(px) = capture_rect(0, 0, 8, 8) {
            assert_eq!(px.len(), 8 * 8 * 4);
        }
    }
}

/// How much of two same-size BGRA frames differs: the share of sampled
/// pixels (every 4th in each direction) whose colour moved by more than a
/// little. Text appearing, a dialog line advancing — not video noise.
pub fn difference(a: &[u8], b: &[u8], w: usize, h: usize) -> f32 {
    if a.len() != b.len() || a.len() < w * h * 4 || w == 0 || h == 0 {
        return 1.0;
    }
    let (mut seen, mut changed) = (0u32, 0u32);
    for y in (0..h).step_by(4) {
        for x in (0..w).step_by(4) {
            let i = (y * w + x) * 4;
            let d = (0..3).map(|k| (a[i + k] as i32 - b[i + k] as i32).abs()).max().unwrap_or(0);
            seen += 1;
            if d > 40 {
                changed += 1;
            }
        }
    }
    changed as f32 / seen.max(1) as f32
}

#[cfg(test)]
mod diff_tests {
    #[test]
    fn difference_counts_changed_samples() {
        let a = vec![0u8; 8 * 8 * 4];
        let mut b = a.clone();
        assert_eq!(super::difference(&a, &b, 8, 8), 0.0);
        // One sampled pixel (0, 0) of four changes.
        b[0] = 200;
        assert_eq!(super::difference(&a, &b, 8, 8), 0.25);
        assert_eq!(super::difference(&a, &b[..10], 8, 8), 1.0);
    }
}
