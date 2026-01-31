use std::sync::{Arc, Mutex};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Stroke, Vec2, Visuals};
use enum_iterator::all;
use midi_fundsp::{
    io::{Speaker, SynthMsg, get_first_midi_device, start_input_thread, start_output_thread},
    sound_builders::ProgramTable,
    sounds::favorites,
};
use midi_improv_hero::{
    melody_renderer::MelodyRenderer,
    recorder::{Recorder, RecordingMode},
    setup_font,
};
use midi_note_recorder::Recording;
use midir::MidiInput;
use music_analyzer_generator::analyzer::{ChordProgression, Melody};

const MIN_TIMEOUT: f64 = 0.25;
const MAX_TIMEOUT: f64 = 3.0;
const DEFAULT_TIMEOUT: f64 = 1.0;
const NUM_CHANNELS: usize = 10;
const FPS: f32 = 20.0;
const FRAME_INTERVAL: f32 = 1.0 / FPS;

fn main() {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2 { x: 800.0, y: 600.0 })
            .with_position(Pos2 { x: 50.0, y: 25.0 })
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "MIDI Improv Hero",
        native_options,
        Box::new(|cc| Ok(Box::new(GameApp::new(cc).unwrap()))),
    )
    .unwrap();
}

struct GameApp {
    recorder: Arc<Mutex<Recorder>>,
    selected_recording: usize,
    synth_sounds: ProgramTable,
    accompaniment_sound: usize,
    solo_sound: usize,
    playback_progress: Arc<AtomicCell<Option<f64>>>,
}

impl eframe::App for GameApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::light());
        egui::CentralPanel::default().show(ctx, |ui| {
            let heading = format!("MIDI Improv Hero ({})", self.port_name());
            ui.heading(heading);
            self.mode_buttons(ui);
            match self.recording_mode() {
                RecordingMode::Record => {
                    self.render_recorder(ui, false);
                    ctx.request_repaint_after_secs(FRAME_INTERVAL);
                }
                RecordingMode::PlaybackAccompaniments => {
                    self.render_recorder(ui, true);
                    ctx.request_repaint_after_secs(FRAME_INTERVAL);
                }
                RecordingMode::SoloOver => {
                    self.render_solo(ui);
                    ctx.request_repaint_after_secs(FRAME_INTERVAL);
                }
                RecordingMode::Playthrough => {
                    self.render_settings(ui);
                }
            }
        });
    }
}

