//! English sentence splitting tuned for academic prose.

const ABBREVIATIONS: &[&str] = &[
    "al", "app", "appx", "approx", "cf", "cor", "def", "dr", "eq", "eqs", "etc", "fig", "figs",
    "lem", "mr", "ms", "no", "nos", "pp", "prof", "prop", "ref", "refs", "resp", "sec", "secs",
    "sect", "st", "tab", "thm", "viz", "vol", "vs",
];

fn starts_sentence(c: char) -> bool {
    c.is_uppercase() || c.is_ascii_digit() || matches!(c, '(' | '[' | '"' | '“' | '‘' | '⟦')
}

fn ends_with_abbreviation(before: &[char]) -> bool {
    let word: String = before
        .iter()
        .rev()
        .take_while(|c| !c.is_whitespace() && !matches!(c, '(' | '['))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let word = word
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if word.is_empty() {
        return false;
    }
    // "e.g", "i.e", "U.S" keep an inner period. A number ("3.2", "(§4.2)") is not an
    // abbreviation: "see §3.2. The" ends a sentence.
    (word.contains('.') && word.chars().all(|c| c.is_alphabetic() || c == '.'))
        || ABBREVIATIONS.contains(&word.as_str())
}

pub fn split(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out: Vec<String> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if !matches!(c, '.' | '?' | '!') {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < n && matches!(chars[end], ')' | ']' | '"' | '\'' | '”' | '’') {
            end += 1;
        }
        if end < n && chars[end].is_whitespace() {
            let mut next = end;
            while next < n && chars[next].is_whitespace() {
                next += 1;
            }
            let abbreviation = c == '.' && ends_with_abbreviation(&chars[start..i]);
            if next < n && starts_sentence(chars[next]) && !abbreviation {
                out.push(
                    chars[start..end]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string(),
                );
                start = next;
                i = next;
                continue;
            }
        }
        i = end;
    }
    let tail: String = chars[start..].iter().collect::<String>().trim().to_string();
    if !tail.is_empty() {
        out.push(tail);
    }
    // Fold fragments too short to stand alone into the previous sentence.
    let mut merged: Vec<String> = Vec::new();
    for s in out {
        match merged.last_mut() {
            Some(prev) if s.chars().filter(|c| c.is_alphanumeric()).count() < 3 => {
                prev.push(' ');
                prev.push_str(&s);
            }
            _ => merged.push(s),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn splits_plain_sentences() {
        assert_eq!(
            split("We propose a model. It is fast! Is it good? Yes."),
            vec!["We propose a model.", "It is fast!", "Is it good?", "Yes."]
        );
    }

    #[test]
    fn keeps_abbreviations_and_decimals() {
        let s = "As shown in Fig. 3 and Eq. (2), training took 3.5 days, e.g. On GPUs. Smith et al. (2020) agree.";
        assert_eq!(
            split(s),
            vec![
                "As shown in Fig. 3 and Eq. (2), training took 3.5 days, e.g. On GPUs.",
                "Smith et al. (2020) agree."
            ]
        );
    }

    #[test]
    fn splits_after_math_and_citations() {
        assert_eq!(
            split("The cost is ⟦M1⟧. Prior work [5, 2] differs."),
            vec!["The cost is ⟦M1⟧.", "Prior work [5, 2] differs."]
        );
    }

    #[test]
    fn a_section_number_ends_a_sentence() {
        assert_eq!(
            split("Gains shrink (§4.2). For details, see §3.1. The mean is 12.5. Done."),
            vec![
                "Gains shrink (§4.2).",
                "For details, see §3.1.",
                "The mean is 12.5.",
                "Done."
            ]
        );
        assert_eq!(
            split("Use e.g. Adam. It works."),
            vec!["Use e.g. Adam.", "It works."]
        );
    }

    #[test]
    fn does_not_split_before_lowercase() {
        assert_eq!(
            split("See sec. two. it continues"),
            vec!["See sec. two. it continues"]
        );
    }

    #[test]
    fn closing_parenthesis_stays_with_sentence() {
        assert_eq!(
            split("This holds (see below.) Then it ends."),
            vec!["This holds (see below.)", "Then it ends."]
        );
    }
}
