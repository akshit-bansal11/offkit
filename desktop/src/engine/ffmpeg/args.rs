//! What a tool may tell FFmpeg. `Ffmpeg::run` takes a tool's output options as data and checks
//! them here, failing closed: anything not on these lists is refused before FFmpeg starts.
//!
//! - Output options must be on `OUTPUT_OPTIONS`, each followed by exactly the value FFmpeg
//!   reads for it. A bare word would be a second output, which `-y` would overwrite, so none is
//!   allowed; nor are `-/opt` (reads the value from a file), `-attach`, `-f` (`run` derives
//!   the muxer from the output's extension) or anything else that names a file.
//! - Filtergraphs (`-vf`, `-af`, `-filter`, `-filter_complex`) may only use `FILTERS`, none of
//!   which reads a file, and no filter option may be loaded from a file (`/opt=`) or be a file
//!   name (`filename=`, `textfile=`, …). Build them with `Filter`, `Chain` and `Graph`, which
//!   escape every value; `check_graph` parses the result the way FFmpeg does.

use std::ffi::OsString;
use std::fmt::Display;
use std::path::Path;

use anyhow::{Context, bail};

/// How FFmpeg reads the argument after an output option.
#[derive(Clone, Copy)]
enum Value {
    /// The option takes no value (`-an`).
    None,
    /// Any value that doesn't look like another option.
    Any,
    /// A filtergraph, checked by `check_graph`.
    Graph,
    /// A stream selector or a filtergraph link label (`0:a:0?`, `[out]`).
    Map,
    /// Only these exact values.
    OneOf(&'static [&'static str]),
}

/// Output options the tools use, by name before any `:stream` specifier (`-c:v` is `c`). Every
/// entry takes the value kind FFmpeg reads for it, so a value can never be parsed as an option.
const OUTPUT_OPTIONS: &[(&str, Value)] = &[
    // Streams
    ("map", Value::Map),
    ("an", Value::None),
    ("vn", Value::None),
    ("sn", Value::None),
    ("dn", Value::None),
    ("shortest", Value::None),
    // Codecs and their quality settings
    ("c", Value::Any),
    ("codec", Value::Any),
    ("b", Value::Any),
    ("maxrate", Value::Any),
    ("minrate", Value::Any),
    ("bufsize", Value::Any),
    ("q", Value::Any),
    ("crf", Value::Any),
    ("qp", Value::Any),
    ("preset", Value::Any),
    ("tune", Value::Any),
    ("profile", Value::Any),
    ("level", Value::Any),
    ("g", Value::Any),
    ("bf", Value::Any),
    ("compression_level", Value::Any),
    ("strict", Value::OneOf(&["-2", "experimental"])),
    // Encoder private options used by the Video Converter (libvpx, NVENC, Quick Sync, AMF)
    ("row-mt", Value::Any),
    ("deadline", Value::OneOf(&["good", "realtime", "best"])),
    ("cpu-used", Value::Any),
    ("rc", Value::Any),
    ("cq", Value::Any),
    ("global_quality", Value::Any),
    ("qp_i", Value::Any),
    ("qp_p", Value::Any),
    // libx265 options can name stats and analysis files, so only this exact value passes.
    ("x265-params", Value::OneOf(&["log-level=error"])),
    // Frames and samples
    ("pix_fmt", Value::Any),
    ("r", Value::Any),
    ("s", Value::Any),
    ("aspect", Value::Any),
    (
        "fps_mode",
        Value::OneOf(&["passthrough", "cfr", "vfr", "auto"]),
    ),
    ("frames", Value::Any),
    ("ar", Value::Any),
    ("ac", Value::Any),
    ("sample_fmt", Value::Any),
    // Time
    ("ss", Value::Any),
    ("t", Value::Any),
    ("to", Value::Any),
    // Filters
    ("vf", Value::Graph),
    ("af", Value::Graph),
    ("filter", Value::Graph),
    ("filter_complex", Value::Graph),
    // Container details
    ("metadata", Value::Any),
    ("map_metadata", Value::Any),
    ("map_chapters", Value::Any),
    ("movflags", Value::OneOf(&["+faststart"])),
    ("tag", Value::Any),
    ("loop", Value::Any),
    ("id3v2_version", Value::Any),
];

