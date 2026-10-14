//! Renders every page of a PDF and prints what OCR reads there:
//! `cargo run -p hl-win --example pdf_ocr -- file.pdf [lang]`

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    let pdf = hl_win::pdf::Pdf::open(&bytes).unwrap();
    println!("{} pages", pdf.pages());
    for i in 0..pdf.pages() {
        let t = std::time::Instant::now();
        let png = pdf.page_png(i, 1700).unwrap();
        let r = hl_win::ocr::recognize(&png, args.get(2).map(String::as_str)).unwrap();
        println!("page {} ({} KB png, {} ms, {}):", i + 1, png.len() / 1024, t.elapsed().as_millis(), r.language);
        for b in r.blocks {
            println!("  {:?} {}", (b.bounds.x as i32, b.bounds.y as i32, b.bounds.w as i32, b.bounds.h as i32), b.text);
        }
    }
}
