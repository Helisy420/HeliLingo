//! Raster images for the main window's Images tab: decoding through
//! Windows.Graphics.Imaging (WIC codecs: PNG, JPEG, WEBP, BMP, GIF, TIFF),
//! images and copied files on the clipboard, the open/save dialogs, PNG
//! encoding, and putting a picture on the clipboard.

use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, BitmapTransform, ColorManagementMode,
    ExifOrientationMode, SoftwareBitmap,
};
use windows::Storage::Streams::{Buffer, DataReader, DataWriter, InMemoryRandomAccessStream};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    DragQueryFileW, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FileOpenDialog, FileSaveDialog, HDROP,
    IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, PCWSTR};

use super::Hwnd;

/// Largest image file we accept ("up to 20 MB").
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// File extensions offered in the open dialog and accepted from drops.
pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "jfif"];

/// An opaque picture: straight RGBA, row by row, top to bottom.
#[derive(Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RgbaImage {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y.min(self.height - 1) * self.width + x.min(self.width - 1)) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// The part inside `x, y, w, h` (clamped to the picture).
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        let w = w.clamp(1, self.width - x);
        let h = h.clamp(1, self.height - y);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let start = ((row * self.width + x) * 4) as usize;
            rgba.extend_from_slice(&self.rgba[start..start + (w * 4) as usize]);
        }
        RgbaImage { width: w, height: h, rgba }
    }

    /// Bilinear resize (used to fit OCR's size limits).
    pub fn resized(&self, w: u32, h: u32) -> RgbaImage {
        let (w, h) = (w.max(1), h.max(1));
        let sx = self.width as f32 / w as f32;
        let sy = self.height as f32 / h as f32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let fy = ((y as f32 + 0.5) * sy - 0.5).max(0.0);
            let (y0, ty) = (fy.floor() as u32, fy.fract());
            let y1 = (y0 + 1).min(self.height - 1);
            for x in 0..w {
                let fx = ((x as f32 + 0.5) * sx - 0.5).max(0.0);
                let (x0, tx) = (fx.floor() as u32, fx.fract());
                let x1 = (x0 + 1).min(self.width - 1);
                let (a, b, c, d) = (self.pixel(x0, y0), self.pixel(x1, y0), self.pixel(x0, y1), self.pixel(x1, y1));
                let o = ((y * w + x) * 4) as usize;
                for k in 0..4 {
                    let top = a[k] as f32 * (1.0 - tx) + b[k] as f32 * tx;
                    let bottom = c[k] as f32 * (1.0 - tx) + d[k] as f32 * tx;
                    rgba[o + k] = (top * (1.0 - ty) + bottom * ty).round() as u8;
                }
            }
        }
        RgbaImage { width: w, height: h, rgba }
    }

    /// The pixels as BGRA (Windows bitmaps).
    pub fn to_bgra(&self) -> Vec<u8> {
        let mut out = self.rgba.clone();
        for px in out.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        out
    }

    /// PNG file bytes.
    pub fn encode_png(&self) -> Option<Vec<u8>> {
        use resvg::tiny_skia::{IntSize, Pixmap};
        // Opaque, so straight == premultiplied.
        let pixmap = Pixmap::from_vec(self.rgba.clone(), IntSize::from_wh(self.width, self.height)?)?;
        pixmap.encode_png().ok()
    }
}

/// Why a picture could not be opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// Larger than [`MAX_BYTES`].
    TooLarge,
    /// Not a format Windows can decode (or a broken file).
    Unsupported,
    /// The file could not be read.
    Unreadable,
}

/// Joins the WinRT multithreaded apartment on worker threads (harmless if
/// the thread already has one).
pub fn ensure_mta() {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
}

/// Decodes an image file in memory. Respects the EXIF orientation of
/// photos; transparent areas are composited over white (like a page).
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, LoadError> {
    ensure_mta();
    decode_winrt(bytes).map_err(|_| LoadError::Unsupported)
}

fn decode_winrt(bytes: &[u8]) -> windows::core::Result<RgbaImage> {
    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.join()?;
    writer.FlushAsync()?.join()?;
    writer.DetachStream()?;
    stream.Seek(0)?;
    let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
    let bitmap = decoder
        .GetSoftwareBitmapTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Straight,
            &BitmapTransform::new()?,
            ExifOrientationMode::RespectExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )?
        .join()?;
    let (w, h) = (bitmap.PixelWidth()? as u32, bitmap.PixelHeight()? as u32);
    let len = w * h * 4;
    let buffer = Buffer::Create(len)?;
    bitmap.CopyToBuffer(&buffer)?;
    let reader = DataReader::FromBuffer(&buffer)?;
    let mut bgra = vec![0u8; len as usize];
    reader.ReadBytes(&mut bgra)?;
    Ok(opaque_from_bgra(w, h, bgra))
}

