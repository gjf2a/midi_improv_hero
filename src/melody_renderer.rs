use std::{
    cmp::{max, min},
    ops::RangeInclusive,
};

use bare_metal_modulo::{MNum, OffsetNumC};
use eframe::{
    egui::{Painter, Sense, Ui},
    emath::Align2,
    epaint::{Color32, FontFamily, FontId, Pos2, Stroke, Vec2},
};
use midi_msg::{Channel, ChannelVoiceMsg, MidiMsg};
use music_analyzer_generator::{Accidental, NoteLetter, NoteName, ScaleMode, RootedScale};
use ordered_float::OrderedFloat;

type MidiByte = i16;

const MIDDLE_C: MidiByte = 60;
const STAFF_PITCH_WIDTH: MidiByte = 19;
const LOWEST_STAFF_PITCH: MidiByte = MIDDLE_C - STAFF_PITCH_WIDTH;
const HIGHEST_STAFF_PITCH: MidiByte = MIDDLE_C + STAFF_PITCH_WIDTH;
const BORDER_SIZE: f32 = 8.0;
const Y_OFFSET: f32 = BORDER_SIZE * 2.0;
const X_OFFSET: f32 = BORDER_SIZE * 5.0;
const ACCIDENTAL_SIZE_MULTIPLIER: f32 = 5.0;
const KEY_SIGNATURE_OFFSET: f32 = 28.0;
const NUM_STAFF_LINES: MidiByte = 5;
const LINE_STROKE: Stroke = Stroke {
    width: 1.0,
    color: Color32::BLACK,
};

