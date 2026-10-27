//! What an import did, and everything it could not take.

use std::collections::BTreeMap;

#[derive(Default, Debug)]
pub struct Report {
    /// kind -> (created, already there)
    pub counts: BTreeMap<&'static str, (usize, usize)>,
    pub problems: Vec<String>,
}

impl Report {
    pub(super) fn created(&mut self, kind: &'static str) {
        self.counts.entry(kind).or_default().0 += 1;
    }
    pub(super) fn existed(&mut self, kind: &'static str) {
        self.counts.entry(kind).or_default().1 += 1;
    }
    pub(super) fn problem(&mut self, msg: impl Into<String>) {
        self.problems.push(msg.into());
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (kind, (new, old)) in &self.counts {
            writeln!(f, "{kind:<14} created {new:>6}   already there {old:>6}")?;
        }
        if self.problems.is_empty() {
            writeln!(f, "no problems")?;
        } else {
            writeln!(f, "{} problem(s):", self.problems.len())?;
            for p in &self.problems {
                writeln!(f, "  - {p}")?;
            }
        }
        Ok(())
    }
}
