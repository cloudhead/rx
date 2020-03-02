use crate::event::TimedEvent;

use std::collections::VecDeque;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time;

use rgx::color::{Bgra8, Rgba8};
use seahash::SeaHasher;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GifMode {
    Ignore,
    Record,
}

/// Determines whether frame digests are recorded, verified or ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestMode {
    /// Verify digests.
    Verify,
    /// Record digests.
    Record,
    /// Ignore digest.
    Ignore,
}

pub struct DigestState {
    pub mode: DigestMode,
    pub path: Option<PathBuf>,
}

impl DigestState {
    pub fn from<P: AsRef<Path>>(mode: DigestMode, path: P) -> io::Result<Self> {
        match mode {
            DigestMode::Verify => Self::verify(path),
            DigestMode::Record => Self::record(path),
            DigestMode::Ignore => Self::ignore(),
        }
    }

    pub fn verify<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut frames = Vec::new();
        let path = path.as_ref();

        match File::open(&path) {
            Ok(f) => {
                let r = io::BufReader::new(f);
                for line in r.lines() {
                    let line = line?;
                    let hash = Hash::from_str(line.as_str())
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
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

        Ok(Self {
            mode: DigestMode::Verify,
            path: Some(path.into()),
        })
    }

    pub fn record<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Ok(Self {
            mode: DigestMode::Record,
            path: Some(path.as_ref().into()),
        })
    }

    pub fn ignore() -> io::Result<Self> {
        Ok(Self {
            mode: DigestMode::Ignore,
            path: None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum ExecutionMode {
    Normal,
    Record(PathBuf, DigestMode, GifMode),
    Replay(PathBuf, DigestMode),
}

/// Execution mode. Controls whether the session is playing or recording
/// commands.
// TODO: Make this a `struct` and have `ExecutionMode`.
pub enum Execution {
    /// Normal execution. User inputs are processed normally.
    Normal,
    /// Recording user inputs to log.
    Recording {
        /// Events being recorded.
        events: Vec<TimedEvent>,
        /// Start time of recording.
        start: time::Instant,
        /// Path to save recording to.
        path: PathBuf,
        /// Digest mode.
        digest: DigestState,
        /// Frame recorder.
        recorder: FrameRecorder,
    },
    /// Replaying inputs from log.
    Replaying {
        /// Events being replayed.
        events: VecDeque<TimedEvent>,
        /// Start time of the playback.
        start: time::Instant,
        /// Path to read events from.
        path: PathBuf,
        /// Digest mode.
        digest: DigestState,
        /// Replay result.
        result: ReplayResult,
        /// Frame recorder.
        recorder: FrameRecorder,
    },
}

impl Execution {
    /// Create a normal execution.
    pub fn normal() -> io::Result<Self> {
        Ok(Self::Normal)
    }

    /// Create a recording.
    pub fn recording<P: AsRef<Path>>(
        path: P,
        digest_mode: DigestMode,
        w: u16,
        h: u16,
        gif_mode: GifMode,
    ) -> io::Result<Self> {
        use io::{Error, ErrorKind};

        let path = path.as_ref();
        let file_name: &Path = path
            .file_name()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                format!("invalid path {:?}", path),
            ))?
            .as_ref();

        std::fs::create_dir_all(path)?;

        let digest = DigestState::from(digest_mode, path.join(file_name).with_extension("digest"))?;
        let gif_recorder = if gif_mode == GifMode::Record {
            GifRecorder::new(path.join(file_name).with_extension("gif"), w, h)?
        } else {
            GifRecorder::dummy()
        };
        let recorder = FrameRecorder::new(gif_recorder, gif_mode, digest_mode);

        Ok(Self::Recording {
            events: Vec::new(),
            start: time::Instant::now(),
            path: path.to_path_buf(),
            digest,
            recorder,
        })
    }

    /// Create a replay.
    pub fn replaying<P: AsRef<Path>>(path: P, mode: DigestMode) -> io::Result<Self> {
        use io::{Error, ErrorKind};

        let mut events = VecDeque::new();
        let path = path.as_ref();

        let file_name: &Path = path
            .file_name()
            .ok_or(Error::new(
                ErrorKind::InvalidInput,
                format!("invalid path {:?}", path),
            ))?
            .as_ref();

        let digest = DigestState::from(mode, path.join(file_name).with_extension("digest"))?;

        let recorder = match &digest {
            DigestState {
                path: Some(path),
                mode: DigestMode::Verify,
            } => {
                let mut frames = Vec::new();

                match File::open(&path) {
                    Ok(f) => {
                        let r = io::BufReader::new(f);
                        for line in r.lines() {
                            let line = line?;
                            let hash = Hash::from_str(line.as_str())
                                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
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
                FrameRecorder::from(frames, mode)
            }
            _ => FrameRecorder::new(GifRecorder::dummy(), GifMode::Ignore, mode),
        };

        let events_path = path.join(file_name).with_extension("events");
        match File::open(&events_path) {
            Ok(f) => {
                let r = io::BufReader::new(f);
                for (i, line) in r.lines().enumerate() {
                    let line = line?;
                    let ev = TimedEvent::from_str(&line).map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("{}:{}: {}", events_path.display(), i + 1, e),
                        )
                    })?;
                    events.push_back(ev);
                }
                Ok(Self::Replaying {
                    events,
                    start: time::Instant::now(),
                    path: path.to_path_buf(),
                    digest,
                    result: ReplayResult::new(),
                    recorder,
                })
            }
            Err(e) => Err(io::Error::new(
                e.kind(),
                format!("{}: {}", events_path.display(), e),
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

    pub fn is_recording(&self) -> bool {
        if let Execution::Recording { .. } = self {
            true
        } else {
            false
        }
    }

    pub fn record(&mut self, data: &[Bgra8]) {
        match self {
            // Replaying and verifying digests.
            Self::Replaying {
                digest:
                    DigestState {
                        mode: DigestMode::Verify,
                        ..
                    },
                result,
                recorder,
                ..
            } => {
                result.record(recorder.verify_frame(data));
            }
            // Replaying/Recording and recording digests.
            Self::Replaying { recorder, .. } | Self::Recording { recorder, .. } => {
                recorder.record_frame(data);
            }
            _ => {}
        }
    }

    ////////////////////////////////////////////////////////////////////////////

    pub fn stop_recording(&mut self) -> io::Result<PathBuf> {
        use io::{Error, ErrorKind};

        let result = if let Execution::Recording {
            events,
            path,
            digest,
            recorder,
            ..
        } = self
        {
            recorder.finish()?;

            let file_name: &Path = path
                .file_name()
                .ok_or(Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid path {:?}", path),
                ))?
                .as_ref();

            let mut f = File::create(path.join(file_name.with_extension("events")))?;
            for ev in events.clone() {
                writeln!(&mut f, "{}", String::from(ev))?;
            }

            if let DigestState {
                mode: DigestMode::Record,
                path: Some(path),
                ..
            } = digest
            {
                Execution::write_digest(recorder, path)?;
            }

            Ok(path.clone())
        } else {
            panic!("record finalizer called outside of recording context")
        };

        if result.is_ok() {
            *self = Execution::Normal;
        }
        result
    }

    pub fn finalize_replaying(&self) -> io::Result<PathBuf> {
        if let Execution::Replaying {
            digest:
                DigestState {
                    mode: DigestMode::Record,
                    path: Some(path),
                    ..
                },
            recorder,
            ..
        } = &self
        {
            Execution::write_digest(recorder, path)?;
            Ok(path.clone())
        } else {
            panic!("replay finalizer called outside of replay context")
        }
    }

    ////////////////////////////////////////////////////////////////////////////

    fn write_digest<P: AsRef<Path>>(recorder: &FrameRecorder, path: P) -> io::Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = File::create(path)?;

        for frame in &recorder.frames {
            writeln!(&mut f, "{}", frame)?;
        }
        Ok(())
    }
}

impl Default for Execution {
    fn default() -> Self {
        Execution::Normal
    }
}

pub struct GifRecorder {
    width: u16,
    height: u16,
    encoder: Option<gif::Encoder<Box<File>>>,
    frames: Vec<(time::Instant, Vec<u8>)>,
}

impl GifRecorder {
    const GIF_ENCODING_SPEED: i32 = 30;

    pub fn new<P: AsRef<Path>>(path: P, width: u16, height: u16) -> io::Result<Self> {
        let file = Box::new(File::create(path.as_ref())?);
        let encoder = Some(gif::Encoder::new(file, width, height, &[])?);

        Ok(Self {
            width,
            height,
            encoder,
            frames: Vec::new(),
        })
    }

    fn dummy() -> Self {
        Self {
            width: 0,
            height: 0,
            encoder: None,
            frames: Vec::new(),
        }
    }

    fn is_dummy(&self) -> bool {
        self.width == 0 && self.height == 0
    }

    fn record(&mut self, data: &[Bgra8]) {
        if self.is_dummy() {
            return;
        }
        let now = time::Instant::now();
        let mut gif_data: Vec<u8> = Vec::with_capacity(data.len());
        // TODO: (perf) Is it faster to convert to `Vec<Rgba8>` and then `align_to`?
        for bgra in data.iter().cloned() {
            let rgba: Rgba8 = bgra.into();
            gif_data.extend_from_slice(&[rgba.r, rgba.g, rgba.b]);
        }
        self.frames.push((now, gif_data));
    }

    fn finish(&mut self) -> io::Result<()> {
        use std::convert::TryInto;

        if let Some(encoder) = &mut self.encoder {
            for (i, (t1, gif_data)) in self.frames.iter().enumerate() {
                let delay = if let Some((t2, _)) = self.frames.get(i + 1) {
                    *t2 - *t1
                } else {
                    // Let the last frame linger for a second...
                    time::Duration::from_secs(1)
                };

                let mut frame = gif::Frame::from_rgb_speed(
                    self.width,
                    self.height,
                    &gif_data,
                    Self::GIF_ENCODING_SPEED,
                );
                frame.dispose = gif::DisposalMethod::Background;
                frame.delay = (delay.as_millis() / 10)
                    .try_into()
                    .expect("`delay` is not an unreasonably large number");

                encoder.write_frame(&frame)?;
            }
        }
        Ok(())
    }
}

/// Records and verifies frames being replayed.
pub struct FrameRecorder {
    frames: VecDeque<Hash>,
    last_verified: Option<Hash>,
    gif_recorder: GifRecorder,
    gif_mode: GifMode,
    digest_mode: DigestMode,
}

impl FrameRecorder {
    fn new(gif_recorder: GifRecorder, gif_mode: GifMode, digest_mode: DigestMode) -> Self {
        Self {
            frames: VecDeque::new(),
            last_verified: None,
            gif_recorder,
            gif_mode,
            digest_mode,
        }
    }

    fn from(frames: Vec<Hash>, digest_mode: DigestMode) -> Self {
        Self {
            frames: frames.into(),
            last_verified: None,
            gif_recorder: GifRecorder::dummy(),
            gif_mode: GifMode::Ignore,
            digest_mode,
        }
    }

    fn record_frame(&mut self, data: &[Bgra8]) {
        let hash = Self::hash(data);

        if self.frames.back().map(|h| h != &hash).unwrap_or(true) {
            debug!("frame: {}", hash);

            if self.digest_mode == DigestMode::Record || self.gif_mode == GifMode::Record {
                self.frames.push_back(hash);
            }

            self.gif_recorder.record(data);
        }
    }

    fn verify_frame(&mut self, data: &[Bgra8]) -> VerifyResult {
        let actual = Self::hash(data);

        if self.frames.is_empty() {
            return VerifyResult::EOF;
        }
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

    fn finish(&mut self) -> io::Result<()> {
        self.gif_recorder.finish()
    }

    fn hash(data: &[Bgra8]) -> Hash {
        use std::hash::Hasher;

        let mut hasher = SeaHasher::new();
        let (_, data, _) = unsafe { data.align_to::<u8>() };

        hasher.write(data);
        Hash(hasher.finish())
    }
}

#[derive(Debug, Clone)]
pub struct ReplayResult {
    verify_results: Vec<VerifyResult>,
    eof: bool,
    okay_count: u32,
    failed_count: u32,
    stale_count: u32,
}

impl ReplayResult {
    pub fn is_ok(&self) -> bool {
        self.failed_count == 0
    }

    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    pub fn is_done(&self) -> bool {
        self.eof
    }

    pub fn summary(&self) -> String {
        let total = (self.okay_count + self.failed_count) as f32;

        format!(
            "{:.1}% OK, {:.1}% FAILED",
            self.okay_count as f32 / total * 100.,
            self.failed_count as f32 / total * 100.
        )
    }

    ////////////////////////////////////////////////////////////////////////////

    fn new() -> Self {
        ReplayResult {
            verify_results: Vec::new(),
            okay_count: 0,
            failed_count: 0,
            stale_count: 0,
            eof: false,
        }
    }

    fn record(&mut self, result: VerifyResult) {
        match &result {
            VerifyResult::Okay(actual) => {
                info!("verify: {} OK", actual);
                self.okay_count += 1;
            }
            VerifyResult::Failed(actual, expected) => {
                error!("verify: {} != {}", actual, expected);
                self.failed_count += 1;
                // TODO: Stop replaying
            }
            VerifyResult::EOF => {
                self.eof = true;
            }
            VerifyResult::Stale { .. } => {
                self.stale_count += 1;
            }
        }
        self.verify_results.push(result);
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
pub struct Hash(u64);

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl FromStr for Hash {
    type Err = std::num::ParseIntError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        u64::from_str_radix(input, 16).map(Hash)
    }
}
