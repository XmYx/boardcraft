//! Write the demo board as `.boardcraft` and as an OTIO animatic:
//! `cargo run -p boardcraft-core --example export_demo -- <out dir> [<font.ttf> for a board sheet]`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "demo-out".into()));
    std::fs::create_dir_all(&out)?;
    let project = boardcraft_core::demo::demo_project();
    boardcraft_core::format::save(&project, &out.join("paper-boats.boardcraft"))?;
    let stills = boardcraft_core::timeline::export(&project, &out.join("paper-boats.otio"))?;
    println!("wrote {} and stills in {}", out.join("paper-boats.otio").display(), stills.display());
    if let Some(font) = std::env::args().nth(2) {
        let font = std::fs::read(font)?;
        let pdf = boardcraft_core::sheet::board_sheet_pdf(&project, &Default::default(), &font, 0)?;
        std::fs::write(out.join("paper-boats-sheet.pdf"), pdf)?;
        println!("wrote {}", out.join("paper-boats-sheet.pdf").display());
    }
    Ok(())
}