/// The only filters a filtergraph may use: none reads or writes a file, opens a device or runs
/// commands. Covers the Video Converter and the audio and video tools' website parity.
const FILTERS: &[&str] = &[
    // Audio
    "aformat",
    "afade",
    "anull",
    "aresample",
    "asetpts",
    "asetrate",
    "asplit",
    "atempo",
    "atrim",
    "bass",
    "equalizer",
    "showwavespic",
    "treble",
    "volume",
    // Video
    "crop",
    "fade",
    "format",
    "fps",
    "hflip",
    "null",
    "pad",
    "palettegen",
    "paletteuse",
    "scale",
    "setpts",
    "setsar",
    "split",
    "transpose",
    "trim",
    "vflip",
    // Both
    "concat",
];

/// The muxer for each output extension `run` writes. Single-file muxers only: `image2`,
/// `segment`, `tee`, `hls` and `dash` write many files (or others' files) and are left out.
pub(super) const MUXERS: &[(&str, &str)] = &[
    ("3gp", "3gp"),
    ("aac", "adts"),
    ("ac3", "ac3"),
    ("aif", "aiff"),
    ("aiff", "aiff"),
    ("avi", "avi"),
    ("flac", "flac"),
    ("flv", "flv"),
    ("gif", "gif"),
    ("m4a", "ipod"),
    ("mka", "matroska"),
    ("mkv", "matroska"),
    ("mov", "mov"),
    ("mp2", "mp2"),
    ("mp3", "mp3"),
    ("mp4", "mp4"),
    ("mpeg", "mpeg"),
    ("ogg", "ogg"),
    ("ogv", "ogg"),
    ("opus", "opus"),
    ("wav", "wav"),
    ("webm", "webm"),
    ("wma", "asf"),
];

/// `av_get_token`'s whitespace.
const WHITESPACE: &[char] = &[' ', '\n', '\t', '\r'];

/// The muxer `run` pins for `output`, from its extension.
pub(super) fn muxer_for(output: &Path) -> anyhow::Result<&'static str> {
    let ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    MUXERS
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, muxer)| *muxer)
        .with_context(|| format!("FFmpeg can't write “.{ext}” files here"))
}

/// Refuses any output option, value or filter that isn't allowlisted (see the module docs).
pub(super) fn check_output_args(args: &[OsString]) -> anyhow::Result<()> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let arg = arg.to_str().context("FFmpeg options must be text")?;
        let Some(name) = arg.strip_prefix('-') else {
            bail!("“{arg}” isn't an FFmpeg option");
        };
        let (base, spec) = name.split_once(':').unwrap_or((name, ""));
        if !spec.chars().all(|c| c.is_ascii_alphanumeric() || c == ':') {
            bail!("FFmpeg option “{arg}” has an unreadable stream specifier");
        }
        let Some((_, kind)) = OUTPUT_OPTIONS.iter().find(|(n, _)| *n == base) else {
            bail!("FFmpeg option “-{base}” isn't allowed");
        };
        if let Value::None = kind {
            continue;
        }
        let value = args
            .next()
            .with_context(|| format!("FFmpeg option “{arg}” needs a value"))?
            .to_str()
            .context("FFmpeg options must be text")?;
        if value.starts_with('-') && value.parse::<f64>().is_err() {
            bail!("“{value}” after “{arg}” looks like another option");
        }
        match kind {
            Value::Graph => check_graph(value)?,
            Value::Map => check_label(value.trim_start_matches('[').trim_end_matches(']'))?,
            Value::OneOf(allowed) if !allowed.contains(&value) => {
                bail!("“{value}” isn't allowed for “{arg}”")
            }
            _ => {}
        }
    }
    Ok(())
}

/// FFmpeg's `av_get_token`: reads up to the first unquoted, unescaped character of `term`,
/// resolving `\` escapes and `'…'` quoting, and trims the whitespace FFmpeg trims. Returns the
/// token and the rest, which starts at the terminator.
fn get_token<'a>(s: &'a str, term: &str) -> (String, &'a str) {
    let s = s.trim_start_matches(WHITESPACE);
    let mut out = String::new();
    // Trailing-whitespace trimming never cuts into escaped or quoted text.
    let mut keep = 0;
    let mut chars = s.char_indices().peekable();
    let mut rest = "";
    while let Some((i, c)) = chars.next() {
        if term.contains(c) {
            rest = &s[i..];
            break;
        }
        match c {
            '\\' if chars.peek().is_some() => {
                if let Some((_, escaped)) = chars.next() {
                    out.push(escaped);
                    keep = out.len();
                }
            }
            '\'' => {
                let mut closed = false;
                for (_, quoted) in chars.by_ref() {
                    if quoted == '\'' {
                        closed = true;
                        break;
                    }
                    out.push(quoted);
                }
                if closed {
                    keep = out.len();
                }
            }
            c => out.push(c),
        }
    }
    let trimmed = out.trim_end_matches(WHITESPACE).len().max(keep);
    out.truncate(trimmed);
    (out, rest)
}

