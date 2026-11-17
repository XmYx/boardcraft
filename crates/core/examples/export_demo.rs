//! Write the demo board as `.boardcraft` and as an OTIO animatic:
//! `cargo run -p boardcraft-core --example export_demo -- <out dir>`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "demo-out".into()));
    std::fs::create_dir_all(&out)?;
    let project = boardcraft_core::demo::demo_project();
    boardcraft_core::format::save(&project, &out.join("paper-boats.boardcraft"))?;
    let stills = boardcraft_core::timeline::export(&project, &out.join("paper-boats.otio"))?;
    println!("wrote {} and stills in {}", out.join("paper-boats.otio").display(), stills.display());
    Ok(())
}
