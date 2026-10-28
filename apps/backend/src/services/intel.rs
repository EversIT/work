//! The built-in analysis: classic text statistics, no model file and no network. It is what finds
//! related issues and likely duplicates and suggests field values from
//! similar issues. It "learns" from the issues of the workspace itself: the index is rebuilt from
//! them on every request, so there is nothing to train, ship or keep in sync.
//!
//! Pure functions only; the routes feed them the issues the caller may see.

use std::collections::HashMap;

/// Words that carry no topic, in the two languages the tool ships in.
const STOPWORDS: &[&str] = &[
    // English
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "had", "her", "was",
    "one", "our", "out", "has", "have", "that", "this", "with", "from", "they", "will", "would",
    "there", "their", "what", "about", "which", "when", "make", "like", "just", "into", "than",
    "then", "them", "these", "some", "could", "should", "also", "been", "being", "does", "doing",
    "very", "more", "most", "other", "over", "such", "only", "need", "needs", "please", "issue",
    // German
    "und", "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer",
    "ist", "sind", "war", "wird", "werden", "wurde", "wurden", "nicht", "auch", "auf", "aus",
    "bei", "mit", "nach", "von", "vor", "zum", "zur", "für", "über", "als", "wie", "wenn", "dass",
    "sich", "oder", "aber", "noch", "nur", "kann", "können", "soll", "sollen", "muss", "müssen",
    "wir", "ich", "sie", "ihr", "man", "hat", "haben", "hier", "dort", "dann", "dies", "diese",
    "dieser", "dieses", "bitte", "bitte",
];

/// Splits into lower-case word stems: letters and digits only, no stopwords, with the usual
/// English and German endings cut off so `logins`, `login` and `Anmeldungen`/`Anmeldung` meet.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_lowercase)
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .map(|w| stem(&w))
        .collect()
}

fn stem(w: &str) -> String {
    const ENDINGS: &[&str] = &[
        "ungen", "ing", "ung", "ed", "es", "en", "er", "ly", "e", "s",
    ];
    for e in ENDINGS {
        if let Some(root) = w.strip_suffix(e) {
            if root.chars().count() >= 4 {
                return root.to_owned();
            }
        }
    }
    w.to_owned()
}

/// One issue as the index sees it.
#[derive(Clone, Debug)]
pub struct Doc {
    pub key: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug)]
pub struct Hit {
    /// Index into the documents the index was built from.
    pub doc: usize,
    pub score: f32,
    /// The words that made the two issues alike, strongest first.
    pub shared: Vec<String>,
}

pub struct Index {
    idf: HashMap<String, f32>,
    vecs: Vec<HashMap<String, f32>>,
}

/// Title words count five times: a title says what an issue is about. Only the start of a long
/// description is read: what follows is usually a log or a stack trace, which says little about the topic.
fn weighted_terms(title: &str, body: &str) -> HashMap<String, f32> {
    let mut tf: HashMap<String, f32> = HashMap::new();
    for t in tokenize(title) {
        *tf.entry(t).or_default() += 5.0;
    }
    let head: String = body.chars().take(1500).collect();
    for t in tokenize(&head) {
        *tf.entry(t).or_default() += 1.0;
    }
    tf
}

fn normalize(v: &mut HashMap<String, f32>) {
    let norm = v.values().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.values_mut().for_each(|x| *x /= norm);
    }
}

impl Index {
    pub fn build(docs: &[Doc]) -> Self {
        let tfs: Vec<_> = docs
            .iter()
            .map(|d| weighted_terms(&d.title, &d.body))
            .collect();
        let mut df: HashMap<&str, f32> = HashMap::new();
        for tf in &tfs {
            for t in tf.keys() {
                *df.entry(t).or_default() += 1.0;
            }
        }
        let n = docs.len() as f32;
        let idf: HashMap<String, f32> = df
            .into_iter()
            .map(|(t, d)| (t.to_owned(), ((n + 1.0) / (d + 1.0)).ln() + 1.0))
            .collect();
        let vecs = tfs
            .into_iter()
            .map(|tf| {
                let mut v: HashMap<String, f32> = tf
                    .into_iter()
                    // sub-linear term frequency, so a word repeated ten times is not ten times as important
                    .map(|(t, f)| {
                        let w = (1.0 + f.ln()) * idf.get(&t).copied().unwrap_or(1.0);
                        (t, w)
                    })
                    .collect();
                normalize(&mut v);
                v
            })
            .collect();
        Self { idf, vecs }
    }