/// Parses a filtergraph as libavfilter does and refuses any filter not on `FILTERS` and any
/// filter option loaded from or naming a file.
pub(super) fn check_graph(graph: &str) -> anyhow::Result<()> {
    let mut rest = graph;
    loop {
        rest = skip_labels(rest)?;
        let end = rest.find(['=', ',', ';', '[']).unwrap_or(rest.len());
        // The raw text, not the unquoted token, so a quoted or escaped name never matches.
        let name = rest[..end].trim_matches(WHITESPACE);
        if !FILTERS.contains(&name) {
            bail!("FFmpeg filter “{name}” isn't allowed");
        }
        rest = &rest[end..];
        if let Some(after) = rest.strip_prefix('=') {
            let (options, after) = get_token(after, "[],;");
            check_filter_options(name, &options)?;
            rest = after;
        }
        rest = skip_labels(rest)?.trim_start_matches(WHITESPACE);
        match rest.chars().next() {
            None => return Ok(()),
            Some(',' | ';') => rest = &rest[1..],
            Some(c) => bail!("Unreadable filtergraph near “{c}”"),
        }
    }
}

fn skip_labels(mut rest: &str) -> anyhow::Result<&str> {
    loop {
        rest = rest.trim_start_matches(WHITESPACE);
        let Some(after) = rest.strip_prefix('[') else {
            return Ok(rest);
        };
        let (label, after) = get_token(after, "]");
        check_label(&label)?;
        rest = after
            .strip_prefix(']')
            .context("A filtergraph link label isn't closed")?;
    }
}

fn check_label(label: &str) -> anyhow::Result<()> {
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '?');
    if label.is_empty() || !label.chars().all(plain) {
        bail!("“{label}” isn't a stream or link label");
    }
    Ok(())
}

/// One filter's `key=value:value:…` options, split as `av_opt_set_from_string` splits them.
fn check_filter_options(filter: &str, options: &str) -> anyhow::Result<()> {
    let mut rest = options;
    loop {
        let item = rest.trim_start_matches(WHITESPACE);
        let key_len = item
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.')))
            .unwrap_or(item.len());
        let after_key = item[key_len..].trim_start_matches(WHITESPACE);
        let value = match after_key.strip_prefix('=') {
            Some(value) if key_len > 0 => {
                let key = &item[..key_len];
                if key.contains('/') || key.to_ascii_lowercase().contains("file") {
                    bail!("FFmpeg filter option “{filter}={key}” isn't allowed");
                }
                value
            }
            _ => item,
        };
        let (_, after) = get_token(value, ":");
        match after.strip_prefix(':') {
            Some(next) => rest = next,
            None => return Ok(()),
        }
    }
}

/// One filter for a `Chain`, from `FILTERS`. Values are escaped for both levels of FFmpeg's
/// filtergraph syntax, so a value can hold `:`, `,` or quotes without ending the option.
pub struct Filter {
    text: String,
    first: bool,
}

impl Filter {
    pub fn new(name: &str) -> Self {
        Self {
            text: name.to_owned(),
            first: true,
        }
    }

    /// An option by position: `scale` then `.arg(1280).arg(-2)` is `scale=1280:-2`.
    pub fn arg(self, value: impl Display) -> Self {
        let value = quote_value(&value.to_string());
        self.push(&value)
    }

    /// A named option: `.opt("flags", "lanczos")`.
    pub fn opt(self, key: &str, value: impl Display) -> Self {
        let item = format!("{key}={}", quote_value(&value.to_string()));
        self.push(&item)
    }

    fn push(mut self, item: &str) -> Self {
        self.text.push(if self.first { '=' } else { ':' });
        self.first = false;
        // Second level: the graph parser must see the whole option string as one token.
        for c in item.chars() {
            if matches!(c, '\\' | '\'' | '[' | ']' | ',' | ';') {
                self.text.push('\\');
            }
            self.text.push(c);
        }
        self
    }
}

