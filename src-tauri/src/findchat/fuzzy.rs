//! fzf-style matching for names: every typed letter in order, scored higher for runs, word starts and the name's start.

const PER_LETTER: i32 = 16;
const AT_START: i32 = 12;
const AT_WORD: i32 = 8;
const IN_A_RUN: i32 = 6;

/// A name's match: its score and the matched letters' character positions, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fuzzy {
    pub score: u32,
    pub marks: Vec<u32>,
}

/// Every word of `words` (lower case) found in `name`, letters in order; `None` when any word isn't.
pub fn all_words(words: &[String], name: &str) -> Option<Fuzzy> {
    if words.is_empty() {
        return None;
    }
    let chars: Vec<char> = name.chars().collect();
    let mut score = 0;
    let mut marks = Vec::new();
    for word in words {
        let (one, at) = one_word(word, &chars)?;
        score += one;
        marks.extend(at);
    }
    marks.sort_unstable();
    marks.dedup();
    Some(Fuzzy { score, marks })
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The better of the first match found forward and that match narrowed backward, as fzf narrows it.
fn one_word(word: &str, name: &[char]) -> Option<(u32, Vec<u32>)> {
    let wanted: Vec<char> = word.chars().filter(|c| !c.is_whitespace()).collect();
    if wanted.is_empty() {
        return None;
    }
    let mut forward = Vec::with_capacity(wanted.len());
    for (at, &c) in name.iter().enumerate() {
        if forward.len() < wanted.len() && lower(c) == wanted[forward.len()] {
            forward.push(at);
        }
    }
    if forward.len() < wanted.len() {
        return None;
    }
    let end = *forward.last()?;
    let mut narrowed = Vec::with_capacity(wanted.len());
    for at in (0..=end).rev() {
        if narrowed.len() < wanted.len()
            && lower(name[at]) == wanted[wanted.len() - 1 - narrowed.len()]
        {
            narrowed.push(at);
        }
    }
    narrowed.reverse();
    let (a, b) = (scored(name, forward), scored(name, narrowed));
    Some(if b.0 > a.0 { b } else { a })
}

fn scored(name: &[char], marks: Vec<usize>) -> (u32, Vec<u32>) {
    let mut score = 0;
    for (index, &at) in marks.iter().enumerate() {
        score += PER_LETTER;
        if at == 0 {
            score += AT_START;
        } else if is_word_start(name, at) {
            score += AT_WORD;
        }
        if index > 0 && marks[index - 1] + 1 == at {
            score += IN_A_RUN;
        }
    }
    // Letters skipped inside the run cost one point each, so a tighter match wins.
    let span = marks
        .last()
        .zip(marks.first())
        .map_or(0, |(l, f)| l - f + 1);
    score -= i32::try_from(span - marks.len()).unwrap_or(i32::MAX);
    let score = u32::try_from(score.max(1)).unwrap_or(1);
    let marks = marks
        .into_iter()
        .map(|at| u32::try_from(at).unwrap_or(u32::MAX))
        .collect();
    (score, marks)
}

// After a separator, or an upper-case letter after a lower-case one, as in `camelCase`.
fn is_word_start(name: &[char], at: usize) -> bool {
    let (before, now) = (name[at - 1], name[at]);
    !before.is_alphanumeric() || (before.is_lowercase() && now.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_lowercase).collect()
    }

    #[test]
    fn letters_in_order_match_and_mark_where_they_landed() {
        let found = all_words(&words("cerpl"), "Cerberus Planner").unwrap();

        assert_eq!(found.marks, [0, 1, 2, 9, 10]);
    }

    #[test]
    fn letters_out_of_order_or_missing_do_not_match() {
        assert_eq!(all_words(&words("lpc"), "Cerberus Planner"), None);
        assert_eq!(all_words(&words("cerx"), "Cerberus Planner"), None);
        assert_eq!(all_words(&words(""), "Cerberus Planner"), None);
    }

    #[test]
    fn a_run_at_the_start_beats_letters_scattered_through_the_middle() {
        let start = all_words(&words("plan"), "Planner notes").unwrap().score;
        let word = all_words(&words("plan"), "the planner").unwrap().score;
        let scattered = all_words(&words("plan"), "spelling a nap").unwrap().score;

        assert!(start > word, "{start} vs {word}");
        assert!(word > scattered, "{word} vs {scattered}");
    }

    #[test]
    fn every_word_must_match_and_scores_add() {
        let both = all_words(&words("cer plan"), "Cerberus Planner").unwrap();
        let one = all_words(&words("cer"), "Cerberus Planner").unwrap();

        assert!(both.score > one.score);
        assert_eq!(all_words(&words("cer zebra"), "Cerberus Planner"), None);
    }

    #[test]
    fn a_camel_case_hump_counts_as_a_word_start() {
        let hump = all_words(&words("fc"), "FindChat").unwrap().score;
        let inside = all_words(&words("fc"), "Fiche").unwrap().score;

        assert!(hump > inside);
    }
}
