//! The lines run this session, per family (specs/completion, "History").

/// A line that was run, how often, and when last (in lines recorded).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Record {
    pub line: String,
    pub uses: u32,
    pub last: u64,
}

/// The history of one family: at most `capacity` lines, the least recently used going first.
#[derive(Default)]
pub(crate) struct History {
    pub records: Vec<Record>,
    /// The lines recorded so far: the clock of frecency.
    pub clock: u64,
}

impl History {
    /// Counts a line run again, or adds it.
    pub(crate) fn record(&mut self, line: &str, capacity: usize) {
        if capacity == 0 {
            return;
        }
        self.clock += 1;
        if let Some(record) = self.records.iter_mut().find(|record| record.line == line) {
            record.uses = record.uses.saturating_add(1);
            record.last = self.clock;
            return;
        }
        self.records.push(Record { line: line.to_string(), uses: 1, last: self.clock });
        self.trim(capacity);
    }

    /// Drops the least recently used past `capacity`.
    pub(crate) fn trim(&mut self, capacity: usize) {
        while self.records.len() > capacity {
            // Records are pushed in order of their first use, not their last: look for the oldest.
            let oldest = self.records.iter().enumerate().min_by_key(|(_, record)| record.last);
            let Some((at, _)) = oldest else { break };
            self.records.remove(at);
        }
    }

    /// Recent and frequent lines rank higher: `uses / (1 + age / 100)`.
    pub(crate) fn frecency(&self, record: &Record) -> f32 {
        let age = self.clock.saturating_sub(record.last) as f32;
        record.uses as f32 / (1.0 + age / 100.0)
    }
}
