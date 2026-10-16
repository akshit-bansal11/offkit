//! Lorem Generator: placeholder text in words, sentences or paragraphs, from the website's two
//! word lists (classic Latin and English), optionally opening with "Lorem ipsum dolor sit amet".
//! The shapes match the website: sentences of 5 to 15 words, paragraphs of 4 to 8 sentences.
//! A small seeded generator (SplitMix64) draws the words, so the same seed gives the same text:
//! changing a setting keeps the seed, and Regenerate picks a new one. Generating is instant, so
//! it all runs on the UI thread. The view is ui/tools/lorem-generator.slint.

use std::cell::RefCell;
use std::hash::{BuildHasher, RandomState};
use std::path::PathBuf;
use std::thread::JoinHandle;

use slint::ComponentHandle;

use crate::engine::text;
use crate::{AppWindow, LoremGenerator, LoremStyle, LoremUnit, clipboard, files};

pub const SLUG: &str = "lorem-generator";

// ---- Logic --------------------------------------------------------------------------------

// The website's lists verbatim (src/app/(tools)/lorem-generator/page.tsx), repeats included:
// a repeated word is drawn more often.
const CLASSIC: [&str; 91] = [
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "ad",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "ut",
    "aliquip",
    "ex",
    "ea",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "dolor",
    "in",
    "reprehenderit",
    "in",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "dolore",
    "eu",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "non",
    "proident",
    "sunt",
    "in",
    "culpa",
    "qui",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "id",
    "est",
    "laborum",
    "sed",
    "ut",
    "perspiciatis",
    "unde",
    "omnis",
    "iste",
    "natus",
    "error",
    "sit",
    "voluptatem",
    "accusantium",
    "doloremque",
    "laudantium",
    "totam",
    "rem",
    "aperiam",
    "eaque",
    "ipsa",
    "quae",
    "ab",
    "illo",
    "inventore",
    "veritatis",
];

const ENGLISH: [&str; 90] = [
    "the",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "time",
    "flies",
    "like",
    "an",
    "arrow",
    "fruit",
    "banana",
    "apple",
    "sky",
    "blue",
    "ocean",
    "deep",
    "mountain",
    "high",
    "river",
    "flow",
    "wind",
    "blow",
    "sun",
    "shine",
    "moon",
    "glow",
    "star",
    "bright",
    "night",
    "dark",
    "day",
    "light",
    "morning",
    "dew",
    "evening",
    "mist",
    "tree",
    "green",
    "leaf",
    "fall",
    "winter",
    "cold",
    "summer",
    "hot",
    "spring",
    "warm",
    "autumn",
    "cool",
    "city",
    "busy",
    "street",
    "quiet",
    "home",
    "sweet",
    "family",
    "love",
    "friend",
    "true",
    "journey",
    "long",
    "adventure",
    "wild",
    "dream",
    "big",
    "goal",
    "reach",
    "mind",
    "clear",
    "heart",
    "pure",
    "soul",
    "free",
    "spirit",
    "brave",
    "courage",
    "strong",
    "hope",
    "always",
    "faith",
    "never",
    "give",
    "up",
    "keep",
    "going",
    "move",
    "forward",
];

const STARTER: [&str; 5] = ["Lorem", "ipsum", "dolor", "sit", "amet"];
const SENTENCE_WORDS: (usize, usize) = (5, 15);
const PARAGRAPH_SENTENCES: (usize, usize) = (4, 8);
/// The website's input maxima, enforced here: 500 words, or 100 sentences or paragraphs.
const MAX_WORDS: usize = 500;
const MAX_BLOCKS: usize = 100;

/// SplitMix64: tiny, fast, and good enough to pick words. Any seed works, zero included.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `low..=high`, without modulo bias.
    fn between(&mut self, (low, high): (usize, usize)) -> usize {
        let span = (high - low + 1) as u128;
        low + ((u128::from(self.next()) * span) >> 64) as usize
    }

    fn word<'a>(&mut self, words: &[&'a str]) -> &'a str {
        words[self.between((0, words.len() - 1))]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Words,
    Sentences,
    Paragraphs,
}

#[derive(Clone, Copy)]
struct Settings {
    unit: Unit,
    count: usize,
    english: bool,
    /// Only for the classic list, as on the website.
    start_with_lorem: bool,
}

