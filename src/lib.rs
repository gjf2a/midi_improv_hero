pub mod recorder;

use std::path::PathBuf;

use eframe::egui::FontDefinitions;

pub fn filename_sans_suffix(path: &PathBuf) -> String {
    path.file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .split(".")
        .next()
        .unwrap()
        .to_owned()
}

pub fn setup_font(filename: &str, cc: &eframe::CreationContext<'_>) -> anyhow::Result<()> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let file_path = PathBuf::from(manifest_dir).join(filename);
    let bytes = std::fs::read(&file_path)?;
    let name = filename_sans_suffix(&file_path);
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        name.clone(),
        eframe::egui::FontData::from_owned(bytes).into(),
    );
    cc.egui_ctx.set_fonts(fonts);
    Ok(())
}
