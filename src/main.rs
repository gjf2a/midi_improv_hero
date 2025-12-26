use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use eframe::egui::{self, Pos2, Vec2, Visuals};
use midi_fundsp::{
    io::{SynthMsg, get_first_midi_device, start_input_thread, start_output_thread},
    sounds::options,
};
use midi_improv_hero::setup_font;
use midi_note_recorder::Recording;
use midir::MidiInput;

const TIMEOUT: f64 = 3.0;
const NUM_CHANNELS: usize = 10;

// Vision for this program
//
// Version 1:
// * Records a chord progression, displaying notes & chords as it records.
// * Stops after a sufficient pause.
// * Lets you save the chord progression.
// * You can load and play chord progressions.
//
// Version 2:
// * When you play a chord progression, you can play over it.
// * When the chord progression ends, it displays a score for your melody.
// * Score components:
//   * A point for each note that is part of a melodic figure.
//   * A point for each note that is part of a scale for the chord it is over.
//   * Subtract a point for notes that fail the above criteria.
//
// Version 3:
// * Once you have played at least one melody over a chord progression,
//   you can ask it to generate a melody for you to match.
// * Scoring is based on how closely you hit the notes.
// * The note durations will be taken from one of your melodies for that progression.
// * The melody itself will be generated as follows:
//   * Start with the same note as the source melody.
//   * For each succeeding note:
//     * Pick randomly from the following:
//       * Notes that are part of a scale associated with the current chord.
//       * Notes that continue a melodic figure from the preceding notes.
//   * End with the same note as the original melody. Restrict the last few
//     as needed in order to make this work.

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

struct Recorder {
    recordings: Vec<Recording>,
    timeout: f64,
    last_msg: Instant,
    current_start: Instant,
    input_port_name: String,
}

impl Recorder {
    fn new(timeout: f64, input_port_name: String) -> Self {
        Self {
            timeout,
            recordings: vec![],
            last_msg: Instant::now(),
            current_start: Instant::now(),
            input_port_name,
        }
    }

    fn is_playing(&self) -> bool {
        !self.recordings.is_empty()
            && Instant::now().duration_since(self.last_msg).as_secs_f64() < self.timeout
    }

    fn receive(&mut self, msg: SynthMsg) {
        let now = Instant::now();
        if !self.is_playing() {
            self.recordings.push(Recording::default());
            self.current_start = now;
        }
        self.recordings.last_mut().unwrap().add_message(
            now.duration_since(self.current_start).as_secs_f64(),
            &msg.msg,
        );
        self.last_msg = now;
    }
}

struct GameApp {
    recorder: Arc<Mutex<Recorder>>,
}

impl GameApp {
    fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        setup_font("bravura/BravuraText.otf", cc)?;
        Ok(Self {
            recorder: Self::setup_threads()?,
        })
    }

    fn port_name(&self) -> String {
        self.recorder.lock().unwrap().input_port_name.clone()
    }

    fn setup_threads() -> anyhow::Result<Arc<Mutex<Recorder>>> {
        let mut midi_in = MidiInput::new("midir reading input")?;
        let in_port = get_first_midi_device(&mut midi_in)?;
        let input2monitor = Arc::new(SegQueue::new());
        let monitor2output = Arc::new(SegQueue::new());
        let quit = Arc::new(AtomicCell::new(false));
        let recorder = Arc::new(Mutex::new(Recorder::new(
            TIMEOUT,
            midi_in.port_name(&in_port)?,
        )));
        start_input_thread(input2monitor.clone(), midi_in, in_port, quit.clone());
        start_monitor_thread(
            input2monitor,
            monitor2output.clone(),
            quit,
            recorder.clone(),
        );
        start_output_thread::<NUM_CHANNELS>(monitor2output, Arc::new(Mutex::new(options())));
        Ok(recorder)
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
                outgoing.push(msg.clone());
                let mut recorder = recorder.lock().unwrap();
                recorder.receive(msg);
            }
        }
    });
}

impl eframe::App for GameApp {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::light());
        egui::CentralPanel::default().show(ctx, |ui| {
            let heading = format!("MIDI Improv Hero ({})", self.port_name());
            ui.heading(heading);
        });
    }
}
