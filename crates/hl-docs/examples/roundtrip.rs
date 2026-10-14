//! Reads a document, "translates" every unit to «[RU] text» and writes it
//! back — to check real files without a network:
//! `cargo run -p hl-docs --example roundtrip -- in.docx out-dir [bilingual]`

use std::path::Path;

use hl_docs::{Doc, Kind, Output, WriteOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, out_dir) = (Path::new(&args[1]), Path::new(&args[2]));
    let bilingual = args.get(3).is_some_and(|a| a == "bilingual");
    let kind = Kind::from_path(input).expect("a known extension");
    let doc = Doc::read(kind, &std::fs::read(input).unwrap()).unwrap();
    println!("{} units, pages {:?}", doc.units.len(), doc.pages);
    for u in &doc.units {
        println!("  [{}] {}", u.group, u.text);
    }
    let tr: Vec<String> = doc.units.iter().map(|u| format!("[RU] {}", u.text)).collect();
    let opts = WriteOptions { output: Output::Same, keep_formatting: true, bilingual };
    let (bytes, ext) = doc.write(&tr, &opts).unwrap();
    let out = out_dir.join(format!("{}.out.{ext}", input.file_stem().unwrap().to_string_lossy()));
    std::fs::write(&out, bytes).unwrap();
    println!("→ {}", out.display());
}