    /// The `n` documents most like `title` + `body`, best first, never `skip` itself.
    pub fn similar(&self, title: &str, body: &str, skip: Option<usize>, n: usize) -> Vec<Hit> {
        let mut q: HashMap<String, f32> = weighted_terms(title, body)
            .into_iter()
            .filter_map(|(t, f)| {
                let idf = *self.idf.get(&t)?; // words no issue has say nothing about similarity
                Some((t, (1.0 + f.ln()) * idf))
            })
            .collect();
        normalize(&mut q);
        let mut hits: Vec<Hit> = self
            .vecs
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != skip)
            .filter_map(|(doc, v)| {
                let mut shared: Vec<(&String, f32)> = q
                    .iter()
                    .filter_map(|(t, w)| v.get(t).map(|x| (t, w * x)))
                    .collect();
                let score: f32 = shared.iter().map(|(_, s)| s).sum();
                if score <= 0.0 {
                    return None;
                }
                shared.sort_by(|a, b| b.1.total_cmp(&a.1));
                Some(Hit {
                    doc,
                    score,
                    shared: shared.into_iter().take(4).map(|(t, _)| t.clone()).collect(),
                })
            })
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(n);
        hits
    }
}

/// Above this the issues are worth a look as possible duplicates. Measured on 32,793 real Jira issues
/// with known duplicates (GitBugs): about one in three such answers is right, and the true
/// duplicate is among the best ten hits in two of three cases. A hint, not a verdict.
pub const DUPLICATE_SCORE: f32 = 0.5;
/// Below this a hit is noise.
pub const RELATED_SCORE: f32 = 0.18;

/// The words a text is most about, for a one-line "topics" hint.
pub fn keywords(text: &str, n: usize) -> Vec<String> {
    let mut freq: HashMap<String, usize> = HashMap::new();
    for t in tokenize(text) {
        *freq.entry(t).or_default() += 1;
    }
    let mut v: Vec<_> = freq.into_iter().filter(|(_, c)| *c >= 2).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|(t, _)| t).collect()
}

/// A weighted vote among similar issues: the value most of the weight sits on, with the share of
/// the weight it got, or `None` when the neighbours are too few or too divided to say.
pub fn vote(values: &[(String, f32)]) -> Option<(String, f32)> {
    let total: f32 = values.iter().map(|(_, w)| w).sum();
    if values.len() < 2 || total <= 0.0 {
        return None;
    }
    let mut by: HashMap<&str, f32> = HashMap::new();
    for (v, w) in values {
        *by.entry(v).or_default() += w;
    }
    let (best, w) = by
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(a.0)))?;
    let share = w / total;
    (share > 0.5).then(|| (best.to_owned(), share))
}

