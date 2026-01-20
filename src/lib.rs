pub mod recorder;

use std::path::PathBuf;

use eframe::egui::FontDefinitions;
use midi_note_recorder::Recording;
use music_analyzer_generator::{ChordName, PitchSequence};

pub fn chords_starts(recording: &Recording) -> Vec<(ChordName, f64)> {
    let mut result = vec![];
    for (chord, start, _) in PitchSequence::new(recording).chords_starts_durations() {
        let push = result
            .last()
            .map_or(true, |(last_name, _)| *last_name != chord.name());
        if push {
            result.push((chord.name(), start));
        }
    }
    result
}

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