/// First level: quote an option value holding characters the option parser splits on.
fn quote_value(value: &str) -> String {
    let plain = !value.is_empty()
        && !value.contains(['\\', '\'', ':'])
        && value.trim_matches(WHITESPACE) == value;
    if plain {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

/// Filters in sequence, with optional input and output link labels: `[0:a][1:a]concat=…[out]`.
#[derive(Default)]
pub struct Chain {
    inputs: String,
    filters: Vec<String>,
    outputs: String,
}

impl Chain {
    pub fn new() -> Self {
        Self::default()
    }

    /// A link label or stream this chain reads, such as `0:v` or `a`.
    pub fn input(mut self, label: &str) -> Self {
        self.inputs.push_str(&format!("[{label}]"));
        self
    }

    pub fn then(mut self, filter: Filter) -> Self {
        self.filters.push(filter.text);
        self
    }

    /// A link label this chain's last filter writes, for a later chain or `-map [label]`.
    pub fn output(mut self, label: &str) -> Self {
        self.outputs.push_str(&format!("[{label}]"));
        self
    }
}

/// A filtergraph: one chain for `-vf`/`-af`, or several for `-filter_complex`.
#[derive(Default)]
pub struct Graph {
    chains: Vec<String>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chain(mut self, chain: Chain) -> Self {
        let filters = chain.filters.join(",");
        self.chains
            .push(format!("{}{filters}{}", chain.inputs, chain.outputs));
        self
    }

    /// The graph as the argument after `-vf`, `-af` or `-filter_complex`.
    pub fn arg(self) -> OsString {
        self.chains.join(";").into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    fn refused(words: &[&str]) -> String {
        check_output_args(&args(words)).unwrap_err().to_string()
    }

    // ---- The bypasses a review ran against the real FFmpeg; each must now be refused. ----

    #[test]
    fn a_file_reading_source_filter_is_refused() {
        assert_eq!(
            refused(&["-filter_complex", "amovie=secret.wav[out]", "-map", "[out]"]),
            "FFmpeg filter “amovie” isn't allowed"
        );
    }

    #[test]
    fn attaching_a_file_is_refused() {
        assert_eq!(
            refused(&["-attach", "secret.wav"]),
            "FFmpeg option “-attach” isn't allowed"
        );
    }

    #[test]
    fn an_option_read_from_a_file_is_refused() {
        assert_eq!(
            refused(&["-/filter_complex", "f.txt"]),
            "FFmpeg option “-/filter_complex” isn't allowed"
        );
    }

    #[test]
    fn a_bare_word_that_would_be_a_second_output_is_refused() {
        assert_eq!(
            refused(&["-c:a", "aac", "victim.mp3"]),
            "“victim.mp3” isn't an FFmpeg option"
        );
    }

    #[test]
    fn a_flag_cannot_swallow_the_option_after_it() {
        assert_eq!(
            refused(&["-an", "-attach", "secret.wav"]),
            "FFmpeg option “-attach” isn't allowed"
        );
    }

    #[test]
    fn a_value_that_looks_like_an_option_is_refused() {
        assert_eq!(
            refused(&["-c:a", "-attach"]),
            "“-attach” after “-c:a” looks like another option"
        );
    }

    #[test]
    fn choosing_the_muxer_is_refused() {
        assert_eq!(
            refused(&["-f", "image2"]),
            "FFmpeg option “-f” isn't allowed"
        );
    }

    #[test]
    fn x265_params_accept_only_the_log_level() {
        assert_eq!(
            refused(&["-x265-params", "csv=stats.csv"]),
            "“csv=stats.csv” isn't allowed for “-x265-params”"
        );
    }

    #[test]
    fn filters_that_read_files_or_run_commands_are_refused() {
        let names = [
            "movie=a.mp4",
            "subtitles=a.srt",
            "ass=a.ass",
            "sendcmd=f=a.txt",
            "asendcmd=f=a.txt",
            "zmq",
            "lut3d=a.cube",
            "haldclut",
            "frei0r=x",
            "ladspa=x",
            "lv2=x",
            "ocr",
            "drawtext=text=x",
        ];

        let refused: Vec<_> = names.iter().filter(|g| check_graph(g).is_err()).collect();

        assert_eq!(refused.len(), names.len());
    }

    #[test]
    fn a_quoted_or_escaped_filter_name_is_refused() {
        assert!(check_graph("'amovie'=secret.wav").is_err());
        assert!(check_graph("a\\movie=secret.wav").is_err());
    }

    #[test]
    fn a_file_hidden_after_a_quoted_comma_is_still_parsed_as_a_filter() {
        assert_eq!(
            check_graph("volume='1,2',amovie=secret.wav")
                .unwrap_err()
                .to_string(),
            "FFmpeg filter “amovie” isn't allowed"
        );
    }

    #[test]
    fn filter_options_loaded_from_or_naming_a_file_are_refused() {
        assert_eq!(
            check_graph("volume=/volume=secret.txt")
                .unwrap_err()
                .to_string(),
            "FFmpeg filter option “volume=/volume” isn't allowed"
        );
        assert!(check_graph("scale=w=1:filename=x").is_err());
    }

    #[test]
    fn a_graph_option_ending_in_a_stray_character_is_refused() {
        assert!(check_graph("scale=1:2 ]x").is_err());
    }

    // ---- What the tools need passes. ----

    #[test]
    fn the_video_converter_gif_graph_passes() {
        let gif = "fps=12,scale='min(640,iw)':-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse";

        assert!(check_graph(gif).is_ok());
    }

    #[test]
    fn audio_editor_and_merger_graphs_pass() {
        let editor = "asetrate=48000*1.059463,atempo=2.0,atempo=0.944,aresample=48000,bass=g=3:f=100,equalizer=f=1000:t=q:w=2:g=-2,treble=g=1:f=8000";
        let merger = "[0:v]scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,setsar=1[v0];[v0][0:a][1:v][1:a]concat=n=2:v=1:a=1[v][a]";

        assert!(check_graph(editor).is_ok());
        assert!(check_graph(merger).is_ok());
    }

    #[test]
    fn typical_output_options_pass() {
        let words = [
            "-map",
            "0:a:0?",
            "-vn",
            "-c:a",
            "libopus",
            "-b:a",
            "160k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-ss",
            "1.5",
            "-t",
            "3",
            "-strict",
            "-2",
            "-map_metadata",
            "-1",
            "-filter:a",
            "volume=0.5",
            "-metadata:s:a:0",
            "title=x",
        ];

        assert!(check_output_args(&args(&words)).is_ok());
    }

    // ---- The builder escapes what it's given, and its output reads back as one filter. ----

    #[test]
    fn builder_escapes_option_separators_in_values() {
        let graph = Graph::new()
            .chain(
                Chain::new().then(
                    Filter::new("scale")
                        .arg("min(640,iw)")
                        .arg(-1)
                        .opt("flags", "a:b"),
                ),
            )
            .arg();

        assert_eq!(graph, "scale=min(640\\,iw):-1:flags=\\'a:b\\'");
        assert!(check_graph(graph.to_str().unwrap()).is_ok());
    }

    #[test]
    fn builder_cannot_smuggle_a_second_filter_through_a_value() {
        let graph = Graph::new()
            .chain(Chain::new().then(Filter::new("volume").arg("1,amovie=secret.wav")))
            .arg();

        // One `volume` filter whose (invalid) value FFmpeg rejects; no `amovie` filter exists.
        assert!(check_graph(graph.to_str().unwrap()).is_ok());
    }

    #[test]
    fn builder_writes_labels_and_chains() {
        let graph = Graph::new()
            .chain(
                Chain::new()
                    .input("0:a")
                    .input("1:a")
                    .then(Filter::new("concat").opt("n", 2).opt("v", 0).opt("a", 1))
                    .output("out"),
            )
            .chain(Chain::new().input("out").then(Filter::new("anull")))
            .arg();

        assert_eq!(graph, "[0:a][1:a]concat=n=2:v=0:a=1[out];[out]anull");
    }

    #[test]
    fn get_token_matches_ffmpeg_quoting_and_escapes() {
        assert_eq!(get_token("  a\\,b 'c,d' ,e", ","), ("a,b c,d".into(), ",e"));
        assert_eq!(get_token("x\\ ", ","), ("x ".into(), ""));
    }

    #[test]
    fn outputs_get_a_single_file_muxer_from_their_extension() {
        assert_eq!(muxer_for(Path::new("song.M4A")).unwrap(), "ipod");
        assert_eq!(
            muxer_for(Path::new("frame.png")).unwrap_err().to_string(),
            "FFmpeg can't write “.png” files here"
        );
    }
}