/// The median of numbers, each counting as much as its weight (the similarity of the issue it came
/// from), and how much of the weight lies within one step of it. `None` with fewer than two values.
pub fn weighted_median(values: &[(f64, f32)]) -> Option<(f64, f32)> {
    if values.len() < 2 {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: f32 = v.iter().map(|x| x.1).sum();
    if total <= 0.0 {
        return None;
    }
    let mut acc = 0.0;
    let mut median = v[v.len() - 1].0;
    for (value, w) in &v {
        acc += w;
        if acc >= total / 2.0 {
            median = *value;
            break;
        }
    }
    // "one step": the neighbouring values of the common scales (1, 2, 3, 5, 8, ...) are about 1.7x apart.
    let near: f32 = v
        .iter()
        .filter(|(x, _)| *x >= median / 1.7 - 1e-9 && *x <= median * 1.7 + 1e-9)
        .map(|x| x.1)
        .sum();
    Some((median, near / total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(key: &str, title: &str, body: &str) -> Doc {
        Doc {
            key: key.into(),
            title: title.into(),
            body: body.into(),
        }
    }

    fn corpus() -> Vec<Doc> {
        vec![
            doc("W-1", "Login fails with wrong password message", "After the update users cannot log in. The login form shows a wrong password message although the password is right."),
            doc("W-2", "Anmeldung schlägt fehl", "Nutzer können sich nicht anmelden, die Anmeldung zeigt ein falsches Passwort."),
            doc("W-3", "Export to CSV is slow", "Exporting more than 10000 rows to CSV takes minutes."),
            doc("W-4", "Dark mode colours on the board", "Cards on the board are unreadable in dark mode."),
            doc("W-5", "Login form shows wrong password for valid users", "Users report the login form rejects the correct password."),
        ]
    }

    #[test]
    fn tokens_drop_stopwords_and_endings() {
        assert_eq!(tokenize("The Logins are failing"), ["login", "fail"]);
        assert_eq!(
            tokenize("Die Anmeldungen und der Export"),
            ["anmeld", "export"]
        );
        assert!(tokenize("a an of 12").is_empty());
    }

    #[test]
    fn finds_the_duplicate_and_ranks_unrelated_issues_last() {
        let docs = corpus();
        let idx = Index::build(&docs);
        let hits = idx.similar(&docs[0].title, &docs[0].body, Some(0), 5);
        assert_eq!(docs[hits[0].doc].key, "W-5", "{hits:?}");
        assert!(hits[0].score >= DUPLICATE_SCORE, "{hits:?}");
        assert!(hits[0].shared.contains(&"login".to_owned()));
        assert!(
            hits.iter().all(|h| docs[h.doc].key != "W-1"),
            "never itself"
        );
        let unrelated = hits.iter().find(|h| docs[h.doc].key == "W-3");
        assert!(unrelated.is_none_or(|h| h.score < RELATED_SCORE));
    }

    #[test]
    fn unknown_words_match_nothing() {
        let idx = Index::build(&corpus());
        assert!(idx.similar("zzzqqq", "xxxyyy", None, 5).is_empty());
        assert!(Index::build(&[]).similar("login", "", None, 5).is_empty());
    }

    #[test]
    fn keywords_need_repetition() {
        let k = keywords("login login login form form password", 2);
        assert_eq!(k, ["login", "form"]);
    }

    #[test]
    fn votes_need_a_clear_majority() {
        let v = |x: &[(&str, f32)]| {
            x.iter()
                .map(|(a, b)| (a.to_string(), *b))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            vote(&v(&[("high", 0.9), ("high", 0.5), ("low", 0.3)]))
                .unwrap()
                .0,
            "high"
        );
        assert!(vote(&v(&[("high", 0.5), ("low", 0.5)])).is_none(), "split");
        assert!(
            vote(&v(&[("high", 0.9)])).is_none(),
            "one neighbour is not a pattern"
        );
        assert!(vote(&[]).is_none());
    }

    #[test]
    fn the_weighted_median_follows_the_weights() {
        let median = |v: &[(f64, f32)]| weighted_median(v).unwrap().0;
        assert_eq!(median(&[(3.0, 0.5), (5.0, 0.9), (8.0, 0.4)]), 5.0);
        // one value with half of all the weight is the median, whatever the rest says
        assert_eq!(median(&[(3.0, 0.9), (5.0, 0.5), (8.0, 0.4)]), 3.0);
        assert_eq!(median(&[(1.0, 0.1), (8.0, 0.9), (13.0, 0.2)]), 8.0);
        let (m, near) = weighted_median(&[(3.0, 1.0), (3.0, 1.0), (13.0, 1.0)]).unwrap();
        assert_eq!(m, 3.0);
        assert!((near - 2.0 / 3.0).abs() < 1e-6, "{near}");
        assert!(weighted_median(&[(3.0, 1.0)]).is_none());
        assert!(weighted_median(&[]).is_none());
    }
}
