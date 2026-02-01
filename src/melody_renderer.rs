use std::{
    cmp::{max, min},
    collections::HashSet,
    ops::RangeInclusive,
};

use bare_metal_modulo::{MNum, OffsetNumC};
use eframe::{
    egui::{Painter, Sense, Ui},
    emath::Align2,
    epaint::{Color32, FontFamily, FontId, Pos2, Stroke, Vec2},
};
use music_analyzer_generator::{
    analyzer::{Melody, MelodyDirection},
    notes::{Accidental, Note, NoteLetter, NoteName},
    scales::{RootedScale, ScaleMode},
};

const MAIN_MELODY_SCALING: f32 = 0.8;
const MIDDLE_C: u8 = 60;
const STAFF_PITCH_WIDTH: u8 = 19;
const LOWEST_STAFF_PITCH: u8 = MIDDLE_C - STAFF_PITCH_WIDTH;
const HIGHEST_STAFF_PITCH: u8 = MIDDLE_C + STAFF_PITCH_WIDTH;
const BORDER_SIZE: f32 = 8.0;
const Y_OFFSET: f32 = BORDER_SIZE * 2.0;
const X_OFFSET: f32 = BORDER_SIZE * 5.0;
const ACCIDENTAL_SIZE_MULTIPLIER: f32 = 5.0;
const KEY_SIGNATURE_OFFSET: f32 = 28.0;
const NUM_STAFF_LINES: u8 = 5;
const LINE_STROKE: Stroke = Stroke {
    width: 1.0,
    color: Color32::BLACK,
};
const NUM_NOTES_ON_STAFF: usize = 11;
const TREBLE_INITIAL_OFFSET: u8 = 3;
const BASS_TO_TREBLE_OFFSET: u8 = 14;

pub fn font_id(size: f32) -> FontId {
    FontId {
        size,
        family: FontFamily::Proportional,
    }
}

const SHARP_ORDER: [NoteLetter; 7] = [
    NoteLetter::F,
    NoteLetter::C,
    NoteLetter::G,
    NoteLetter::D,
    NoteLetter::A,
    NoteLetter::E,
    NoteLetter::B,
];

fn key_sig_sharps(sharps: &HashSet<NoteLetter>) -> Vec<NoteLetter> {
    SHARP_ORDER
        .iter()
        .filter(|nl| sharps.contains(*nl))
        .copied()
        .collect()
}

