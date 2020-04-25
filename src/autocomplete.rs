use std::ffi::OsString;
use std::path::PathBuf;
use std::{iter, ops::Range, path, path::Path, vec};

pub trait Completer: std::fmt::Debug {
    type Options: Default;

    fn complete(&self, input: &str, opts: Self::Options) -> Vec<String>;
}

#[derive(Debug)]
pub struct Autocomplete<T> {
    /// Available command completions.
    completions: Option<iter::Peekable<iter::Cycle<vec::IntoIter<String>>>>,
    /// Range within the input that is being completed.
    range: Range<usize>,
    /// The completer we are using to find candidates.
    completer: T,
}

impl<T: Completer> Autocomplete<T> {
    pub fn new(completer: T) -> Self {
        Self {
            completions: None,
            range: 0..0,
            completer,
        }
    }

    pub fn invalidate(&mut self) {
        self.completions = None;
        self.range = 0..0;
    }

    pub fn next(&mut self, input: &str, cursor: usize) -> Option<(String, Range<usize>)> {
        match &mut self.completions {
            Some(iter) => {
                iter.next().map(|completion| {
                    let range = self.range.clone();
                    // New completion range starts where current one did, but ends
                    // based on new completion length.
                    self.range = self.range.start..self.range.start + completion.len();

                    (completion, range)
                })
            }
            None => {
                let candidates = self
                    .completer
                    .complete(&input[..cursor], Default::default());
                let mut iter = candidates.into_iter().cycle().peekable();

                iter.next().map(|completion| {
                    if iter.peek() == Some(&completion) {
                        // If there's only one match, we can go ahead and invalidate the rest
                        // of the completions so that next time this function is called, it
                        // loads new matches based on this one match.
                        self.invalidate();
                    } else {
                        // Otherwise, base the range on the position returned from the
                        // completer.
                        self.range = cursor..cursor + completion.len();
                        self.completions = Some(iter);
                    }
                    (completion, cursor..cursor)
                })
            }
        }
    }
}

#[derive(Debug)]
pub struct FileCompleter {
    cwd: path::PathBuf,
    extensions: Vec<OsString>,
}

#[derive(Default)]
pub struct FileCompleterOpts {
    pub directories: bool,
}

impl FileCompleter {
    pub fn new<P: AsRef<Path>>(cwd: P, extensions: &[&str]) -> Self {
        Self {
            cwd: cwd.as_ref().into(),
            extensions: extensions.iter().map(|e| e.to_owned().into()).collect(),
        }
    }
}

impl Completer for FileCompleter {
    type Options = FileCompleterOpts;

    fn complete(&self, input: &str, opts: Self::Options) -> Vec<String> {
        // The five possible cases:
        // 1. "|"            -> ["rx.png"]
        // 2. "rx.|"         -> ["png"]
        // 3. "assets/|"     -> ["cursors.png"]
        // 4. "assets/curs|" -> ["ors.png"]
        // 5. "assets|"      -> ["assets/"]
        let (search_dir, prefix) = if let Some(pos) = input.chars().rev().position(|s| s == '/') {
            let idx = input.len() - pos;
            let (dir, file) = input.split_at(idx);

            (self.cwd.join(dir), file)
        } else {
            (self.cwd.clone(), input)
        };

        let mut candidates: Vec<(String, bool)> = match self.paths(&search_dir) {
            Ok(paths) => paths
                .map(|p| {
                    (
                        p.to_string_lossy().into_owned(),
                        search_dir.join(p).is_dir(),
                    )
                })
                .filter(|(_, is_dir)| if opts.directories { *is_dir } else { true })
                .collect(),
            Err(_) => vec![],
        };

        if !prefix.is_empty() {
            candidates.retain(|(c, _)| c.starts_with(prefix));

            for (c, _) in candidates.iter_mut() {
                c.replace_range(..prefix.len(), "");
            }
        }

        let len = candidates.len();
        if let Some((ref mut c, is_dir)) = candidates.first_mut() {
            if *is_dir && len == 1 {
                c.push_str("/");
                return vec![c.to_owned()];
            }
        }

        candidates.into_iter().map(|(c, _)| c).collect()
    }
}