impl GameApp {
    fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        setup_font("bravura/BravuraText.otf", cc)?;
        let synth_sounds = favorites();
        Ok(Self {
            recorder: Self::setup_threads(synth_sounds.clone())?,
            selected_recording: 0,
            synth_sounds,
            accompaniment_sound: 0,
            solo_sound: 0,
            playback_progress: Arc::new(AtomicCell::new(None)),
        })
    }

    fn port_name(&self) -> String {
        self.recorder.lock().unwrap().input_port_name().to_string()
    }

    fn recording_mode(&self) -> RecordingMode {
        let recorder = self.recorder.lock().unwrap();
        recorder.mode
    }

    fn setup_threads(synth_sounds: ProgramTable) -> anyhow::Result<Arc<Mutex<Recorder>>> {
        let mut midi_in = MidiInput::new("midir reading input")?;
        let in_port = get_first_midi_device(&mut midi_in)?;
        let input2monitor = Arc::new(SegQueue::new());
        let monitor2output = Arc::new(SegQueue::new());
        let quit = Arc::new(AtomicCell::new(false));
        let recorder = Arc::new(Mutex::new(Recorder::new(
            DEFAULT_TIMEOUT,
            input2monitor.clone(),
            monitor2output.clone(),
            midi_in.port_name(&in_port)?,
        )));
        start_input_thread(input2monitor.clone(), midi_in, in_port, quit.clone());
        start_monitor_thread(
            input2monitor,
            monitor2output.clone(),
            quit,
            recorder.clone(),
        );
        start_output_thread::<NUM_CHANNELS>(monitor2output, Arc::new(Mutex::new(synth_sounds)));
        Ok(recorder)
    }

    fn mode_buttons(&mut self, ui: &mut egui::Ui) {
        let mut recorder = self.recorder.lock().unwrap();
        ui.horizontal(|ui| {
            for option in all::<RecordingMode>() {
                if option != RecordingMode::SoloOver || !recorder.is_empty() {
                    ui.radio_value(&mut recorder.mode, option, option.text());
                }
            }
        });
    }

    fn render_recorder(&mut self, ui: &mut egui::Ui, playback_button: bool) {
        let mut recorder = self.recorder.lock().unwrap();
        if !recorder.actively_recording() && recorder.last_accompaniment_spurious() {
            recorder.delete_last_accompaniment();
        }

        let timeout = recorder.timeout;
        let suffix = if timeout == 1.0 { "second" } else { "seconds" };
        ui.add(
            egui::Slider::new(&mut recorder.timeout, MIN_TIMEOUT..=MAX_TIMEOUT)
                .text(format!("Recording stops after {timeout} {suffix}"))
                .show_value(false),
        );
        if recorder.actively_recording() && !recorder.actively_soloing() {
            ui.label("recording in progress");
        } else if recorder.is_empty() {
            ui.label("No recordings");
        } else {
            let current =
                Self::render_recording_header(ui, &mut self.selected_recording, &recorder);
            Self::show_chords(ui, current, self.playback_progress.clone());
            if playback_button {
                if ui.button("Play chords").clicked() {
                    recorder.start_accompaniment_playback_thread(
                        self.selected_recording,
                        false,
                        self.playback_progress.clone(),
                    );
                }
            }
        }
    }

    fn render_solo(&mut self, ui: &mut egui::Ui) {
        self.render_recorder(ui, false);
        let mut recorder = self.recorder.lock().unwrap();
        if recorder.actively_soloing() {
            ui.label("Soloing...");
        } else {
            if ui.button("Start accompaniment").clicked() {
                recorder.start_accompaniment_playback_thread(
                    self.selected_recording,
                    true,
                    self.playback_progress.clone(),
                );
            }
        }
        if let Some(solo) = recorder.current_solo() {
            let melody: Melody = Melody::from(solo);
            MelodyRenderer::render(ui, &vec![(melody, Color32::BLACK)]);
        }
    }

    fn show_chords(
        ui: &mut egui::Ui,
        current: &Recording,
        playback_progress: Arc<AtomicCell<Option<f64>>>,
    ) {
        let painter = ui.painter();
        let progression = ChordProgression::from(current);
        Self::paint_spaced_chords(painter, &progression, current.duration());
        Self::paint_progress_bar(painter, current.duration(), playback_progress.clone());
    }

    fn paint_spaced_chords(painter: &Painter, progression: &ChordProgression, duration: f64) {
        let painter_box = painter.clip_rect();
        for (chord, start) in progression.chord_start_iter() {
            painter.text(
                Pos2 {
                    x: (start / duration) as f32 * painter_box.width(),
                    y: painter_box.height() * 0.85,
                },
                Align2::LEFT_TOP,
                chord.compact_name(),
                FontId::default(),
                Color32::BLUE,
            );
        }
    }

    fn paint_progress_bar(
        painter: &Painter,
        duration: f64,
        playback_progress: Arc<AtomicCell<Option<f64>>>,
    ) {
        if let Some(progress) = playback_progress.load() {
            let painter_box = painter.clip_rect();
            let x = (progress / duration) as f32 * painter_box.width();
            painter.line_segment(
                [
                    Pos2 { x, y: 0.0 },
                    Pos2 {
                        x,
                        y: painter_box.height(),
                    },
                ],
                Stroke {
                    width: 5.0,
                    color: Color32::GREEN,
                },
            );
        }
    }

    fn render_recording_header<'a>(
        ui: &mut egui::Ui,
        selected_recording: &mut usize,
        recorder: &'a Recorder,
    ) -> &'a Recording {
        if recorder.num_accompaniments() == 1 {
            ui.label("One recording");
            &recorder.accompaniment(0)
        } else {
            let recs = format!("{} recordings", recorder.num_accompaniments());
            ui.label(recs.as_str());
            ui.heading("Select a Recording");
            ui.add(
                egui::Slider::new(selected_recording, 0..=recorder.num_accompaniments() - 1)
                    .integer(),
            );
            &recorder.accompaniment(*selected_recording)
        }
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        let sounds = self
            .synth_sounds
            .iter()
            .map(|(n, _)| n.clone())
            .collect::<Vec<_>>();
        ui.horizontal(|ui| {
            let mut recorder = self.recorder.lock().unwrap();
            ui.radio_value(&mut recorder.free_speaker, Speaker::Left, "Left Speaker");
            ui.radio_value(&mut recorder.free_speaker, Speaker::Right, "Right Speaker");
        });

        ui.horizontal(|ui| {
            if let Some(changed) = Self::render_synth_sounds(
                "Accompaniment",
                &mut self.accompaniment_sound,
                &sounds,
                ui,
            ) {
                self.recorder
                    .lock()
                    .unwrap()
                    .program_change(changed as u8, Speaker::Left);
            }
            if let Some(changed) =
                Self::render_synth_sounds("Solo", &mut self.solo_sound, &sounds, ui)
            {
                self.recorder
                    .lock()
                    .unwrap()
                    .program_change(changed as u8, Speaker::Right);
            }
        });
    }

    fn render_synth_sounds(
        label: &str,
        target: &mut usize,
        sounds: &Vec<String>,
        ui: &mut egui::Ui,
    ) -> Option<usize> {
        let start = *target;
        ui.vertical(|ui| {
            ui.label(label);
            for (i, name) in sounds.iter().enumerate() {
                ui.radio_value(target, i, name);
            }
        });
        if start != *target {
            Some(*target)
        } else {
            None
        }
    }
}

fn start_monitor_thread(
    incoming: Arc<SegQueue<SynthMsg>>,
    outgoing: Arc<SegQueue<SynthMsg>>,
    quit: Arc<AtomicCell<bool>>,
    recorder: Arc<Mutex<Recorder>>,
) {
    std::thread::spawn(move || {
        while !quit.load() {
            if let Some(msg) = incoming.pop() {
                let mut recorder = recorder.lock().unwrap();
                let mut outgoing_msg = msg.clone();
                outgoing_msg.speaker = recorder.live_speaker();
                outgoing.push(outgoing_msg);
                recorder.receive(msg);
            }
        }
    });
}