fn key_sig_flats(flats: &HashSet<NoteLetter>) -> Vec<NoteLetter> {
    SHARP_ORDER
        .iter()
        .rev()
        .filter(|nl| flats.contains(*nl))
        .copied()
        .collect()
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct KeySignature {
    notes: Vec<NoteLetter>,
    accidental: Accidental,
}

impl From<&RootedScale> for KeySignature {
    fn from(value: &RootedScale) -> Self {
        let sharps = value.all_sharps().collect::<HashSet<_>>();
        let flats = value.all_flats().collect::<HashSet<_>>();
        if sharps.len() > 0 {
            Self {
                notes: key_sig_sharps(&sharps),
                accidental: Accidental::Sharp,
            }
        } else if flats.len() > 0 {
            Self {
                notes: key_sig_flats(&flats),
                accidental: Accidental::Flat,
            }
        } else {
            Self {
                notes: vec![],
                accidental: Accidental::Natural,
            }
        }
    }
}

impl KeySignature {
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn symbol(&self) -> Accidental {
        self.accidental
    }

    fn constrain_up(staff_position: u8) -> u8 {
        OffsetNumC::<u8, 7, 5>::new(staff_position).m()
    }

    fn constrain_staff(staff_position: u8) -> u8 {
        OffsetNumC::<u8, NUM_NOTES_ON_STAFF, 1>::new(staff_position).a()
    }

    fn constrain(staff_position: u8, direction: i16) -> u8 {
        if direction > 0 {
            Self::constrain_up(staff_position)
        } else {
            Self::constrain_staff(staff_position)
        }
    }

    pub fn treble_clef(&self) -> Vec<u8> {
        let (offset, direction) = match self.accidental {
            Accidental::Sharp => (-(TREBLE_INITIAL_OFFSET as i16), 1_i16),
            Accidental::Flat => (TREBLE_INITIAL_OFFSET as i16, -1),
            Accidental::Natural => return vec![],
            _ => panic!("These should not appear in a clef"),
        };
        let c_major = ScaleMode::Major.rooted(NoteName::name_of(60));
        let middle_c = c_major.middle_c();
        let start1 = Self::constrain_up(
            c_major
                .diatonic_steps_between(middle_c, middle_c + self.notes[0].natural_pitch())
                .unwrap(),
        );
        let mut frontier = [start1, Self::constrain_up((start1 as i16 + offset) as u8)];
        let mut result = vec![];
        for (i, _) in self.notes.iter().enumerate() {
            result.push(frontier[i % 2]);
            frontier[i % 2] =
                Self::constrain((frontier[i % 2] as i16 + direction) as u8, direction);
        }
        result
    }

    pub fn bass_clef(&self) -> Vec<u8> {
        self.treble_clef()
            .drain(..)
            .map(|p| p - BASS_TO_TREBLE_OFFSET)
            .collect()
    }
}

/// Musical symbols are a very tricky issue. Here are resources I've used:
/// * Font: [Bravura](https://github.com/steinbergmedia/bravura)
/// * [Unicode for a few symbols](https://www.compart.com/en/unicode/block/U+2600)
/// * [Unicode for the remaining symbols](https://unicode.org/charts/PDF/U1D100.pdf)
pub struct MelodyRenderer {
    scale: RootedScale,
    sig: KeySignature,
    x_range: RangeInclusive<f32>,
    //y_range: RangeInclusive<f32>,
    y_per_pitch: f32,
    y_middle_c: f32,
    hi: u8,
}

impl MelodyRenderer {
    pub fn render(ui: &mut Ui, melodies: &Vec<(Melody, Color32)>) {
        let size = Vec2::new(
            ui.available_width(),
            ui.available_height() * MAIN_MELODY_SCALING,
        );
        if melodies.len() > 0 && melodies.iter().any(|(m, _)| m.len() > 0) {
            let (response, painter) = ui.allocate_painter(size, Sense::hover());
            let scale = melodies[0].0.highest_weight_scale();
            let (lo, hi) = Self::min_max_staff(&scale, melodies);
            let num_diatonic_pitches = 1 + scale.diatonic_steps_between(lo, hi).unwrap();
            let y_per_pitch = ((response.rect.max.y - response.rect.min.y) - BORDER_SIZE * 2.0)
                / num_diatonic_pitches as f32;
            let y_border = Y_OFFSET + response.rect.min.y;
            let sig = KeySignature::from(&scale);
            let scale_hi = scale.round_up(hi);
            if let Some(steps) = scale.diatonic_steps_to_middle_c(scale_hi) {
                let y_middle_c = y_border + y_per_pitch * steps as f32;
                let renderer = MelodyRenderer {
                    hi,
                    scale,
                    y_per_pitch,
                    x_range: response.rect.min.x + BORDER_SIZE..=response.rect.max.x - BORDER_SIZE,
                    //y_range: response.rect.min.y + BORDER_SIZE..=response.rect.max.y - BORDER_SIZE,
                    sig,
                    y_middle_c,
                };
                let y_treble = y_border + y_per_pitch * renderer.space_above_staff();
                renderer.draw_staff(&painter, Clef::Treble, y_treble);
                let y_bass = renderer.y_middle_c + renderer.staff_line_space();
                renderer.draw_staff(&painter, Clef::Bass, y_bass);
                for (melody, color) in melodies.iter().rev() {
                    renderer.draw_melody(&painter, melody, *color);
                }
            } else {
                println!(
                    "middle-c failed: scale: {} {:?} (hi: {scale_hi})",
                    scale.root_name(),
                    scale.mode()
                );
                println!("{:?}", melodies[0].0);
            }
        }
    }

    fn staff_line_space(&self) -> f32 {
        self.y_per_pitch * 2.0
    }

    fn space_above_staff(&self) -> f32 {
        let highest_staff = self.scale.round_up(HIGHEST_STAFF_PITCH as u8);
        let highest_pitch = self.scale.round_up(self.hi as u8);
        1.0 + self
            .scale
            .diatonic_steps_between(highest_staff, highest_pitch)
            .unwrap() as f32
    }

    fn min_x(&self) -> f32 {
        *self.x_range.start()
    }

    fn total_note_x(&self) -> f32 {
        *self.x_range.end() - self.note_offset_x()
    }

    fn note_offset_x(&self) -> f32 {
        self.min_x() + X_OFFSET + KEY_SIGNATURE_OFFSET + self.y_per_pitch * self.sig.len() as f32
    }

    fn draw_melody(&self, painter: &Painter, melody: &Melody, color: Color32) {
        let mut note_renderer = IncrementalNoteRenderer::new(self, painter, color);
        for (note, direction) in melody.iter_direction() {
            let x = self.note_offset_x()
                + self.total_note_x() * note_renderer.total_duration / melody.duration() as f32;
            note_renderer.note_update(note, direction, &self.scale);
            let y = self.y_middle_c - note_renderer.staff_offset as f32 * self.y_per_pitch;
            if !note.is_rest() {
                note_renderer.show_note(x, y);
            }
        }
    }

    fn draw_staff(&self, painter: &Painter, clef: Clef, start_y: f32) {
        let mut y = start_y;
        clef.render(painter, self.min_x(), y, self.y_per_pitch);
        for _ in 0..NUM_STAFF_LINES {
            painter.hline(self.x_range.clone(), y, LINE_STROKE);
            y += self.staff_line_space();
        }
        for (i, position) in clef.key_signature_positions(&self.sig).iter().enumerate() {
            let x = self.min_x() + KEY_SIGNATURE_OFFSET + self.y_per_pitch * i as f32;
            let y = self.y_middle_c - *position as f32 * self.y_per_pitch;
            self.draw_accidental(painter, self.sig.symbol(), x, y, Color32::BLACK);
        }
    }

    fn draw_accidental(
        &self,
        painter: &Painter,
        text: Accidental,
        x: f32,
        y: f32,
        text_color: Color32,
    ) {
        painter.text(
            Pos2 { x, y },
            Align2::CENTER_CENTER,
            text.symbol(),
            font_id(ACCIDENTAL_SIZE_MULTIPLIER * self.y_per_pitch),
            text_color,
        );
    }

    fn draw_extra_dashes(&self, painter: &Painter, x: f32, staff_offset: i16) {
        let staff_extra_threshold = (NUM_STAFF_LINES as i16 + 1) * 2;
        if staff_offset == 0 {
            self.draw_extra_dash(painter, x, staff_offset);
        } else if staff_offset >= staff_extra_threshold {
            for offset in staff_extra_threshold..=staff_offset {
                self.draw_extra_dash(painter, x, offset);
            }
        } else if staff_offset <= -staff_extra_threshold {
            for offset in staff_offset..=-staff_extra_threshold {
                self.draw_extra_dash(painter, x, offset);
            }
        }
    }

    fn draw_extra_dash(&self, painter: &Painter, x: f32, staff_offset: i16) {
        let x_offset = self.y_per_pitch * 1.5;
        let x1 = x - x_offset;
        let x2 = x + x_offset;
        let y = self.y_middle_c - staff_offset as f32 * self.y_per_pitch;
        painter.line_segment([Pos2 { x: x1, y }, Pos2 { x: x2, y }], LINE_STROKE);
    }

    fn min_max_staff(scale: &RootedScale, melodies: &Vec<(Melody, Color32)>) -> (u8, u8) {
        let mut lo = LOWEST_STAFF_PITCH;
        let mut hi = HIGHEST_STAFF_PITCH;
        for (melody, _) in melodies.iter() {
            if let Some((mlo, mhi)) = melody.min_max_pitches() {
                lo = min(lo, mlo);
                hi = max(hi, mhi);
            }
        }
        (scale.round_down(lo), scale.round_up(hi))
    }
}

struct IncrementalNoteRenderer<'a> {
    renderer: &'a MelodyRenderer,
    painter: &'a Painter,
    total_duration: f32,
    staff_offset: i16,
    note_color: Color32,
    auxiliary_symbol: Option<Accidental>,
}

