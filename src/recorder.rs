use crossbeam_queue::SegQueue;
use enum_iterator::Sequence;
use midi_fundsp::io::SynthMsg;
use midi_note_recorder::Recording;
use std::{ops::Index, sync::Arc, time::Instant};

#[derive(Sequence, Copy, Clone, PartialEq, Eq, Debug)]
pub enum RecordingMode {
    Playthrough,
    Record,
    SoloOver,
}

impl RecordingMode {
    pub fn text(&self) -> &'static str {
        match self {
            Self::Playthrough => "Play Freely",
            Self::Record => "Record Accompaniment",
            Self::SoloOver => "Solo Over Recording",
        }
    }
}

pub struct Recorder {
    pub timeout: f64,
    pub mode: RecordingMode,
    accompaniments: Vec<Recording>,
    solos: Vec<Recording>,
    solo_duration: Option<f64>,
    incoming: Arc<SegQueue<SynthMsg>>,
    outgoing: Arc<SegQueue<SynthMsg>>,
    last_msg: Instant,
    current_start: Instant,
    input_port_name: String,
}

impl Recorder {
    pub fn new(timeout: f64, incoming: Arc<SegQueue<SynthMsg>>, outgoing: Arc<SegQueue<SynthMsg>>, input_port_name: String) -> Self {
        Self {
            timeout,
            accompaniments: vec![],
            solos: vec![],
            solo_duration: None,
            incoming,
            outgoing,
            last_msg: Instant::now(),
            current_start: Instant::now(),
            input_port_name,
            mode: RecordingMode::Playthrough,
        }
    }

    pub fn len(&self) -> usize {
        self.accompaniments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn input_port_name(&self) -> &str {
        self.input_port_name.as_str()
    }

    pub fn actively_recording(&self) -> bool {
        !self.accompaniments.is_empty()
            && Instant::now().duration_since(self.last_msg).as_secs_f64() < self.timeout
    }

    pub fn actively_soloing(&self) -> bool {
        self.solo_duration.is_some()
    }

    pub fn receive(&mut self, msg: SynthMsg) {
        match self.mode {
            RecordingMode::Playthrough => {}
            RecordingMode::Record => {
                let now = Instant::now();
                if !self.actively_recording() {
                    self.accompaniments.push(Recording::default());
                    self.current_start = now;
                }
                self.accompaniments.last_mut().unwrap().add_message(
                    now.duration_since(self.current_start).as_secs_f64(),
                    &msg.msg,
                );
                self.last_msg = now;
            }
            RecordingMode::SoloOver => {
                if let Some(duration) = self.solo_duration {
                    let now = Instant::now();
                    let so_far = now.duration_since(self.current_start).as_secs_f64();
                    if so_far > duration {
                        self.solo_duration = None;
                    } else {
                        self.solos.last_mut().unwrap().add_message(so_far, &msg.msg);
                    }
                    self.last_msg = now;
                }
            }
        }
    }

    pub fn start_solo_thread(&mut self, selected: usize) {
        assert_eq!(self.mode, RecordingMode::SoloOver);
        let backing = self.accompaniments[selected].clone();
        self.solo_duration = Some(backing.duration());
        self.solos.push(Recording::default());
        self.current_start = Instant::now();
        let incoming = self.incoming.clone();
        let outgoing = self.outgoing.clone();
        std::thread::spawn(move || {
            backing.playback_loop(None, outgoing, |msg| SynthMsg {
                msg: msg,
                speaker: midi_fundsp::io::Speaker::Both,
            });
            incoming.push(SynthMsg::all_notes_off(midi_fundsp::io::Speaker::Both));
        });
    }
}

impl Index<usize> for Recorder {
    type Output = Recording;

    fn index(&self, index: usize) -> &Self::Output {
        &self.accompaniments[index]
    }
}
