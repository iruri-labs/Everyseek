use std::{borrow::Cow, collections::BTreeSet};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

pub fn canonical(s: &str) -> Cow<'_, str> {
    if s.is_ascii() || is_nfc_quick(s.chars()) == IsNormalized::Yes {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.nfc().collect())
    }
}

pub fn normalize(s: &str, insensitive: bool) -> String {
    if insensitive {
        s.nfc().flat_map(char::to_lowercase).nfc().collect()
    } else {
        s.nfc().collect()
    }
}

fn unigram(c: char) -> String {
    format!("u{:06x}", c as u32)
}
fn bigram(a: char, b: char) -> String {
    format!("b{:06x}{:06x}", a as u32, b as u32)
}

// Encode scalar grams as ASCII words. FTS5 stores only compressed document IDs,
// without positions, frequencies or a second copy of the filename. Encoding
// keeps punctuation, spaces and user query operators literal.
pub fn tokens(name: &str) -> String {
    let chars: Vec<_> = normalize(name, true).chars().collect();
    let mut grams = BTreeSet::new();
    for &c in &chars {
        grams.insert(unigram(c));
    }
    for pair in chars.windows(2) {
        grams.insert(bigram(pair[0], pair[1]));
    }
    grams.into_iter().collect::<Vec<_>>().join(" ")
}

pub fn terms(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut term = String::new();
    let mut quoted = false;
    for c in query.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !term.is_empty() {
                    out.push(std::mem::take(&mut term));
                }
            }
            c => term.push(c),
        }
    }
    if !term.is_empty() {
        out.push(term);
    }
    out
}

pub fn candidate_query(terms: &[String]) -> Option<String> {
    let mut grams = BTreeSet::new();
    for term in terms {
        let folded = normalize(term, true);
        for literal in folded.split(['*', '?']).filter(|s| !s.is_empty()) {
            let chars: Vec<_> = literal.chars().collect();
            if chars.len() == 1 {
                grams.insert(unigram(chars[0]));
            }
            for pair in chars.windows(2) {
                grams.insert(bigram(pair[0], pair[1]));
            }
        }
    }
    if grams.is_empty() {
        None
    } else {
        Some(grams.into_iter().collect::<Vec<_>>().join(" AND "))
    }
}

pub fn matches(pattern: &str, text: &str, whole_word: bool) -> bool {
    let p: Vec<_> = pattern.chars().collect();
    let t: Vec<_> = text.chars().collect();
    if !p.contains(&'*') && !p.contains(&'?') {
        if p.is_empty() {
            return true;
        }
        return t.windows(p.len()).enumerate().any(|(i, s)| {
            s == p
                && (!whole_word
                    || ((i == 0 || !word(t[i - 1]))
                        && (i + p.len() == t.len() || !word(t[i + p.len()]))))
        });
    }
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            mark = ti;
        } else if let Some(s) = star {
            mark += 1;
            ti = mark;
            pi = s + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || unicode_normalization::char::is_combining_mark(c)
}