impl FileCompleter {
    pub fn paths<P: AsRef<Path>>(&self, dir: P) -> std::io::Result<impl Iterator<Item = PathBuf>> {
        let path = dir.as_ref();
        let mut paths = Vec::new();

        for entry in path.read_dir()? {
            let entry = entry?;
            let path = entry.path();

            if let Some(file_name) = path.file_name() {
                if file_name.to_str().map_or(false, |s| s.starts_with('.')) {
                    continue;
                }
                let known = path.extension().map_or(false, |e| {
                    e == "rx" || self.extensions.iter().any(|ext| ext == e)
                });
                if known || path.is_dir() {
                    paths.push(file_name.into());
                }
            }
        }
        Ok(paths.into_iter())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::{self, File};
    use tempfile;

    #[derive(Debug)]
    pub struct StaticCompleter {
        candidates: Vec<String>,
    }

    impl StaticCompleter {
        pub fn new(candidates: &[&str]) -> Self {
            Self {
                candidates: candidates.into_iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    impl Completer for StaticCompleter {
        type Options = ();

        fn complete(&self, _input: &str, _opts: ()) -> Vec<String> {
            self.candidates.clone()
        }
    }

    #[test]
    fn test_autocomplete_static() {
        let completer = StaticCompleter::new(&["one.png", "two.png", "three.png"]);
        let mut auto = Autocomplete::new(completer);

        assert_eq!(Some(("one.png".to_owned(), 0..0)), auto.next("", 0),);
        assert_eq!(Some(("two.png".to_owned(), 0..7)), auto.next("", 0),);
        assert_eq!(Some(("three.png".to_owned(), 0..7)), auto.next("", 0),);
        assert_eq!(Some(("one.png".to_owned(), 0..9)), auto.next("", 0),);
        assert_eq!(Some(("two.png".to_owned(), 0..7)), auto.next("", 0),);
    }

    #[test]
    fn test_autocomplete_file() {
        let tmp = tempfile::tempdir().unwrap();

        // Hidden directories should be ignored by the completer.
        fs::create_dir(tmp.path().join(".git")).unwrap();
        // Normal directories *shouldn't* be ignored.
        fs::create_dir(tmp.path().join("backup")).unwrap();
        // Non-PNG files should be ignored by the completer.
        for file_name in &["one.png", "two.png", "three.png", "other.jpeg", ".rxrc"] {
            let path = tmp.path().join(file_name);
            File::create(path).unwrap();
        }
        for file_name in &["four.png", "five.png", "six.png"] {
            let path = tmp.path().join("backup").join(file_name);
            File::create(path).unwrap();
        }

        let completer = FileCompleter::new(tmp.path(), &["png"]);
        let mut auto = Autocomplete::new(completer);

        assert_eq!(Some(("three.png".to_owned(), 0..0)), auto.next("", 0),);
        assert_eq!(
            Some(("two.png".to_owned(), 0..9)),
            auto.next("three.png", 0),
        );
        assert_eq!(Some(("one.png".to_owned(), 0..7)), auto.next("two.png", 7),);
        assert_eq!(Some(("backup".to_owned(), 0..7)), auto.next("one.png", 7),);
        assert_eq!(Some(("three.png".to_owned(), 0..6)), auto.next("backup", 6),);

        // Invalidate completions, as we're insert a '/' into the input.
        auto.invalidate();

        assert_eq!(Some(("six.png".to_owned(), 7..7)), auto.next("backup/", 7),);
        assert_eq!(
            Some(("five.png".to_owned(), 7..14)),
            auto.next("backup/six.png", 14),
        );
        assert_eq!(
            Some(("four.png".to_owned(), 7..15)),
            auto.next("backup/five.png", 15),
        );
    }
}
