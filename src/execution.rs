use crate::event::TimedEvent;

use std::collections::VecDeque;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time;

use digest::generic_array::{sequence::*, typenum::consts::*, GenericArray};
use digest::Digest;
use meowhash::MeowHasher;

/// Execution mode. Controls whether the session is playing or recording
/// commands.
#[derive(Debug, Clone)]
pub enum Execution {
    /// Normal execution. User inputs are processed normally.
    Normal,
    /// Recording user inputs to log.
    Recording {
        /// Events being recorded.
        events: Vec<TimedEvent>,
        /// Frames being recorded.
        recorder: FrameRecorder,
        /// Start time of recording.
        start: time::Instant,
        /// Path to save recording to.
        path: PathBuf,
        /// Whether this is a test recording.
        test: bool,
    },
    /// Replaying inputs from log.
    Replaying {
        /// Events being replayed.
        events: VecDeque<TimedEvent>,
        /// Frames being replayed.
        recorder: FrameRecorder,
        /// Start time of the playback.
        start: time::Instant,
        /// Path to read events from.
        path: PathBuf,
        /// Whether this is a test replay.
        test: bool,
        /// Replay result.
        result: ReplayResult,
    },
}

impl Execution {
    /// Create a normal execution.
    pub fn normal() -> io::Result<Self> {
        Ok(Self::Normal)
    }

    /// Create a recording.
    pub fn recording<P: AsRef<Path>>(path: P, test: bool) -> io::Result<Self> {
        Ok(Self::Recording {
            events: Vec::new(),
            recorder: FrameRecorder::new(),
            start: time::Instant::now(),
            path: path.as_ref().to_path_buf(),
            test,
        })
    }

    /// Create a replay.
    pub fn replaying<P: AsRef<Path>>(path: P, test: bool) -> io::Result<Self> {
        let mut events = VecDeque::new();
        let path = path.as_ref();
        let abs_path = path.canonicalize()?;

        let mut frames = Vec::new();
        if test {
            let path = path.with_extension("digest");
            match File::open(&path) {
                Ok(f) => {
                    let r = io::BufReader::new(f);
                    for line in r.lines() {
                        let line = line?;
                        let hash =
                            Hash::from_str(line.as_str()).map_err(|e| {
                                io::Error::new(io::ErrorKind::InvalidInput, e)
                            })?;
                        frames.push(hash);
                    }
                }
                Err(e) => {
                    return Err(io::Error::new(
                        e.kind(),
                        format!("{}: {}", path.display(), e),
                    ));
                }
            }
        }

        match File::open(&path) {
            Ok(f) => {
                let r = io::BufReader::new(f);
                for (i, line) in r.lines().enumerate() {
                    let line = line?;
                    let ev = TimedEvent::from_str(&line).map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("{}:{}: {}", abs_path.display(), i + 1, e),
                        )
                    })?;
                    events.push_back(ev);
                }
                Ok(Self::Replaying {
                    events,
                    start: time::Instant::now(),
                    path: path.to_path_buf(),
                    test,
                    recorder: FrameRecorder::from(frames),
                    result: ReplayResult::new(),
                })
            }
            Err(e) => Err(io::Error::new(
                e.kind(),
                format!("{}: {}", path.display(), e),
            )),
        }
    }

    pub fn is_normal(&self) -> bool {
        if let Execution::Normal = self {
            true
        } else {
            false
        }
    }

    pub fn record(&mut self, data: &[u8]) {
        match self {
            Self::Recording { recorder, .. } => {
                recorder.record_frame(data);
            }
            Self::Replaying {
                test: true,
                result,
                recorder,
                ..
            } => {
                result.record(recorder.verify_frame(data));
            }
            _ => {}
        }
    }

    pub fn stop_recording(&mut self) -> Option<(PathBuf, bool)> {
        // TODO: (rust) Style
        let result = if let Execution::Recording {
            events,
            path,
            test,
            recorder,
            ..
        } = &self
        {
            if let Ok(mut f) = File::create(path) {
                use std::io::Write;

                for ev in events.clone() {
                    if let Err(e) = writeln!(&mut f, "{}", String::from(ev)) {
                        panic!("error while saving recording: {}", e);
                    }
                }

                if let Ok(mut f) = File::create(path.with_extension("digest")) {
                    for digest in &recorder.frames {
                        if let Err(e) = writeln!(&mut f, "{}", digest) {
                            panic!("error while saving recording: {}", e);
                        }
                    }
                }
            }
            Some((path.clone(), *test))
        } else {
            None
        };

        *self = Execution::Normal;

        result
    }

    pub fn stop_playing(&mut self) {
        if let Execution::Replaying { result, .. } = &self {
            info!("replaying: {}", result.summary());
        }
        *self = Execution::Normal;
    }
}

