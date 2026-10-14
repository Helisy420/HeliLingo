//! PDF pages as images, with Windows' own PDF renderer (Windows.Data.Pdf,
//! the one Edge uses) — no PDF library. «Перевод файлов» renders each page
//! and reads it with `ocr::recognize`, so scanned PDFs work too.

use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    /// Not a PDF, damaged, or protected by a password.
    Unreadable,
    Failed(String),
}

/// An opened PDF.
pub struct Pdf {
    doc: PdfDocument,
}

impl Pdf {
    /// Opens a PDF from its bytes. Blocking.
    pub fn open(bytes: &[u8]) -> Result<Pdf, PdfError> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let failed = |e: windows::core::Error| PdfError::Failed(e.message());
        let stream = InMemoryRandomAccessStream::new().map_err(failed)?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(failed)?;
        writer.WriteBytes(bytes).map_err(failed)?;
        writer.StoreAsync().map_err(failed)?.join().map_err(failed)?;
        writer.DetachStream().map_err(failed)?;
        stream.Seek(0).map_err(failed)?;
        let doc = PdfDocument::LoadFromStreamAsync(&stream)
            .map_err(|_| PdfError::Unreadable)?
            .join()
            .map_err(|_| PdfError::Unreadable)?;
        Ok(Pdf { doc })
    }

    pub fn pages(&self) -> u32 {
        self.doc.PageCount().unwrap_or(0)
    }

    /// Page `index` (0-based) as PNG, `width` pixels wide. Blocking.
    pub fn page_png(&self, index: u32, width: u32) -> Result<Vec<u8>, PdfError> {
        let failed = |e: windows::core::Error| PdfError::Failed(e.message());
        let page = self.doc.GetPage(index).map_err(failed)?;
        let stream = InMemoryRandomAccessStream::new().map_err(failed)?;
        let options = PdfPageRenderOptions::new().map_err(failed)?;
        options.SetDestinationWidth(width).map_err(failed)?;
        // PNG is the default encoder.
        page.RenderWithOptionsToStreamAsync(&stream, &options).map_err(failed)?.join().map_err(failed)?;
        let size = stream.Size().map_err(failed)? as u32;
        stream.Seek(0).map_err(failed)?;
        let reader = DataReader::CreateDataReader(&stream).map_err(failed)?;
        reader.LoadAsync(size).map_err(failed)?.join().map_err(failed)?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).map_err(failed)?;
        Ok(bytes)
    }
}
