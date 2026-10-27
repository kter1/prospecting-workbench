//! The one text matcher shared by evidence extraction and scoring.
//!
//! Before this existed the crawler and the scorer each did their own string
//! search, and both were literal: "hiring servers" missed "we're hiring
//! experienced servers", "owner" matched "homeowners", and "we are not hiring"
//! counted as a hiring signal. A single deterministic matcher keeps the two
//! sides in agreement: the crawler only stores an excerpt that the scorer will
//! accept, and the scorer never credits a line the crawler would have skipped.
//!
//! Matching works on whole tokens with light suffix stemming, so plurals and
//! verb forms line up. A phrase of several words may have up to four other
//! words between its parts, in order. A match is rejected when a negation sits
//! in the three tokens before it.

/// Words that reverse the meaning of what follows them.
const NEGATIONS: &[&str] = &[
    "not", "no", "never", "without", "none", "nor", "cannot", "isn", "aren", "don", "doesn",
    "didn", "won", "wasn", "weren", "unable",
];

/// How many unrelated tokens a multi-word phrase may skip over in total.
const PHRASE_SLACK: usize = 4;

/// How far back a negation can reach.
const NEGATION_REACH: usize = 3;

fn is_vowel(character: u8) -> bool {
    matches!(character, b'a' | b'e' | b'i' | b'o' | b'u')
}

/// Reduce a lowercase token to a comparison stem.
///
/// Deliberately crude and symmetric: it is applied to the page and to the
/// keyword alike, so it only has to map related forms to the same stem, not
/// produce real words. "scheduling", "scheduled" and "schedule" all become
/// "schedul"; "services" and "service" become "servic".
pub fn stem(token: &str) -> String {
    let mut word = token.to_string();
    if word.len() < 4 || word.bytes().all(|byte| byte.is_ascii_digit()) {
        return word;
    }
    if word.ends_with("ies") && word.len() > 4 {
        word.truncate(word.len() - 3);
        word.push('y');
    } else if word.ends_with("sses") {
        word.truncate(word.len() - 2);
    } else if word.ends_with('s')
        && !word.ends_with("ss")
        && !word.ends_with("us")
        && !word.ends_with("is")
    {
        word.truncate(word.len() - 1);
    }
    if word.ends_with("ing") && word.len() >= 6 {
        word.truncate(word.len() - 3);
    } else if word.ends_with("ed") && word.len() >= 5 {
        word.truncate(word.len() - 2);
    }
    if word.ends_with('e') && word.len() > 3 {
        word.truncate(word.len() - 1);
    }
    let bytes = word.as_bytes();
    if bytes.len() > 3 {
        let last = bytes[bytes.len() - 1];
        if last == bytes[bytes.len() - 2] && !is_vowel(last) && last.is_ascii_alphabetic() {
            word.truncate(word.len() - 1);
        }
    }
    word
}

