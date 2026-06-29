use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const BASE_MIDI: i32 = 57; // A3 — the octave the `root` (0..12) sits in

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    #[default]
    MajorPentatonic,
    MinorPentatonic,
    Dorian,
    Mixolydian,
    Lydian,
    NaturalMinor,
}

impl Scale {
    pub fn intervals(&self) -> &'static [u8] {
        match self {
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rhythm {
    #[default]
    Even,
    Tresillo,
    Gallop,
    Dotted,
    LongShort,
    Syncopated,
    Cascade,
}

impl Rhythm {
    pub fn pattern(&self) -> &'static [u8] {
        match self {
            Rhythm::Even => &[2, 2, 2, 2],
            Rhythm::Tresillo => &[3, 3, 2],
            Rhythm::Gallop => &[1, 1, 2, 2, 2],
            Rhythm::Dotted => &[3, 1, 3, 1],
            Rhythm::LongShort => &[4, 2, 2],
            Rhythm::Syncopated => &[2, 3, 3],
            Rhythm::Cascade => &[1, 1, 1, 1, 4],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Instrument {
    #[default]
    Synth,
    Pluck,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SonicSignature {
    pub root: u8,
    pub scale: Scale,
    pub progression: Vec<i8>,
    pub rhythm: Rhythm,
    pub instrument: Instrument,
}

const SCALES: [Scale; 6] = [
    Scale::MajorPentatonic,
    Scale::MinorPentatonic,
    Scale::Dorian,
    Scale::Mixolydian,
    Scale::Lydian,
    Scale::NaturalMinor,
];
const RHYTHMS: [Rhythm; 7] = [
    Rhythm::Even,
    Rhythm::Tresillo,
    Rhythm::Gallop,
    Rhythm::Dotted,
    Rhythm::LongShort,
    Rhythm::Syncopated,
    Rhythm::Cascade,
];
const PROGS: [&[i8]; 7] = [
    &[0, 4],
    &[0, 5],
    &[5, 0],
    &[0, 3],
    &[3, 4],
    &[0, 4, 5],
    &[0, 5, 3],
];

impl SonicSignature {
    pub fn from_user_id(s: &str) -> Self {
        let hash = Sha256::digest(s.as_bytes());

        Self {
            scale: SCALES[hash[0] as usize % SCALES.len()],
            root: hash[1] % 12,
            progression: PROGS[hash[2] as usize % PROGS.len()].to_vec(),
            rhythm: RHYTHMS[hash[6] as usize % RHYTHMS.len()],
            instrument: if hash[7] & 1 == 0 {
                Instrument::Synth
            } else {
                Instrument::Pluck
            },
        }
    }
}

/// What the signature is being played *for*. Identity comes from the signature;
/// expression (speed, bass, direction, length) comes from the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureEvent {
    Joined,
    Left,
    Muted,
    Unmuted,
    Deafened,
    Undeafened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BassMode {
    Full, // root + a walking fifth
    Root, // sustained root only
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Performance {
    pub tempo: f32,
    pub bass: BassMode,
    pub ascending: bool, // melodic direction; false also reverses the progression
    pub octave_shift: i32,
    pub arpeggio: bool,
    pub gain: f32,
    pub max_notes: Option<usize>, // abbreviate (mute/unmute) to a short cue
}

impl SignatureEvent {
    pub fn performance(self) -> Performance {
        match self {
            SignatureEvent::Joined => Performance {
                tempo: 140.0,
                bass: BassMode::Full,
                ascending: true,
                octave_shift: 0,
                arpeggio: true,
                gain: 1.0,
                max_notes: None,
            },
            SignatureEvent::Left => Performance {
                tempo: 112.0,
                bass: BassMode::Root,
                ascending: false,
                octave_shift: -12,
                arpeggio: false,
                gain: 0.9,
                max_notes: None,
            },
            SignatureEvent::Muted => Performance {
                tempo: 168.0,
                bass: BassMode::None,
                ascending: false,
                octave_shift: 0,
                arpeggio: false,
                gain: 0.55,
                max_notes: Some(2),
            },
            SignatureEvent::Unmuted => Performance {
                tempo: 168.0,
                bass: BassMode::None,
                ascending: true,
                octave_shift: 0,
                arpeggio: false,
                gain: 0.6,
                max_notes: Some(2),
            },
            SignatureEvent::Deafened => Performance {
                tempo: 140.0,
                bass: BassMode::None,
                ascending: false,
                octave_shift: -12,
                arpeggio: false,
                gain: 0.5,
                max_notes: Some(2),
            },
            SignatureEvent::Undeafened => Performance {
                tempo: 140.0,
                bass: BassMode::None,
                ascending: true,
                octave_shift: 0,
                arpeggio: false,
                gain: 0.5,
                max_notes: Some(2),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Timbre {
    Sine,     // bass / body
    Triangle, // synth lead / arp
    Pluck,    // Karplus–Strong
}

/// A single scheduled note — everything the player needs, no theory required.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NoteEvent {
    pub freq: f32,  // Hz
    pub start: f32, // seconds from playback start
    pub dur: f32,
    pub gain: f32, // 0..1
    pub pan: f32,  // -1..1
    pub attack: f32,
    pub release: f32,
    pub timbre: Timbre,
}

fn midi_to_freq(m: i32) -> f32 {
    440.0 * 2f32.powf((m as f32 - 69.0) / 12.0)
}

/// Resolve a scale degree (which may span octaves) to a MIDI note.
fn scale_note(intervals: &[u8], base: i32, deg: i32) -> i32 {
    let n = intervals.len() as i32;
    let oct = deg.div_euclid(n);
    let idx = deg.rem_euclid(n) as usize;
    base + 12 * oct + intervals[idx] as i32
}

#[allow(clippy::too_many_arguments)]
fn note(
    freq: f32,
    start: f32,
    dur: f32,
    gain: f32,
    pan: f32,
    attack: f32,
    release: f32,
    timbre: Timbre,
) -> NoteEvent {
    NoteEvent {
        freq,
        start,
        dur,
        gain,
        pan,
        attack,
        release,
        timbre,
    }
}

impl SonicSignature {
    pub fn schedule(&self, event: SignatureEvent) -> Vec<NoteEvent> {
        let perf = event.performance();
        let intervals = self.scale.intervals();
        let base = BASE_MIDI + self.root as i32 + perf.octave_shift;
        let beat = 60.0 / perf.tempo;
        let chord_dur = beat * 1.6;
        let six = chord_dur / 8.0; // one sixteenth of the chord window
        let pat = self.rhythm.pattern();
        let lead_timbre = if self.instrument == Instrument::Pluck {
            Timbre::Pluck
        } else {
            Timbre::Triangle
        };

        let mut prog: Vec<i8> = self.progression.clone();
        if prog.is_empty() {
            prog.push(0); // safety: never silent
        }
        if !perf.ascending {
            prog.reverse();
        }

        let mut out: Vec<NoteEvent> = Vec::new();
        let mut produced = 0usize;

        for (ci, &cd) in prog.iter().enumerate() {
            let cd = cd as i32;
            let start = ci as f32 * chord_dur;
            // triad by stacking scale-thirds
            let tones = [
                scale_note(intervals, base, cd),
                scale_note(intervals, base, cd + 2),
                scale_note(intervals, base, cd + 4),
            ];

            // --- BASS ---
            match perf.bass {
                BassMode::None => {}
                BassMode::Root => out.push(note(
                    midi_to_freq(tones[0] - 24),
                    start,
                    chord_dur * 0.98,
                    0.34 * perf.gain,
                    0.0,
                    0.02,
                    0.4,
                    Timbre::Sine,
                )),
                BassMode::Full => {
                    out.push(note(
                        midi_to_freq(tones[0] - 24),
                        start,
                        chord_dur * 0.98,
                        0.34 * perf.gain,
                        0.0,
                        0.02,
                        0.4,
                        Timbre::Sine,
                    ));
                    out.push(note(
                        midi_to_freq(tones[2] - 24),
                        start + chord_dur * 0.55,
                        chord_dur * 0.45,
                        0.26 * perf.gain,
                        0.0,
                        0.02,
                        0.3,
                        Timbre::Sine,
                    ));
                }
            }

            // --- ARPEGGIO ---
            if perf.arpeggio {
                let steps = 6usize;
                let sd = chord_dur / steps as f32;
                for s in 0..steps {
                    let m = tones[s % 3] + 12;
                    let pan = if s % 2 == 0 { -0.35 } else { 0.35 };
                    out.push(note(
                        midi_to_freq(m),
                        start + s as f32 * sd,
                        sd * 0.9,
                        0.085 * perf.gain,
                        pan,
                        0.004,
                        0.06,
                        lead_timbre,
                    ));
                }
            }

            // --- LEAD ---
            // melody = chord tones cycled (root/3rd/5th), placed on the rhythm,
            // contour set by the event. Deterministic from the signature alone.
            let mut degs: Vec<i32> = (0..pat.len()).map(|k| cd + 2 * (k as i32 % 3)).collect();
            degs.sort_by_key(|&d| scale_note(intervals, base, d));
            if !perf.ascending {
                degs.reverse();
            }

            let mut acc = 0.0f32;
            for (k, &deg) in degs.iter().enumerate() {
                if perf.max_notes.is_some_and(|max| produced >= max) {
                    break;
                }
                let len = pat[k] as f32 * six;
                let ns = start + acc;
                acc += len;
                let m = scale_note(intervals, base, deg);
                let pan = if k % 2 == 0 { -0.12 } else { 0.12 };
                out.push(note(
                    midi_to_freq(m),
                    ns,
                    len * 0.94,
                    0.2 * perf.gain,
                    pan,
                    0.006,
                    len * 0.5,
                    lead_timbre,
                ));
                if self.instrument == Instrument::Synth {
                    // soft sine body under the triangle lead
                    out.push(note(
                        midi_to_freq(m),
                        ns,
                        len * 0.94,
                        0.06 * perf.gain,
                        0.0,
                        0.006,
                        len * 0.5,
                        Timbre::Sine,
                    ));
                }
                produced += 1;
            }

            if perf.max_notes.is_some_and(|max| produced >= max) {
                break;
            }
        }

        out
    }
}

const SAMPLE_RATE: u32 = 44_100;
const TAIL_SECS: f32 = 0.3;

// The thing for an `<audio>` tag: `data:audio/wav;base64,...`
pub fn signature_audio_src(sig: &SonicSignature, event: SignatureEvent) -> String {
    let wav = render_wav(sig, event);
    format!("data:audio/wav;base64,{}", STANDARD.encode(&wav))
}

/// Render the whole schedule to a 16-bit stereo WAV byte vector.
pub fn render_wav(sig: &SonicSignature, event: SignatureEvent) -> Vec<u8> {
    let notes = sig.schedule(event);
    let total = notes
        .iter()
        .map(|n| n.start + n.dur)
        .fold(0.0_f32, f32::max)
        + TAIL_SECS;
    let frames = (total * SAMPLE_RATE as f32).ceil() as usize;

    let mut left = vec![0.0_f32; frames];
    let mut right = vec![0.0_f32; frames];
    for n in &notes {
        mix_note(n, &mut left, &mut right);
    }
    prevent_clipping(&mut left, &mut right);
    encode_wav_stereo(&left, &right)
}

fn mix_note(n: &NoteEvent, left: &mut [f32], right: &mut [f32]) {
    let sr = SAMPLE_RATE as f32;
    let start = (n.start * sr) as usize;
    let count = (n.dur * sr).ceil() as usize;
    // equal-power pan
    let lg = (((1.0 - n.pan) * 0.5).max(0.0)).sqrt();
    let rg = (((1.0 + n.pan) * 0.5).max(0.0)).sqrt();

    let ks = (n.timbre == Timbre::Pluck).then(|| karplus(n.freq, count));

    let dphase = n.freq / sr;
    let mut phase = 0.0_f32;
    for i in 0..count {
        let idx = start + i;
        if idx >= left.len() {
            break;
        }
        let t = i as f32 / sr;
        let amp = envelope(t, n) * n.gain;
        let wave = match n.timbre {
            Timbre::Sine => (phase * std::f32::consts::TAU).sin(),
            Timbre::Triangle => 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0,
            Timbre::Pluck => ks.as_ref().unwrap()[i],
        };
        let s = wave * amp;
        left[idx] += s * lg;
        right[idx] += s * rg;
        phase += dphase;
        if phase >= 1.0 {
            phase -= 1.0;
        }
    }
}

/// Linear attack / release envelope (1.0 in the sustain region).
fn envelope(t: f32, n: &NoteEvent) -> f32 {
    if t < n.attack {
        (t / n.attack).clamp(0.0, 1.0)
    } else if t > n.dur - n.release {
        ((n.dur - t) / n.release).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Karplus–Strong, deterministic excitation so the WAV is identical every render
/// (lets you cache by signature+event).
fn karplus(freq: f32, len: usize) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let n = ((sr / freq).round() as usize).max(2);
    let decay = 0.9945_f32;

    // seeded LCG -> reproducible noise burst
    let mut seed = (freq.to_bits()).max(1);
    let mut rng = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / 8_388_608.0 - 1.0
    };
    let mut line: Vec<f32> = (0..n).map(|_| rng()).collect();

    let mut out = vec![0.0_f32; len];
    let mut idx = 0;
    for s in out.iter_mut() {
        let cur = line[idx];
        let nxt = line[(idx + 1) % n];
        *s = cur;
        line[idx] = (cur + nxt) * 0.5 * decay; // averaging = string damping
        idx = (idx + 1) % n;
    }
    out
}

/// Only attenuate if we'd clip — keeps quiet events (mute) quieter than loud
/// ones (join) instead of normalizing everything to full scale.
fn prevent_clipping(left: &mut [f32], right: &mut [f32]) {
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0_f32, |m, &x| m.max(x.abs()));
    if peak > 0.9 {
        let g = 0.9 / peak;
        for x in left.iter_mut().chain(right.iter_mut()) {
            *x *= g;
        }
    }
}

fn encode_wav_stereo(left: &[f32], right: &[f32]) -> Vec<u8> {
    let frames = left.len();
    let channels: u16 = 2;
    let bits: u16 = 16;
    let block_align = channels * (bits / 8);
    let byte_rate = SAMPLE_RATE * block_align as u32;
    let data_len = (frames * block_align as usize) as u32;

    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // format = PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        for &sample in &[left[i], right[i]] {
            let v = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    buf
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileAudio {
    pub joined: String,
    pub left: String,
    pub muted: String,
    pub unmuted: String,
    pub deafened: String,
    pub undeafened: String,
}

impl ProfileAudio {
    pub fn new(sig: SonicSignature) -> Self {
        Self {
            joined: signature_audio_src(&sig, SignatureEvent::Joined),
            left: signature_audio_src(&sig, SignatureEvent::Left),
            muted: signature_audio_src(&sig, SignatureEvent::Muted),
            unmuted: signature_audio_src(&sig, SignatureEvent::Unmuted),
            deafened: signature_audio_src(&sig, SignatureEvent::Deafened),
            undeafened: signature_audio_src(&sig, SignatureEvent::Undeafened),
        }
    }
}

impl Default for ProfileAudio {
    fn default() -> Self {
        Self {
            joined: String::new(),
            left: String::new(),
            muted: String::new(),
            unmuted: String::new(),
            deafened: String::new(),
            undeafened: String::new(),
        }
    }
}
