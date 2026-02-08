use music_analyzer_generator::{analyzer::{ChordProgression, Melody}, figures::FigureMatcher};

pub struct MelodyScore {
    total_pitches: usize,
    total_chords: usize,
    compatible_chord_pitches: usize,
    chords_with_tones: usize,
    out_of_figures: usize,
}

impl MelodyScore {
    pub fn score(melody: &Melody, chords: &ChordProgression) -> Self {
        let mut out_of_figures = 0;
        let mut compatible_chord_pitches = 0;
        let mut timestamp = 0.0;
        let figure_table = FigureMatcher::matching_figures(melody);
        for (i, note) in melody.iter().enumerate() {
            if figure_table[i].len() == 0 {
                out_of_figures += 1;
            }
            let chord = chords.chord_at_time(timestamp).unwrap();
            todo!("Figure out how to determine if note fits this chord. Maybe see if it is a member of one of the scales that fit the chord.");
            timestamp += note.duration();
        }
        todo!("Make sure every chord has a chord tone in the melody in the proper place.")
    }
}