pub fn font_id(size: f32) -> FontId {
    FontId {
        size,
        family: FontFamily::Proportional,
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
struct KeySignature {
    notes: Vec<NoteLetter>,
    accidental: Accidental,
}

const NUM_NOTES_ON_STAFF: usize = 11;
const TREBLE_INITIAL_OFFSET: MidiByte = 3;
const TREBLE_TO_BASS_OFFSET: MidiByte = -14;

impl KeySignature {
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn symbol(&self) -> Accidental {
        self.accidental
    }

    fn constrain_up(staff_position: MidiByte) -> MidiByte {
        OffsetNumC::<MidiByte, 7, 5>::new(staff_position).m()
    }

    fn constrain_staff(staff_position: MidiByte) -> MidiByte {
        OffsetNumC::<MidiByte, NUM_NOTES_ON_STAFF, 1>::new(staff_position).a()
    }

    fn constrain(staff_position: MidiByte, direction: MidiByte) -> MidiByte {
        if direction > 0 {
            Self::constrain_up(staff_position)
        } else {
            Self::constrain_staff(staff_position)
        }
    }

    pub fn treble_clef(&self) -> Vec<MidiByte> {
        let (offset, direction) = match self.accidental {
            Accidental::Sharp => (-TREBLE_INITIAL_OFFSET, 1),
            Accidental::Flat => (TREBLE_INITIAL_OFFSET, -1),
            Accidental::Natural => return vec![],
            _ => panic!("These should not appear in a clef"),
        };
        let c_major = ScaleMode::Major.rooted(NoteName::name_of(60)); 
        let middle_c = c_major.middle_c();
        let start1 = Self::constrain_up(
            c_major
                .diatonic_steps_between(middle_c, middle_c + self.notes[0].natural_pitch())
                .unwrap() as MidiByte,
        );
        let mut frontier = [start1, Self::constrain_up(start1 + offset)];
        let mut result = vec![];
        for (i, _) in self.notes.iter().enumerate() {
            result.push(frontier[i % 2]);
            frontier[i % 2] = Self::constrain(frontier[i % 2] + direction, direction);
        }
        result
    }

    pub fn bass_clef(&self) -> Vec<MidiByte> {
        self.treble_clef()
            .drain(..)
            .map(|p| p + TREBLE_TO_BASS_OFFSET)
            .collect()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Note {
    pitch: MidiByte,
    duration: OrderedFloat<f64>,
    velocity: MidiByte,
}

impl Note {
    pub fn new(pitch: MidiByte, duration: f64, velocity: MidiByte) -> Self {
        Note {
            pitch,
            duration: OrderedFloat(duration),
            velocity,
        }
    }

    pub fn to_midi(&self) -> (MidiMsg, f64) {
        let note = self.pitch as u8;
        let midi = MidiMsg::ChannelVoice {
            channel: Channel::Ch1,
            msg: if self.is_rest() {
                ChannelVoiceMsg::NoteOff { note, velocity: 0 }
            } else {
                ChannelVoiceMsg::NoteOn {
                    note,
                    velocity: self.velocity as u8,
                }
            },
        };
        (midi, self.duration.into_inner())
    }

    pub fn pitch(&self) -> MidiByte {
        self.pitch
    }

    pub fn duration(&self) -> f64 {
        self.duration.into_inner()
    }

    pub fn velocity(&self) -> MidiByte {
        self.velocity
    }

    pub fn is_rest(&self) -> bool {
        self.velocity == 0
    }

    pub fn repitched(&self, new_pitch: MidiByte) -> Note {
        Note {
            pitch: new_pitch,
            duration: self.duration,
            velocity: self.velocity,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Melody {
    notes: Vec<Note>
}

impl Melody {
    pub fn iter(&self) -> impl Iterator<Item=&Note> {
        self.notes.iter()
    }

    pub fn duration(&self) -> f64 {
        self.notes.iter().map(|n| n.duration.into_inner()).sum()
    }

    pub fn min_max_pitches(&self) -> (MidiByte, MidiByte) {
        let mut lo = self.notes[0].pitch;
        let mut hi = lo;
        for note in self.notes.iter().skip(1) {
            lo = min(lo, note.pitch);
            hi = max(hi, note.pitch);
        }
        (lo, hi)
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
    y_range: RangeInclusive<f32>,
    y_per_pitch: f32,
    y_middle_c: f32,
    hi: MidiByte,
}

impl MelodyRenderer {
    fn staff_line_space(&self) -> f32 {
        self.y_per_pitch * 2.0
    }

    fn space_above_staff(&self) -> f32 {
        let highest_staff = self.scale.round_up(HIGHEST_STAFF_PITCH as u8);
        let highest_pitch = self.scale.round_up(self.hi as u8);
        1.0 + self.scale.diatonic_steps_between(highest_staff, highest_pitch).unwrap() as f32
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

    pub fn render(
        ui: &mut Ui,
        size: Vec2,
        melodies: &Vec<(Melody, Color32)>,
        show_sections: bool,
        show_figures: bool,
    ) {
        if melodies.len() > 0 {
            let (response, painter) = ui.allocate_painter(size, Sense::hover());
            let scale = melodies[0].0.best_scale_for();
            let (lo, hi) = Self::min_max_staff(&scale, melodies);
            let num_diatonic_pitches =
                1 + scale.diatonic_steps_between(lo, hi).pure_degree().unwrap();
            let y_per_pitch = ((response.rect.max.y - response.rect.min.y) - BORDER_SIZE * 2.0)
                / num_diatonic_pitches as f32;
            let y_border = Y_OFFSET + response.rect.min.y;
            let renderer = MelodyRenderer {
                hi,
                scale,
                y_per_pitch,
                x_range: response.rect.min.x + BORDER_SIZE..=response.rect.max.x - BORDER_SIZE,
                y_range: response.rect.min.y + BORDER_SIZE..=response.rect.max.y - BORDER_SIZE,
                sig: scale.key_signature(),
                y_middle_c: y_border
                    + y_per_pitch * scale.diatonic_steps_between_round_up(MIDDLE_C, hi) as f32,
            };
            let y_treble = y_border + y_per_pitch * renderer.space_above_staff();
            renderer.draw_staff(&painter, Clef::Treble, y_treble);
            let y_bass = renderer.y_middle_c + renderer.staff_line_space();
            renderer.draw_staff(&painter, Clef::Bass, y_bass);
            for (i, (melody, color)) in melodies.iter().enumerate().rev() {
                renderer.draw_melody(
                    &painter,
                    melody,
                    show_sections,
                    *color,
                );
            }
        }
    }

    fn draw_melody(
        &self,
        painter: &Painter,
        melody: &Melody,
        show_sections: bool,
        color: Color32,
    ) {
        let mut note_renderer =
            IncrementalNoteRenderer::new(self, painter, melody, show_sections, color);
        for (i, note) in melody.iter().enumerate() {
            let x = self.note_offset_x()
                + self.total_note_x() * note_renderer.total_duration / melody.duration() as f32;
            note_renderer.note_update(note, &self.scale);
            let y = self.y_middle_c - note_renderer.staff_offset as f32 * self.y_per_pitch;
            if !note.is_rest() {
                note_renderer.show_note(i, x, y);
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

    fn draw_extra_dashes(&self, painter: &Painter, x: f32, staff_offset: MidiByte) {
        let staff_extra_threshold = (NUM_STAFF_LINES + 1) * 2;
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

    fn draw_extra_dash(&self, painter: &Painter, x: f32, staff_offset: MidiByte) {
        let x_offset = self.y_per_pitch * 1.5;
        let x1 = x - x_offset;
        let x2 = x + x_offset;
        let y = self.y_middle_c - staff_offset as f32 * self.y_per_pitch;
        painter.line_segment([Pos2 { x: x1, y }, Pos2 { x: x2, y }], LINE_STROKE);
    }

    fn min_max_staff(scale: &RootedScale, melodies: &Vec<(Melody, Color32)>) -> (MidiByte, MidiByte) {
        let mut lo = LOWEST_STAFF_PITCH;
        let mut hi = HIGHEST_STAFF_PITCH;
        for (melody, _) in melodies.iter() {
            let (mlo, mhi) = melody.min_max_pitches();
            lo = min(lo, mlo);
            hi = max(hi, mhi);
        }
        (scale.round_down(lo as u8) as MidiByte, scale.round_up(hi as u8) as MidiByte)
    }
}

struct IncrementalNoteRenderer<'a> {
    renderer: &'a MelodyRenderer,
    melody: &'a Melody,
    painter: &'a Painter,
    total_duration: f32,
    show_sections: bool,
    staff_offset: i16,
    note_color: Color32,
    auxiliary_symbol: Option<Accidental>,
}

impl<'a> IncrementalNoteRenderer<'a> {
    fn new(
        renderer: &'a MelodyRenderer,
        painter: &'a Painter,
        melody: &'a Melody,
        show_sections: bool,
        note_color: Color32,
    ) -> Self {
        Self {
            renderer,
            total_duration: 0.0,
            melody,
            painter,
            show_sections,
            auxiliary_symbol: None,
            staff_offset: 0,
            note_color,
        }
    }

    fn note_update(&mut self, note: &Note, scale: &RootedScale) {
        self.total_duration += note.duration() as f32;
        let (staff_offset, auxiliary_symbol) = scale.staff_position(note.pitch());
        self.staff_offset = staff_offset;
        self.auxiliary_symbol = auxiliary_symbol;
    }

    fn show_note(&self, i: usize, x: f32, y: f32) {
        self.painter
            .circle_filled(Pos2 { x, y }, self.renderer.y_per_pitch, self.note_color);
        if let Some(auxiliary_symbol) = self.auxiliary_symbol {
            let x = x + self.renderer.staff_line_space();
            self.renderer
                .draw_accidental(self.painter, auxiliary_symbol, x, y, self.note_color);
        }
        self.renderer
            .draw_extra_dashes(self.painter, x, self.staff_offset);
    }
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

    pub fn key_signature_positions(&self, sig: &KeySignature) -> Vec<MidiByte> {
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