/// BGRA (straight alpha) → opaque RGBA over white. Bitmaps whose alpha is
/// all zero (32-bit DIBs that don't use it) are taken as opaque.
fn opaque_from_bgra(width: u32, height: u32, mut px: Vec<u8>) -> RgbaImage {
    let alpha_unused = px.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
    for p in px.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        let a = if alpha_unused { 255 } else { p[3] as u32 };
        if a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 255 * (255 - a)) / 255) as u8;
            }
        }
        p[3] = 255;
    }
    RgbaImage { width, height, rgba: px }
}

/// A WinRT bitmap of the picture (for OCR).
pub fn software_bitmap(img: &RgbaImage) -> windows::core::Result<SoftwareBitmap> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(&img.to_bgra())?;
    let buffer = writer.DetachBuffer()?;
    SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, img.width as i32, img.height as i32)
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Reads and decodes an image file (at most [`MAX_BYTES`]).
pub fn load_file(path: &Path) -> Result<RgbaImage, LoadError> {
    let len = std::fs::metadata(path).map_err(|_| LoadError::Unreadable)?.len();
    if len > MAX_BYTES {
        return Err(LoadError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|_| LoadError::Unreadable)?;
    decode(&bytes)
}

// ------------------------------------------------------------- clipboard

/// What Ctrl+V can give the Images tab.
pub enum Pasted {
    /// An encoded image (PNG, or a DIB wrapped as a BMP file).
    Bytes(Vec<u8>),
    /// A file copied in Explorer.
    File(PathBuf),
}

struct Open;

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn open_clipboard() -> Option<Open> {
    for _ in 0..25 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return Some(Open);
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    None
}

fn register(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(PCWSTR(HSTRING::from(name).as_ptr())) }
}

fn available(format: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

fn read_format(format: u32) -> Option<Vec<u8>> {
    unsafe {
        let handle = GetClipboardData(format).ok()?;
        let hg = HGLOBAL(handle.0);
        let ptr = GlobalLock(hg) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, GlobalSize(hg)).to_vec();
        let _ = GlobalUnlock(hg);
        Some(bytes)
    }
}

fn write_format(format: u32, bytes: &[u8]) -> bool {
    unsafe {
        let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else {
            return false;
        };
        let ptr = GlobalLock(hg) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(hg));
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(hg);
        if SetClipboardData(format, Some(HANDLE(hg.0))).is_err() {
            let _ = GlobalFree(Some(hg));
            return false;
        }
        true
    }
}

/// Ctrl+V held right now (egui only reports pastes of text).
pub fn ctrl_v_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_V};
    unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 && GetAsyncKeyState(VK_V.0 as i32) < 0 }
}

/// The image on the clipboard: a copied image file first, then the "PNG"
/// format (keeps transparency), then DIBs.
pub fn clipboard_image() -> Option<Pasted> {
    let _open = open_clipboard()?;
    if available(CF_HDROP.0 as u32)
        && let Some(path) = dropped_files().into_iter().find(|p| is_image_path(p))
    {
        return Some(Pasted::File(path));
    }
    let png = register("PNG");
    if available(png)
        && let Some(bytes) = read_format(png)
    {
        return Some(Pasted::Bytes(bytes));
    }
    for format in [CF_DIB.0 as u32, CF_DIBV5.0 as u32] {
        if available(format)
            && let Some(bmp) = read_format(format).and_then(|dib| dib_to_bmp(&dib))
        {
            return Some(Pasted::Bytes(bmp));
        }
    }
    None
}

/// Paths in the clipboard's CF_HDROP (clipboard must be open).
fn dropped_files() -> Vec<PathBuf> {
    unsafe {
        let Ok(handle) = GetClipboardData(CF_HDROP.0 as u32) else {
            return Vec::new();
        };
        let drop = HDROP(handle.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        (0..count)
            .filter_map(|i| {
                let len = DragQueryFileW(drop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                let n = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
                (n > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..n])))
            })
            .collect()
    }
}