impl Default for Execution {
    fn default() -> Self {
        Execution::Normal
    }
}

/// Records and verifies frames being replayed.
#[derive(Debug, Clone)]
pub struct FrameRecorder {
    frames: VecDeque<Hash>,
    last_verified: Option<Hash>,
}

impl FrameRecorder {
    fn new() -> Self {
        Self {
            frames: VecDeque::new(),
            last_verified: None,
        }
    }

    fn from(frames: Vec<Hash>) -> Self {
        Self {
            frames: frames.into(),
            last_verified: None,
        }
    }

    fn record_frame(&mut self, data: &[u8]) {
        let hash = Self::hash(data);

        if self.frames.back().map(|h| h != &hash).unwrap_or(true) {
            self.frames.push_back(hash);
        }
    }

    fn verify_frame(&mut self, data: &[u8]) -> VerifyResult {
        let actual = Self::hash(data);

        if Some(actual.clone()) == self.last_verified {
            return VerifyResult::Stale(actual);
        }
        self.last_verified = Some(actual.clone());

        if let Some(expected) = self.frames.pop_front() {
            if actual == expected {
                VerifyResult::Okay(actual)
            } else {
                VerifyResult::Failed(actual, expected)
            }
        } else {
            VerifyResult::EOF
        }
    }

    ////////////////////////////////////////////////////////////////////////////

    fn hash(data: &[u8]) -> Hash {
        let bytes: GenericArray<u8, U64> = MeowHasher::digest(data);
        let (prefix, _): (GenericArray<u8, U4>, _) = bytes.split();

        Hash(prefix.into())
    }
}

#[derive(Debug, Clone)]
pub struct ReplayResult {
    verify_results: Vec<VerifyResult>,
}

impl ReplayResult {
    fn new() -> Self {
        ReplayResult {
            verify_results: Vec::new(),
        }
    }

    fn record(&mut self, result: VerifyResult) {
        match &result {
            VerifyResult::Okay(actual) => {
                info!("replaying: {} OK", actual);
            }
            VerifyResult::Failed(actual, expected) => {
                error!("replaying: {} != {}", actual, expected);
                // TODO: Stop replaying
            }
            VerifyResult::EOF => {
                error!("replaying: EOF");
            }
            VerifyResult::Stale { .. } => {}
        }
        self.verify_results.push(result);
    }

    fn summary(&self) -> String {
        let mut okay = 0;
        let mut failed = 0;

        for r in self.verify_results.iter() {
            match r {
                VerifyResult::Okay(_) => {
                    okay += 1;
                }
                VerifyResult::Failed(_, _) => {
                    failed += 1;
                }
                VerifyResult::EOF => {
                    return format!("EOF reached");
                }
                VerifyResult::Stale { .. } => {}
            }
        }
        let total = (okay + failed) as f32;

        format!(
            "{:.1}% OK, {:.1}% FAILED",
            okay as f32 / total * 100.,
            failed as f32 / total * 100.
        )
    }
}

#[derive(Debug, Clone)]
pub enum VerifyResult {
    /// The hash has already been verified.
    Stale(Hash),
    /// The actual and expected hashes match.
    Okay(Hash),
    /// The actual and expected hashes don't match.
    Failed(Hash, Hash),
    /// There are no further expected hashes.
    EOF,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Hash([u8; 4]);

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for byte in self.0.iter() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl FromStr for Hash {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let val = |c: u8| match c {
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'0'..=b'9' => Ok(c - b'0'),
            _ => Err(format!("invalid hex character {:?}", c)),
        };

        let mut hash: Vec<u8> = Vec::new();
        for pair in input.bytes().collect::<Vec<u8>>().chunks(2) {
            match pair {
                &[l, r] => {
                    let left = val(l)? << 4;
                    let right = val(r)?;

                    hash.push(left | right);
                }
                _ => return Err(format!("invalid hex string: {:?}", input)),
            }
        }

        let mut array = [0; 4];
        array.copy_from_slice(hash.as_slice());

        Ok(Hash(array))
    }
}
