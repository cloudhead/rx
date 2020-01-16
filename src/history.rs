use std::collections::VecDeque;
use std::io;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct History {
    /// History path.
    pub path: std::path::PathBuf,
    /// The history of commands entered.
    entries: VecDeque<String>,
    /// The current cursor into the history.
    cursor: Option<usize>,
    /// Maximum number of entries.
    capacity: usize,
}

impl History {
    pub fn new<P: AsRef<Path>>(path: P, capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            cursor: None,
            capacity,
            path: path.as_ref().into(),
        }
    }

    pub fn load(&mut self) -> io::Result<()> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        if let Ok(f) = File::open(&self.path) {
            let r = BufReader::new(f);

            for line in r.lines() {
                self.add(line?);
            }
        }
        Ok(())
    }

    pub fn save(&self) -> io::Result<()> {
        use std::fs::File;
        use std::io::{BufWriter, Write};

        if self.is_empty() {
            return Ok(());
        }

        let parent = self
            .path
            .parent()
            .expect("saving to a path with a parent directory");
        std::fs::create_dir_all(parent)?;

        let f = File::create(&self.path)?;
        let mut w = BufWriter::new(f);

        for entry in self.entries.iter().rev() {
            w.write_all(entry.as_bytes())?;
            w.write_all(b"\n")?;
        }
        w.flush()
    }

    pub fn add<S: Into<String>>(&mut self, s: S) {
        let entry = s.into();
        if self.entries.front() != Some(&entry) {
            self.entries.push_front(entry);
            self.entries.truncate(self.capacity);
        }
    }

    pub fn reset(&mut self) {
        self.cursor = None;
    }

    pub fn clear(&mut self) {
        self.entries.clear()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn next(&mut self, prefix: &str) -> Option<&str> {
        let start = self.len() - self.cursor.unwrap_or(0);

        match self
            .entries
            .iter()
            .rev()
            .skip(start)
            .position(|e| e.starts_with(prefix) && e != prefix)
        {
            Some(index) => {
                let cursor = self.len() - 1 - start - index;

                self.cursor = Some(cursor);
                self.get(cursor)
            }
            None => {
                self.cursor = None;
                None
            }
        }
    }

    pub fn prev(&mut self, prefix: &str) -> Option<&str> {
        let start = self.cursor.map(|c| c + 1).unwrap_or(0);

        self.entries
            .iter()
            .skip(start)
            .position(|e| e.starts_with(prefix) && e != prefix)
            .and_then(move |index| {
                let cursor = start + index;

                self.cursor = Some(cursor);
                self.get(cursor)
            })
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|s| s.as_str())
    }
}

///////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod test {
    use super::History;
    use tempfile;

    #[test]
    fn test_history() {
        let mut h = History::new("/dev/null", 16);

        h.add("first");
        h.add("second");
        h.add("third");
        h.add("third");

        assert_eq!(h.prev(""), Some("third"));
        assert_eq!(h.prev(""), Some("second"));
        assert_eq!(h.prev(""), Some("first"));
        assert_eq!(h.prev(""), None);
        assert_eq!(h.next(""), Some("second"));
        assert_eq!(h.next(""), Some("third"));
        assert_eq!(h.next(""), None);

        assert_eq!(h.prev(""), Some("third"));
        assert_eq!(h.next(""), None);
    }

    #[test]
    fn test_history_long() {
        let mut h = History::new("/dev/null", 16);

        h.add("first");
        h.add("second");
        h.add("third");
        h.add("fourth");
        h.add("fifth");
        h.add("sixth");

        assert_eq!(h.prev("fifth"), None);
        assert_eq!(h.prev("fift"), Some("fifth"));
        assert_eq!(h.next("second"), None);
    }

    #[test]
    fn test_history_prefix() {
        let mut h = History::new("/dev/null", 16);

        h.add("first");
        h.add("second");
        h.add("third");
        h.add("third");

        assert_eq!(h.prev("fo"), None);
        assert_eq!(h.prev("se"), Some("second"));
        assert_eq!(h.prev(""), Some("first"));
        assert_eq!(h.next("t"), Some("third"));
        assert_eq!(h.prev("th"), None);
        assert_eq!(h.next("fir"), None);
    }

    #[test]
    fn test_history_empty() {
        let mut h = History::new("/dev/null", 16);

        assert_eq!(h.prev(""), None);
        assert_eq!(h.next(""), None);
    }

    #[test]
    fn test_history_save_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".history");
        let mut h1 = History::new(&path, 16);

        h1.add("first");
        h1.add("second");
        h1.add("third");
        h1.save().unwrap();
        h1.save().unwrap();

        let mut h2 = History::new(&path, 16);
        h2.load().unwrap();

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_history_capacity() {
        let mut h = History::new("/dev/null", 3);

        h.add("first");
        h.add("second");
        h.add("third");
        h.add("fourth");
        h.add("fifth");

        assert_eq!(
            h.entries.iter().collect::<Vec<_>>().as_slice(),
            &["fifth", "fourth", "third"]
        );
    }
}