impl<'a> IncrementalNoteRenderer<'a> {
    fn new(renderer: &'a MelodyRenderer, painter: &'a Painter, note_color: Color32) -> Self {
        Self {
            renderer,
            total_duration: 0.0,
            painter,
            auxiliary_symbol: None,
            staff_offset: 0,
            note_color,
        }
    }

    fn note_update(&mut self, note: &Note, direction: MelodyDirection, scale: &RootedScale) {
        self.total_duration += note.duration() as f32;
        let (staff_offset, auxiliary_symbol) = staff_position(&scale, note.pitch(), direction);
        self.staff_offset = staff_offset;
        self.auxiliary_symbol = auxiliary_symbol;
    }

    fn show_note(&self, x: f32, y: f32) {
        self.painter.circle_filled(
            Pos2 { x, y },
            self.renderer.y_per_pitch / 2.0,
            self.note_color,
        );
        if let Some(auxiliary_symbol) = self.auxiliary_symbol {
            let x = x + self.renderer.staff_line_space();
            self.renderer
                .draw_accidental(self.painter, auxiliary_symbol, x, y, self.note_color);
        }
        self.renderer
            .draw_extra_dashes(self.painter, x, self.staff_offset);
    }
}

fn staff_position(
    scale: &RootedScale,
    pitch: u8,
    direction: MelodyDirection,
) -> (i16, Option<Accidental>) {
    let (pitch, acc) = if scale.contains(pitch) {
        (pitch, None)
    } else {
        let (_, pitch, acc) = match direction {
            MelodyDirection::Ascending => scale.ascending_match(pitch),
            MelodyDirection::Descending => scale.descending_match(pitch),
        };
        (pitch, acc)
    };
    let mut steps = scale
        .diatonic_steps_between(scale.middle_c(), pitch)
        .unwrap() as i16;
    if pitch < scale.middle_c() {
        steps = -steps;
    }
    (steps, acc)
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Clef {
    Treble,
    Bass,
}

impl Clef {
    pub fn symbol(&self) -> char {
        match self {
            Self::Treble => '\u{1d11e}',
            Self::Bass => '\u{1d122}',
        }
    }

    pub fn key_signature_positions(&self, sig: &KeySignature) -> Vec<u8> {
        match self {
            Self::Treble => sig.treble_clef(),
            Self::Bass => sig.bass_clef(),
        }
    }

    fn size(&self) -> f32 {
        match self {
            Self::Treble => 13.5,
            Self::Bass => 8.0,
        }
    }

    fn x_offset(&self) -> f32 {
        10.0
    }

    fn y_offset(&self) -> f32 {
        match self {
            Self::Treble => 5.0,
            Self::Bass => -0.45,
        }
    }

    fn render(&self, painter: &Painter, x: f32, y: f32, y_per_pitch: f32) {
        painter.text(
            Pos2 {
                x: x + self.x_offset(),
                y: y + self.y_offset() * y_per_pitch,
            },
            Align2::CENTER_CENTER,
            self.symbol(),
            font_id(self.size() * y_per_pitch),
            Color32::BLACK,
        );
    }
}