/// A packed DIB (BITMAPINFO + bits) with a BITMAPFILEHEADER in front, so
/// the BMP decoder can read it.
pub fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |o: usize| dib.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0)? as usize;
    if header < 12 || dib.len() < header {
        return None;
    }
    let (bit_count, compression, colors_used) = if header == 12 {
        (u16::from_le_bytes([dib[10], dib[11]]) as u32, 0, 0)
    } else {
        (u16::from_le_bytes([dib[14], dib[15]]) as u32, u32_at(16)?, u32_at(32)?)
    };
    let masks = match (header, compression) {
        (40, 3) => 12, // BI_BITFIELDS after a BITMAPINFOHEADER
        (40, 6) => 16, // BI_ALPHABITFIELDS
        _ => 0,
    };
    let entry = if header == 12 { 3 } else { 4 };
    let colors = if colors_used > 0 {
        colors_used as usize
    } else if bit_count <= 8 {
        1 << bit_count
    } else {
        0
    };
    let offset = 14 + header + masks + colors * entry;
    let size = 14 + dib.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

/// Puts the picture on the clipboard as a DIB and as PNG.
pub fn copy_image(img: &RgbaImage) -> bool {
    let png = img.encode_png();
    let Some(_open) = open_clipboard() else { return false };
    unsafe {
        let _ = EmptyClipboard();
    }
    // BITMAPINFOHEADER, 32 bpp BI_RGB, top-down (negative height).
    let mut dib = Vec::with_capacity(40 + img.rgba.len());
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(img.width as i32).to_le_bytes());
    dib.extend_from_slice(&(-(img.height as i32)).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&(img.rgba.len() as u32).to_le_bytes());
    dib.extend_from_slice(&[0; 16]);
    dib.extend_from_slice(&img.to_bgra());
    let ok = write_format(CF_DIB.0 as u32, &dib);
    if let Some(png) = png {
        write_format(register("PNG"), &png);
    }
    ok
}

// --------------------------------------------------------------- dialogs

fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }
}

/// The native "Open" dialog for one image. Blocks; call on its own thread.
pub fn pick_image(owner: Hwnd, title: &str, filter_name: &str) -> Option<PathBuf> {
    init_com();
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let name = HSTRING::from(filter_name);
        let spec = HSTRING::from(
            EXTENSIONS.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";"),
        );
        let filters = [COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) }];
        dialog.SetFileTypes(&filters).ok()?;
        dialog.SetTitle(&HSTRING::from(title)).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_FORCEFILESYSTEM).ok()?;
        dialog.Show((!owner.is_null()).then(|| owner.raw())).ok()?;
        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// The native "Save as" dialog. `ext` is "png" or "txt". Blocks; call on
/// its own thread.
pub fn pick_save_path(owner: Hwnd, title: &str, file_name: &str, filter_name: &str, ext: &str) -> Option<PathBuf> {
    init_com();
    unsafe {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let name = HSTRING::from(filter_name);
        let spec = HSTRING::from(format!("*.{ext}"));
        let filters = [COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) }];
        dialog.SetFileTypes(&filters).ok()?;
        dialog.SetDefaultExtension(&HSTRING::from(ext)).ok()?;
        dialog.SetFileName(&HSTRING::from(file_name)).ok()?;
        dialog.SetTitle(&HSTRING::from(title)).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT).ok()?;
        dialog.Show((!owner.is_null()).then(|| owner.raw())).ok()?;
        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        s.map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_gets_a_file_header() {
        // 1x1, 32 bpp, BI_RGB.
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[1, 2, 3, 0]);
        let bmp = dib_to_bmp(&dib).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 54);
        assert_eq!(bmp.len(), 58);
    }

    #[test]
    fn unused_alpha_means_opaque() {
        let img = opaque_from_bgra(1, 1, vec![10, 20, 30, 0]);
        assert_eq!(img.rgba, vec![30, 20, 10, 255]);
        let half = opaque_from_bgra(1, 2, vec![0, 0, 0, 0, 0, 0, 0, 255]);
        assert_eq!(&half.rgba[..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn crop_and_resize() {
        let img = RgbaImage { width: 2, height: 2, rgba: (0..16).collect() };
        assert_eq!(img.crop(1, 1, 5, 5).rgba, vec![12, 13, 14, 15]);
        let big = img.resized(4, 4);
        assert_eq!((big.width, big.height, big.rgba.len()), (4, 4, 64));
    }
}
