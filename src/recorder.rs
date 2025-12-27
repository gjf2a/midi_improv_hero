use std::{ops::Index, time::Instant};
use midi_fundsp::io::SynthMsg;
use midi_note_recorder::Recording;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum RecordingMode {
    Playthrough,
    Record,
}

pub struct Recorder {
    recordings: Vec<Recording>,
    pub timeout: f64,
    last_msg: Instant,
    current_start: Instant,
    input_port_name: String,
    pub mode: RecordingMode,
}

impl Recorder {
    pub fn new(timeout: f64, input_port_name: String) -> Self {
        Self {
            timeout,
            recordings: vec![],
            last_msg: Instant::now(),
            current_start: Instant::now(),
            input_port_name,
            mode: RecordingMode::Playthrough,
        }
    }

    pub fn len(&self) -> usize {
        self.recordings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn input_port_name(&self) -> &str {
        self.input_port_name.as_str()
    }

    pub fn in_recording_mode(&self) -> bool {
        self.mode == RecordingMode::Record
    }

    pub fn actively_recording(&self) -> bool {
        self.in_recording_mode()
            && !self.recordings.is_empty()
            && Instant::now().duration_since(self.last_msg).as_secs_f64() < self.timeout
    }

    pub fn receive(&mut self, msg: SynthMsg) {
        if self.in_recording_mode() {
            let now = Instant::now();
            if !self.actively_recording() {
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
}

impl Index<usize> for Recorder {
    type Output = Recording;

    fn index(&self, index: usize) -> &Self::Output {
        &self.recordings[index]
    }
}