fn max_count(unit: Unit) -> usize {
    match unit {
        Unit::Words => MAX_WORDS,
        Unit::Sentences | Unit::Paragraphs => MAX_BLOCKS,
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// One sentence of `length` words, capitalised, ending in a full stop. The starter counts toward
/// the length but is never cut, so a starter sentence has at least five words.
fn sentence(rng: &mut Rng, words: &[&str], length: usize, starter: bool) -> String {
    let mut picked: Vec<&str> = if starter {
        STARTER.to_vec()
    } else {
        Vec::new()
    };
    while picked.len() < length {
        picked.push(rng.word(words));
    }
    format!("{}.", capitalize(&picked.join(" ")))
}

fn generate(settings: Settings, seed: u64) -> String {
    let mut rng = Rng(seed);
    let words: &[&str] = if settings.english { &ENGLISH } else { &CLASSIC };
    let starter = settings.start_with_lorem && !settings.english;
    let count = settings.count.clamp(1, max_count(settings.unit));
    match settings.unit {
        Unit::Words => {
            let mut picked: Vec<&str> = if starter {
                STARTER.iter().copied().take(count).collect()
            } else {
                Vec::new()
            };
            while picked.len() < count {
                picked.push(rng.word(words));
            }
            capitalize(&picked.join(" "))
        }
        Unit::Sentences => (0..count)
            .map(|i| {
                let length = rng.between(SENTENCE_WORDS);
                sentence(&mut rng, words, length, starter && i == 0)
            })
            .collect::<Vec<_>>()
            .join(" "),
        Unit::Paragraphs => (0..count)
            .map(|p| {
                let sentences = rng.between(PARAGRAPH_SENTENCES);
                (0..sentences)
                    .map(|s| {
                        let length = rng.between(SENTENCE_WORDS);
                        sentence(&mut rng, words, length, starter && p == 0 && s == 0)
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

/// "3 paragraphs · 214 words".
fn summary(text: &str, unit: Unit, count: usize) -> String {
    let what = match unit {
        Unit::Words => String::new(),
        Unit::Sentences => format!("{} · ", text::counted(count, "sentence", "sentences")),
        Unit::Paragraphs => format!("{} · ", text::counted(count, "paragraph", "paragraphs")),
    };
    format!(
        "{what}{}",
        text::counted(text.split_whitespace().count(), "word", "words")
    )
}

fn new_seed() -> u64 {
    // Each RandomState is freshly keyed by the standard library: a different seed every time.
    RandomState::new().hash_one(())
}

// ---- State and wiring ---------------------------------------------------------------------

struct State {
    seed: u64,
    output: String,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { seed: new_seed(), output: String::new() });
}

/// Never call a native dialog inside `with`: its nested event loop can run posted closures,
/// which borrow the state again.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

pub fn init(app: &AppWindow) {
    let g = app.global::<LoremGenerator>();
    let weak = app.as_weak();
    g.on_changed(move || {
        if let Some(app) = weak.upgrade() {
            refresh(&app);
        }
    });
    let weak = app.as_weak();
    g.on_regenerate(move || {
        let Some(app) = weak.upgrade() else { return };
        with(|s| s.seed = new_seed());
        refresh(&app);
    });
    let weak = app.as_weak();
    g.on_copy_output(move || {
        let Some(app) = weak.upgrade() else {
            return false;
        };
        match with(|s| clipboard::copy_text(&s.output)) {
            Ok(()) => true,
            Err(e) => {
                app.global::<LoremGenerator>()
                    .set_error(format!("Couldn't copy: {e}").into());
                false
            }
        }
    });
    let weak = app.as_weak();
    g.on_save_output(move || {
        let Some(app) = weak.upgrade() else { return };
        let Some(path) = files::choose_save_path("Save Text", "lorem-ipsum.txt") else {
            return;
        };
        let g = app.global::<LoremGenerator>();
        match with(|s| text::save_text(&path, &s.output)) {
            Ok(note) => g.set_note(note.into()),
            Err(error) => g.set_error(error.into()),
        }
    });
    refresh(app);
}

/// Takes no files, so a drag shows no highlight.
pub fn set_drop_hover(_app: &AppWindow, _hover: bool) {}

/// Takes no files, so a drop is ignored.
pub fn add_paths(_app: &AppWindow, _paths: Vec<PathBuf>) {}

/// Nothing runs in the background.
pub fn shutdown() -> Vec<JoinHandle<()>> {
    Vec::new()
}

fn refresh(app: &AppWindow) {
    let g = app.global::<LoremGenerator>();
    let unit = match g.get_unit() {
        LoremUnit::Words => Unit::Words,
        LoremUnit::Sentences => Unit::Sentences,
        LoremUnit::Paragraphs => Unit::Paragraphs,
    };
    let max = max_count(unit);
    // The count field clamps typed values to the old maximum; switching to a unit with a
    // smaller one brings it down here.
    let count = (g.get_count().round().max(1.0) as usize).min(max);
    g.set_max_count(max as f32);
    g.set_count(count as f32);
    let settings = Settings {
        unit,
        count,
        english: g.get_style() == LoremStyle::English,
        start_with_lorem: g.get_start_with_lorem(),
    };
    with(|s| {
        s.output = generate(settings, s.seed);
        g.set_output(s.output.as_str().into());
        g.set_summary(summary(&s.output, unit, count).into());
    });
    g.set_error("".into());
    g.set_note("".into());
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSIC_PARAGRAPHS: Settings = Settings {
        unit: Unit::Paragraphs,
        count: 3,
        english: false,
        start_with_lorem: true,
    };

    #[test]
    fn the_same_seed_gives_the_same_text() {
        assert_eq!(
            generate(CLASSIC_PARAGRAPHS, 42),
            generate(CLASSIC_PARAGRAPHS, 42)
        );
    }

    #[test]
    fn a_new_seed_gives_new_text() {
        assert_ne!(
            generate(CLASSIC_PARAGRAPHS, 1),
            generate(CLASSIC_PARAGRAPHS, 2)
        );
    }

    #[test]
    fn paragraphs_are_separated_by_a_blank_line() {
        let text = generate(CLASSIC_PARAGRAPHS, 7);

        assert_eq!(text.split("\n\n").count(), 3);
    }

    #[test]
    fn paragraphs_hold_four_to_eight_sentences() {
        let settings = Settings {
            count: 100,
            ..CLASSIC_PARAGRAPHS
        };

        let sizes: Vec<usize> = generate(settings, 9)
            .split("\n\n")
            .map(|p| p.matches('.').count())
            .collect();

        assert_eq!(
            (sizes.iter().min(), sizes.iter().max()),
            (Some(&4), Some(&8))
        );
    }

    #[test]
    fn sentences_hold_five_to_fifteen_words() {
        let settings = Settings {
            unit: Unit::Sentences,
            count: 100,
            ..CLASSIC_PARAGRAPHS
        };

        let lengths: Vec<usize> = generate(settings, 3)
            .split_terminator(". ")
            .map(|s| s.split_whitespace().count())
            .collect();

        assert_eq!(lengths.len(), 100);
        assert_eq!(
            (lengths.iter().min(), lengths.iter().max()),
            (Some(&5), Some(&15))
        );
    }

    #[test]
    fn words_come_in_the_exact_count_without_a_full_stop() {
        let settings = Settings {
            unit: Unit::Words,
            count: 12,
            ..CLASSIC_PARAGRAPHS
        };

        let text = generate(settings, 5);

        assert_eq!(text.split_whitespace().count(), 12);
        assert!(!text.ends_with('.'));
    }

    #[test]
    fn classic_text_opens_with_lorem_ipsum() {
        assert!(generate(CLASSIC_PARAGRAPHS, 11).starts_with("Lorem ipsum dolor sit amet "));
    }

    #[test]
    fn fewer_than_five_words_take_the_start_of_lorem_ipsum() {
        let settings = Settings {
            unit: Unit::Words,
            count: 3,
            ..CLASSIC_PARAGRAPHS
        };

        assert_eq!(generate(settings, 0), "Lorem ipsum dolor");
    }

    #[test]
    fn english_text_never_opens_with_lorem_ipsum() {
        let settings = Settings {
            english: true,
            ..CLASSIC_PARAGRAPHS
        };

        assert!(!generate(settings, 11).starts_with("Lorem"));
    }

    #[test]
    fn english_text_uses_only_english_words() {
        let settings = Settings {
            unit: Unit::Words,
            count: 200,
            english: true,
            start_with_lorem: false,
        };

        let foreign: Vec<String> = generate(settings, 13)
            .split_whitespace()
            .map(str::to_lowercase)
            .filter(|w| !ENGLISH.contains(&w.as_str()))
            .collect();

        assert_eq!(foreign, Vec::<String>::new());
    }

    #[test]
    fn counts_above_the_maximum_are_capped() {
        let settings = Settings {
            unit: Unit::Words,
            count: 10_000,
            ..CLASSIC_PARAGRAPHS
        };

        assert_eq!(generate(settings, 1).split_whitespace().count(), MAX_WORDS);
    }

    #[test]
    fn the_summary_counts_paragraphs_and_words() {
        assert_eq!(
            summary("Ab cd.\n\nEf.", Unit::Paragraphs, 2),
            "2 paragraphs · 3 words"
        );
    }

    #[test]
    fn rng_ranges_are_inclusive_and_bounded() {
        let mut rng = Rng(0);

        let draws: Vec<usize> = (0..1000).map(|_| rng.between((5, 15))).collect();

        assert_eq!(
            (draws.iter().min(), draws.iter().max()),
            (Some(&5), Some(&15))
        );
    }
}