fn raw_tokens(value: &str) -> Vec<String> {
    value
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Text prepared once for many phrase lookups.
#[derive(Debug, Clone)]
pub struct Text {
    raw: Vec<String>,
    stems: Vec<String>,
}

impl Text {
    pub fn new(value: &str) -> Self {
        let raw = raw_tokens(value);
        let stems = raw.iter().map(|token| stem(token)).collect();
        Self { raw, stems }
    }

    /// Token span `[start, end)` of every place `phrase` occurs, ignoring
    /// negation.
    ///
    /// A phrase in double quotes is compared on exact tokens, for the few
    /// cases where the plural is the meaning: "locations" says there is more
    /// than one, "location" does not.
    fn occurrences(&self, phrase: &str) -> Vec<(usize, usize)> {
        let trimmed = phrase.trim();
        let exact = trimmed.len() > 2 && trimmed.starts_with('"') && trimmed.ends_with('"');
        let wanted = if exact {
            raw_tokens(trimmed)
        } else {
            raw_tokens(trimmed)
                .iter()
                .map(|token| stem(token))
                .collect()
        };
        let haystack = if exact { &self.raw } else { &self.stems };
        let Some(first) = wanted.first() else {
            return Vec::new();
        };
        let limit = wanted.len() + if wanted.len() > 1 { PHRASE_SLACK } else { 0 };
        let mut found = Vec::new();
        for start in 0..haystack.len() {
            if &haystack[start] != first {
                continue;
            }
            let window_end = (start + limit).min(haystack.len());
            let mut position = start + 1;
            let mut matched = 1;
            while matched < wanted.len() && position < window_end {
                if haystack[position] == wanted[matched] {
                    matched += 1;
                }
                position += 1;
            }
            if matched == wanted.len() {
                found.push((start, position.max(start + 1)));
            }
        }
        found
    }

    /// A negation shortly before the phrase, or between its words: "we are
    /// not hiring" must not match "we are hiring".
    fn negated(&self, (start, end): (usize, usize)) -> bool {
        self.raw[start.saturating_sub(NEGATION_REACH)..end]
            .iter()
            .any(|token| NEGATIONS.contains(&token.as_str()))
    }

    /// The phrase occurs anywhere, negated or not. For identity questions such
    /// as segments, personas and exclusions, where "we do not serve X" still
    /// names X.
    pub fn mentions(&self, phrase: &str) -> bool {
        !self.occurrences(phrase).is_empty()
    }

    /// The phrase occurs at least once without a negation just before it. For
    /// signals, where "we are not hiring" must not count as hiring.
    pub fn affirms(&self, phrase: &str) -> bool {
        self.occurrences(phrase)
            .into_iter()
            .any(|span| !self.negated(span))
    }
}

/// Normalized lowercase alphanumeric text, used to compare excerpts.
pub fn normalized(value: &str) -> String {
    raw_tokens(value).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_forms_share_a_stem() {
        for (left, right) in [
            ("scheduling", "schedule"),
            ("scheduled", "schedule"),
            ("services", "service"),
            ("appointments", "appointment"),
            ("hiring", "hire"),
            ("openings", "opening"),
            ("companies", "company"),
            ("owners", "owner"),
            ("staffing", "staff"),
            ("addresses", "address"),
        ] {
            assert_eq!(stem(left), stem(right), "{left} vs {right}");
        }
        assert_ne!(stem("homeowners"), stem("owner"));
        assert_eq!(stem("24"), "24");
    }

    #[test]
    fn phrases_tolerate_a_few_words_in_between_but_keep_order() {
        let text = Text::new("We're hiring experienced servers and line cooks.");
        assert!(text.affirms("hiring servers"));
        assert!(text.affirms("line cook"));
        assert!(!text.affirms("servers hiring"));
        let far = Text::new("hiring is hard for us and our many patient long-serving servers");
        assert!(!far.affirms("hiring servers"));
    }

    #[test]
    fn whole_tokens_only() {
        let text = Text::new("Trusted by homeowners for avocado toast and law firms.");
        assert!(!text.mentions("owner"));
        assert!(text.mentions("law firm"));
        assert!(!text.mentions("toast pos"));
    }

    #[test]
    fn quoted_phrases_compare_exact_tokens() {
        let one = Text::new("We have one location on Cesar Chavez.");
        let three = Text::new("Fresh tacos at three Austin locations.");
        assert!(one.mentions("locations"));
        assert!(!one.mentions("\"locations\""));
        assert!(three.mentions("\"locations\""));
    }

    #[test]
    fn negation_blocks_affirmation_but_not_mention() {
        let text = Text::new("We are not hiring at this time. We do not offer emergency service.");
        assert!(!text.affirms("hiring"));
        assert!(!text.affirms("emergency service"));
        assert!(text.mentions("emergency service"));
        assert!(!Text::new("We are not hiring right now.").affirms("we are hiring"));
        let later = Text::new("No parking on site. Now hiring cooks.");
        assert!(later.affirms("hiring"));
    }
}
