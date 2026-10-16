//! Dev family (archive-extractor, archiver): ZIP, TAR, TAR.GZ, GZ and 7z. `extract` unpacks one
//! archive into a fresh folder (a lone `.gz` into a fresh file); `plan` and `write` pack files and
//! folders into one archive. Owner: dev family.
//!
//! Extraction treats every archive as hostile and fails closed: it refuses the whole archive, and
//! the reserved folder is deleted with everything already written into it, on
//! - a name that could land outside the folder (zip-slip): `..`, an absolute path, a drive letter
//!   or UNC prefix, or a backslash, which Windows tools read as a separator;
//! - a name Windows reserves (`CON`, `NUL`, `COM1`, …), a `:` (it writes an NTFS alternate data
//!   stream, or names a drive), or another character a file name can't hold;
//! - more entries or more unpacked bytes than its `Limits` (zip and decompression bombs);
//! - a file whose name was already written: files are created with `create_new` inside a folder
//!   the job created itself, so nothing is ever overwritten.
//!
//! Links and device files are never created; they are skipped and listed in the result.

use std::cell::{Cell, RefCell};
use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::UNIX_EPOCH;

use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use sevenz_rust2::encoder_options::Lzma2Options;
use sevenz_rust2::{
    ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderConfiguration, EncoderMethod, Password,
};
use tar::{EntryType, Header};
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::files::{self, Reserved, ReservedDir, file_name, format_size};
use crate::jobs::{CancelToken, JobError};

/// The most entries one archive may hold, extracting or creating.
pub const MAX_ENTRIES: usize = 100_000;
/// Unpacked bytes allowed per archive byte. Text compresses about 10:1 and very repetitive logs
/// past 100:1; DEFLATE tops out near 1,000:1, which only crafted bombs reach.
const MAX_RATIO: u64 = 200;
/// The byte cap never drops below this, so small archives of very compressible data still open…
const MIN_UNPACKED: u64 = 1_000_000_000;
/// …and never rises above this, however big the archive.
const MAX_UNPACKED: u64 = 50_000_000_000;
/// Folder levels an archive may be created from. The walk recurses, so an absurdly deep folder
/// must not exhaust the stack; Directory Scaffold caps its walk at the same depth.
const MAX_DEPTH: usize = 256;
/// TAR long-name and PAX headers are read into memory whole, so they are capped. Real ones hold
/// a path and a few attributes.
const MAX_TAR_HEADER: u64 = 1 << 20;
/// 7-Zip stores a Unix mode in the high 16 bits of the attributes when this bit is set.
const UNIX_EXTENSION: u32 = 0x8000;
/// A Windows symbolic link or junction.
const REPARSE_POINT: u32 = 0x400;
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;

// ---- Formats -------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    /// One gzip-compressed file that isn't a tar.
    Gz,
    SevenZ,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::Gz => "GZIP",
            Self::SevenZ => "7z",
        }
    }

    /// Reads the format from the file's first bytes; its name is never trusted. A gzip stream is
    /// a TAR.GZ when what it holds starts like a tar archive.
    pub fn detect(path: &Path) -> io::Result<Option<Self>> {
        let head = first_block(File::open(path)?)?;
        Ok(
            if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
                Some(Self::Zip)
            } else if head.starts_with(b"7z\xBC\xAF\x27\x1C") {
                Some(Self::SevenZ)
            } else if head.starts_with(b"\x1F\x8B") {
                let inner = first_block(MultiGzDecoder::new(File::open(path)?))?;
                Some(if is_tar(&inner) {
                    Self::TarGz
                } else {
                    Self::Gz
                })
            } else if is_tar(&head) {
                Some(Self::Tar)
            } else {
                None
            },
        )
    }
}

/// A tar header block: 512 bytes. Enough for every signature above.
fn first_block(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut block = Vec::new();
    reader.take(512).read_to_end(&mut block)?;
    Ok(block)
}

/// POSIX and GNU tar headers carry "ustar" at offset 257.
fn is_tar(block: &[u8]) -> bool {
    block.get(257..262) == Some(b"ustar")
}

/// What a quick look at an archive tells before extracting it.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub format: Format,
    /// How many entries it lists, for the formats that keep a directory (ZIP, 7z). Reading a
    /// TAR's list means decompressing all of it, so that waits for the extraction.
    pub entries: Option<usize>,
}

impl Probe {
    /// "ZIP · 12 items".
    pub fn summary(&self) -> String {
        match self.entries {
            Some(n) => format!("{} · {}", self.format.label(), plural(n, "item")),
            None => self.format.label().into(),
        }
    }
}

/// Identifies an archive and counts its entries where that's cheap. The error is the sentence
/// its row shows.
pub fn probe(path: &Path) -> Result<Probe, String> {
    let bad = |e: &dyn Display| format!("Couldn't read the archive: {e}");
    let format = Format::detect(path)
        .map_err(|e| bad(&e))?
        .ok_or("Not a ZIP, TAR, GZ or 7z archive.")?;
    let entries = match format {
        Format::Zip => {
            let file = File::open(path).map_err(|e| bad(&e))?;
            Some(ZipArchive::new(file).map_err(|e| bad(&e))?.len())
        }
        Format::SevenZ => {
            let mut file = File::open(path).map_err(|e| bad(&e))?;
            check_7z_header(&mut file, MAX_7Z_DICTIONARY)?;
            match sevenz_rust2::Archive::read(&mut file, &Password::empty()) {
                Ok(archive) => Some(archive.files.len()),
                Err(e) if is_locked_7z(&e) => return Err(LOCKED.into()),
                Err(e) => return Err(bad(&e)),
            }
        }
        Format::Tar | Format::TarGz | Format::Gz => None,
    };
    Ok(Probe { format, entries })
}

// ---- Extracting ----------------------------------------------------------------------------

/// What one extraction may produce before it's stopped as a bomb.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub entries: usize,
    pub bytes: u64,
}

impl Limits {
    /// The caps for an archive of `len` bytes: `MAX_RATIO` times its size, within
    /// `MIN_UNPACKED..=MAX_UNPACKED`.
    pub fn for_archive(len: u64) -> Self {
        Self {
            entries: MAX_ENTRIES,
            bytes: len
                .saturating_mul(MAX_RATIO)
                .clamp(MIN_UNPACKED, MAX_UNPACKED),
        }
    }

    /// The byte cap in words, for the inspector: "200 times the archive, up to 50 GB".
    pub fn describe() -> String {
        format!(
            "{MAX_RATIO} times the archive, up to {} GB",
            MAX_UNPACKED / 1_000_000_000
        )
    }
}

/// Where an extraction went, still reserved: dropping it deletes the output.
pub enum Output {
    Folder(ReservedDir),
    File(Reserved),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Skip {
    Link,
    Special,
}

/// A finished extraction. `keep` makes it permanent; dropping it deletes what was written.
pub struct Extracted {
    output: Output,
    files: usize,
    skipped: Vec<(Skip, String)>,
}

impl Extracted {
    /// Keeps the output and returns where it is, with the line its row shows:
    /// "Extracted 12 files to photos · skipped 1 link: “docs/latest”".
    pub fn keep(self) -> io::Result<(PathBuf, String)> {
        let (path, done) = match self.output {
            Output::Folder(folder) => {
                let path = folder.keep();
                let done = format!(
                    "Extracted {} to {}",
                    plural(self.files, "file"),
                    file_name(&path)
                );
                (path, done)
            }
            Output::File(file) => {
                let path = file.keep()?;
                let done = format!("Saved as {}", file_name(&path));
                (path, done)
            }
        };
        Ok((path, format!("{done}{}", skipped_note(&self.skipped))))
    }
}

/// " · skipped 2 links and 1 special file: “a”, “b”, “c”", naming the first three.
fn skipped_note(skipped: &[(Skip, String)]) -> String {
    const NAMED: usize = 3;
    if skipped.is_empty() {
        return String::new();
    }
    let links = skipped
        .iter()
        .filter(|(kind, _)| *kind == Skip::Link)
        .count();
    let special = skipped.len() - links;
    let what = match (links, special) {
        (n, 0) => plural(n, "link"),
        (0, n) => plural(n, "special file"),
        (l, s) => format!("{} and {}", plural(l, "link"), plural(s, "special file")),
    };
    let names: Vec<_> = skipped
        .iter()
        .take(NAMED)
        .map(|(_, name)| format!("“{}”", echoed(name)))
        .collect();
    let more = match skipped.len().saturating_sub(NAMED) {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    format!(" · skipped {what}: {}{more}", names.join(", "))
}

/// Unpacks `archive` into a new folder named after it, beside it or in `out_dir` (a `.gz` into a
/// new file named after it), polling `cancel` and reporting progress (0..1) through the archive.
pub fn extract(
    archive: &Path,
    format: Format,
    out_dir: Option<&Path>,
    limits: Limits,
    cancel: &CancelToken,
    progress: &dyn Fn(f32),
) -> Result<Extracted, JobError> {
    let file =
        File::open(archive).map_err(|e| failed(format!("Couldn't open the archive: {e}")))?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let read = Rc::new(Cell::new(0));
    let source = Counted {
        inner: file,
        read: read.clone(),
        cancel,
    };
    let mut sink = Sink {
        root: PathBuf::new(),
        limits,
        cancel,
        meter: Meter {
            read,
            len,
            report: progress,
            shown: 0.0,
        },
        entries: 0,
        unpacked: 0,
        files: 0,
        skipped: Vec::new(),
        buf: vec![0; 64 * 1024],
    };
    let output = match format {
        Format::Gz => gunzip(archive, out_dir, source, &mut sink),
        Format::Zip => into_folder(archive, out_dir, &mut sink, |s| unzip(source, s)),
        Format::Tar => into_folder(archive, out_dir, &mut sink, |s| untar(source, s)),
        Format::TarGz => into_folder(archive, out_dir, &mut sink, |s| {
            untar(MultiGzDecoder::new(source), s)
        }),
        Format::SevenZ => into_folder(archive, out_dir, &mut sink, |s| un7z(source, s)),
    };
    // Cancelling makes the archive's reader fail, which each format reports as its own error.
    let output = output.map_err(|e| {
        if cancel.is_cancelled() {
            JobError::Cancelled
        } else {
            e
        }
    })?;
    Ok(Extracted {
        output,
        files: sink.files,
        skipped: sink.skipped,
    })
}

fn into_folder(
    archive: &Path,
    out_dir: Option<&Path>,
    sink: &mut Sink,
    unpack: impl FnOnce(&mut Sink) -> Result<(), JobError>,
) -> Result<Output, JobError> {
    // "photos.tar.gz" unpacks to "photos", not "photos.tar".
    let named = match archive.file_stem().map(Path::new) {
        Some(stem)
            if stem
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("tar")) =>
        {
            archive.with_extension("")
        }
        _ => archive.to_path_buf(),
    };
    let folder = files::reserve_dir(&named, out_dir, "")
        .map_err(|e| failed(format!("Couldn't create the folder: {e}")))?;
    sink.root = folder.path().to_path_buf();
    unpack(sink)?;
    Ok(Output::Folder(folder))
}

/// A lone `.gz` holds one file, which lands beside the archive: "notes.txt.gz" → "notes.txt".
fn gunzip(
    archive: &Path,
    out_dir: Option<&Path>,
    source: impl Read,
    sink: &mut Sink,
) -> Result<Output, JobError> {
    let inner = archive.with_extension("");
    let ext = inner
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let create = |e: io::Error| failed(format!("Couldn't create the output file: {e}"));
    let output = files::reserve_output(&inner, out_dir, &ext).map_err(create)?;
    let mut file = OpenOptions::new()
        .write(true)
        .open(output.path())
        .map_err(create)?;
    sink.copy(
        &file_name(&inner),
        &mut MultiGzDecoder::new(source),
        &mut file,
    )?;
    sink.files = 1;
    Ok(Output::File(output))
}

fn unzip(source: impl Read + Seek, sink: &mut Sink) -> Result<(), JobError> {
    let mut zip = ZipArchive::new(source).map_err(zip_error)?;
    sink.expect(zip.len())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(zip_error)?;
        let name = entry.name().map_err(zip_error)?.into_owned();
        let rel = sink.entry(&name)?;
        // The zip crate's own guard, on top of ours: either one refusing is enough.
        if entry.enclosed_name().is_none() {
            return Err(refused(&name, "it points outside the folder"));
        }
        if entry.is_symlink() {
            sink.skip(Skip::Link, name);
        } else if entry.is_dir() {
            sink.dir(&rel, &name, &mut entry)?;
        } else {
            sink.file(&rel, &name, &mut entry)?;
        }
    }
    Ok(())
}

fn untar(source: impl Read, sink: &mut Sink) -> Result<(), JobError> {
    let mut archive = tar::Archive::new(source);
    // Raw entries: in its default mode the tar crate reads long-name and PAX headers into memory
    // whole, so one claiming gigabytes (a few megabytes inside a .tar.gz) would exhaust it. Here
    // they're capped, then applied to the entry they describe.
    let entries = archive.entries().map_err(unreadable)?.raw(true);
    let (mut long_name, mut pax_name) = (None, None);
    for entry in entries {
        let mut entry = entry.map_err(unreadable)?;
        let kind = entry.header().entry_type();
        if kind.is_gnu_longname()
            || kind.is_gnu_longlink()
            || kind.is_pax_local_extensions()
            || kind.is_pax_global_extensions()
        {
            sink.count()?;
            let data = sink.header(&mut entry)?;
            if kind.is_gnu_longname() {
                long_name = Some(data);
            } else if kind.is_pax_local_extensions() {
                pax_name = pax_path(&data)?;
            }
            continue;
        }
        let (pax, long) = (pax_name.take(), long_name.take());
        let name = tar_name(
            pax.or(long)
                .unwrap_or_else(|| entry.path_bytes().into_owned()),
        )?;
        let rel = sink.entry(&name)?;
        match kind {
            // Pre-POSIX tars mark folders as files whose names end in "/".
            EntryType::Regular if name.ends_with('/') => sink.dir(&rel, &name, &mut entry)?,
            EntryType::Regular | EntryType::Continuous => sink.file(&rel, &name, &mut entry)?,
            EntryType::Directory => sink.dir(&rel, &name, &mut entry)?,
            EntryType::GNUSparse => return Err(unsupported("sparse files")),
            EntryType::Symlink | EntryType::Link => {
                sink.drain(&name, &mut entry)?;
                sink.skip(Skip::Link, name);
            }
            _ => {
                sink.drain(&name, &mut entry)?;
                sink.skip(Skip::Special, name);
            }
        }
    }
    Ok(())
}

/// The `path` a PAX header gives the next entry. Refuses what raw iteration can't honour: a
/// `size` (the header's own size would misplace every later entry) and GNU sparse maps.
fn pax_path(data: &[u8]) -> Result<Option<Vec<u8>>, JobError> {
    let mut path = None;
    for extension in tar::PaxExtensions::new(data) {
        let extension = extension.map_err(unreadable)?;
        match extension.key_bytes() {
            b"path" => path = Some(extension.value_bytes().to_vec()),
            b"size" => return Err(unsupported("a PAX size field (files over 8 GB)")),
            key if key.starts_with(b"GNU.sparse.") => return Err(unsupported("sparse files")),
            _ => {}
        }
    }
    Ok(path)
}

/// A tar name as text: GNU long names end in NUL; anything not UTF-8 is refused.
fn tar_name(mut raw: Vec<u8>) -> Result<String, JobError> {
    while raw.last() == Some(&0) {
        raw.pop();
    }
    String::from_utf8(raw).map_err(|e| {
        refused(
            &String::from_utf8_lossy(e.as_bytes()),
            "its name isn't valid UTF-8",
        )
    })
}

fn un7z(mut source: impl Read + Seek, sink: &mut Sink) -> Result<(), JobError> {
    check_7z_header(&mut source, MAX_7Z_DICTIONARY).map_err(failed)?;
    let mut archive = ArchiveReader::new(source, Password::empty()).map_err(seven_error)?;
    sink.expect(archive.archive().files.len())?;
    check_7z_methods(archive.archive(), MAX_7Z_DICTIONARY, sink.limits.bytes)?;
    let mut failure = None;
    let result = archive.for_each_entries(|entry, data| {
        // Stopping ends one block only; later blocks still call in.
        if failure.is_some() {
            return Ok(false);
        }
        match unpack_7z_entry(entry, data, sink) {
            Ok(()) => Ok(true),
            Err(e) => {
                failure = Some(e);
                Ok(false)
            }
        }
    });
    match failure {
        Some(e) => Err(e),
        None => result.map_err(seven_error),
    }
}

/// Decoding allocates what a coder's properties declare, before any data is read, so an archive
/// can ask for gigabytes with a few bytes of header. Nothing this app or ordinary tools write
/// needs more than this.
const MAX_7Z_DICTIONARY: u64 = 256 << 20;

/// Refuses a 7z whose coders declare a window or model bigger than `max_dictionary`, or whose
/// blocks declare more unpacked bytes than `max_bytes`, before anything is decoded.
fn check_7z_methods(
    archive: &sevenz_rust2::Archive,
    max_dictionary: u64,
    max_bytes: u64,
) -> Result<(), JobError> {
    let mut declared = 0u64;
    for block in &archive.blocks {
        declared = declared.saturating_add(block.get_unpack_size());
        for coder in &block.coders {
            let wanted = declared_memory(coder.encoder_method_id(), coder.properties());
            if wanted.is_some_and(|bytes| bytes > max_dictionary) {
                return Err(failed(too_much_memory(max_dictionary)));
            }
        }
    }
    if declared > max_bytes {
        return Err(failed(format!(
            "Stopped: the archive unpacks to more than {}, so it may be a decompression bomb. \
             Nothing was extracted.",
            format_size(max_bytes)
        )));
    }
    Ok(())
}

fn too_much_memory(max_dictionary: u64) -> String {
    format!(
        "The archive asks for more than {} of memory to unpack, so it may be a decompression \
         bomb. Nothing was extracted.",
        format_size(max_dictionary)
    )
}

/// A 7z header packed with LZMA is decoded when the archive is opened, before `check_7z_methods`
/// can see any coder. Its packing is described in plain bytes just ahead: the start header
/// points at the "next header", and an encoded one begins with the streams info naming the
/// coders. This reads those bytes alone and refuses a header coder wanting more than
/// `max_dictionary`, or a header unpacking past `MAX_7Z_HEADER`, before anything is decoded.
/// Anything it can't place (not a 7z, a plain header, a truncated file) is left for the 7z reader
/// to accept or report. A 7z whose start header doesn't point at a real next header is refused
/// here, since the reader would otherwise scan for one itself. Leaves `source` at the start.
fn check_7z_header(source: &mut (impl Read + Seek), max_dictionary: u64) -> Result<(), String> {
    let found = encoded_header_streams(source);
    // Whatever reads next starts from the top.
    source
        .seek(SeekFrom::Start(0))
        .map_err(|e| format!("Couldn't read the archive: {e}"))?;
    let Some(streams) = found? else {
        return Ok(());
    };
    let (dictionary, unpacked) =
        scan_encoded_header(&streams).ok_or_else(|| DAMAGED_7Z.to_string())?;
    if dictionary > max_dictionary {
        return Err(too_much_memory(max_dictionary));
    }
    if unpacked > MAX_7Z_HEADER {
        return Err(format!(
            "The archive's header unpacks to more than {}, so it may be a decompression bomb. \
             Nothing was extracted.",
            format_size(MAX_7Z_HEADER)
        ));
    }
    Ok(())
}

const DAMAGED_7Z: &str = "This 7z archive's header is damaged. Nothing was extracted.";

/// The streams info that opens an encoded 7z header: the start of the "next header" when its
/// first byte says it's encoded; `None` for a plain one or a file that isn't a 7z. A 7z whose
/// start header points nowhere real (a zero or out-of-range offset or size) is refused outright:
/// the 7z reader would hunt backwards through the file for a header of its own to decode. At
/// most `MAX_7Z_STREAMS_INFO` bytes are read, as a real one is a few dozen.
fn encoded_header_streams(source: &mut (impl Read + Seek)) -> Result<Option<Vec<u8>>, String> {
    const SIGNATURE_HEADER: u64 = 32;
    const ENCODED_HEADER: u8 = 0x17;
    let io = |e: io::Error| format!("Couldn't read the archive: {e}");
    let damaged = || DAMAGED_7Z.to_string();
    let len = source.seek(SeekFrom::End(0)).map_err(io)?;
    source.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut start = Vec::new();
    source
        .take(SIGNATURE_HEADER)
        .read_to_end(&mut start)
        .map_err(io)?;
    if !start.starts_with(b"7z\xBC\xAF\x27\x1C") {
        return Ok(None);
    }
    let field = |from: usize| -> Option<u64> {
        Some(u64::from_le_bytes(
            start.get(from..from + 8)?.try_into().ok()?,
        ))
    };
    let (offset, size) = field(12).zip(field(20)).ok_or_else(damaged)?;
    let at = SIGNATURE_HEADER.checked_add(offset).ok_or_else(damaged)?;
    let end = at.checked_add(size).ok_or_else(damaged)?;
    if size == 0 || end > len {
        return Err(damaged());
    }
    source.seek(SeekFrom::Start(at)).map_err(io)?;
    let mut streams = Vec::new();
    source
        .take(size.min(MAX_7Z_STREAMS_INFO))
        .read_to_end(&mut streams)
        .map_err(io)?;
    Ok((streams.first() == Some(&ENCODED_HEADER)).then(|| streams.split_off(1)))
}

/// The most of an encoded header's streams info that is read.
const MAX_7Z_STREAMS_INFO: u64 = 64 * 1024;
/// The most a 7z header may unpack to. A real one lists names and times, tens of bytes per entry,
/// so this holds far more than `MAX_ENTRIES`.
const MAX_7Z_HEADER: u64 = 64 << 20;

/// Walks the streams info of an encoded header, as sevenz-rust2 reads it (pack info, then the
/// folders' coders) and returns the largest dictionary a coder declares and the largest unpacked
/// size. `None` for anything that isn't well-formed or that this doesn't read.
fn scan_encoded_header(streams: &[u8]) -> Option<(u64, u64)> {
    const PACK_INFO: u8 = 0x06;
    const UNPACK_INFO: u8 = 0x07;
    const SIZE: u8 = 0x09;
    const CRC: u8 = 0x0A;
    const FOLDER: u8 = 0x0B;
    const CODERS_UNPACK_SIZE: u8 = 0x0C;
    const END: u8 = 0x00;
    let mut bytes = Bytes(streams);
    let bound = streams.len() as u64;
    let mut next = bytes.u8()?;
    if next == PACK_INFO {
        bytes.number()?; // where the packed data starts
        let packed = bytes.count()?;
        loop {
            match bytes.u8()? {
                SIZE => {
                    for _ in 0..packed {
                        bytes.number()?;
                    }
                }
                CRC => {
                    let defined = if bytes.u8()? != 0 {
                        packed
                    } else {
                        let bits = bytes.take(packed.div_ceil(8))?;
                        bits.iter().map(|b| b.count_ones() as usize).sum()
                    };
                    bytes.take(defined.checked_mul(4)?)?;
                }
                END => break,
                _ => return None,
            }
        }
        next = bytes.u8()?;
    }
    if next != UNPACK_INFO || bytes.u8()? != FOLDER {
        return None;
    }
    let folders = bytes.count()?;
    if bytes.u8()? != 0 {
        return None; // The folders live in another stream.
    }
    let (mut dictionary, mut unpacked, mut outputs) = (0, 0, 0usize);
    for _ in 0..folders {
        let (mut ins, mut outs) = (0u64, 0u64);
        for _ in 0..bytes.count()? {
            let flags = bytes.u8()?;
            if flags & 0x80 != 0 {
                return None; // Alternative methods, which 7-Zip itself doesn't read.
            }
            let method = bytes.take(usize::from(flags & 0x0F))?;
            let (coder_ins, coder_outs) = match flags & 0x10 {
                0 => (1, 1),
                _ => (bytes.number()?, bytes.number()?),
            };
            ins = ins.checked_add(coder_ins).filter(|n| *n <= bound)?;
            outs = outs.checked_add(coder_outs).filter(|n| *n <= bound)?;
            if flags & 0x20 != 0 {
                let size = bytes.count()?;
                let properties = bytes.take(size)?;
                let wanted = declared_memory(method, properties).unwrap_or(0);
                dictionary = dictionary.max(wanted);
            }
        }
        let pairs = outs.checked_sub(1)?;
        for _ in 0..pairs * 2 {
            bytes.number()?;
        }
        let loose = ins.checked_sub(pairs)?;
        if loose > 1 {
            for _ in 0..loose {
                bytes.number()?;
            }
        }
        outputs = outputs.checked_add(usize::try_from(outs).ok()?)?;
    }
    if bytes.u8()? != CODERS_UNPACK_SIZE {
        return None;
    }
    for _ in 0..outputs {
        unpacked = unpacked.max(bytes.number()?);
    }
    Some((dictionary, unpacked))
}

/// A cursor over header bytes; every read is `None` past the end.
struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(head)
    }

    /// A 7z number: the leading 1-bits of the first byte say how many bytes follow it.
    fn number(&mut self) -> Option<u64> {
        let first = u64::from(self.u8()?);
        let mut value = 0;
        for i in 0..8 {
            let mask = 0x80 >> i;
            if first & mask == 0 {
                return Some(value | ((first & (mask - 1)) << (8 * i)));
            }
            value |= u64::from(self.u8()?) << (8 * i);
        }
        Some(value)
    }

    /// A number counting things that each take at least a byte after it, so more than are left
    /// is damage, not a big header.
    fn count(&mut self) -> Option<usize> {
        usize::try_from(self.number()?)
            .ok()
            .filter(|n| *n <= self.0.len())
    }
}

/// The window or model size a coder's properties declare, for the methods that allocate by it:
/// LZMA (dictionary, 4 bytes after the first), LZMA2 (one byte of size class) and PPMd (model
/// memory, 4 bytes after the order).
fn declared_memory(method: &[u8], properties: &[u8]) -> Option<u64> {
    let word = |at: usize| {
        let bytes = properties.get(at..at + 4)?;
        Some(u64::from(u32::from_le_bytes(bytes.try_into().ok()?)))
    };
    match method {
        [0x03, 0x01, 0x01] | [0x03, 0x04, 0x01] => word(1),
        [0x21] => {
            let class = u64::from(*properties.first()?);
            Some(match class {
                0..=39 => (2 | (class & 1)) << (class / 2 + 11),
                _ => u64::from(u32::MAX),
            })
        }
        _ => None,
    }
}

fn unpack_7z_entry(
    entry: &ArchiveEntry,
    data: &mut dyn Read,
    sink: &mut Sink,
) -> Result<(), JobError> {
    // An anti-item records a deletion in an updated archive; it has no file to write.
    if entry.is_anti_item() {
        return Ok(());
    }
    let name = entry.name();
    let rel = sink.entry(name)?;
    let unix = (entry.has_windows_attributes && entry.windows_attributes() & UNIX_EXTENSION != 0)
        .then(|| (entry.windows_attributes() >> 16) & S_IFMT);
    let reparse = entry.has_windows_attributes && entry.windows_attributes() & REPARSE_POINT != 0;
    let skip = match unix {
        _ if reparse => Some(Skip::Link),
        Some(S_IFLNK) => Some(Skip::Link),
        Some(0 | S_IFREG | S_IFDIR) | None => None,
        Some(_) => Some(Skip::Special),
    };
    match skip {
        // A solid block streams entries back to back: a skipped one is still read past.
        Some(kind) => {
            sink.drain(name, data)?;
            sink.skip(kind, name.into());
        }
        None if entry.is_directory() => sink.dir(&rel, name, data)?,
        None => sink.file(&rel, name, data)?,
    }
    Ok(())
}

/// Reads the archive file for an extraction: counts bytes for progress, and fails once the job
/// is cancelled, which also stops the tar crate skipping through a huge entry.
struct Counted<'a> {
    inner: File,
    read: Rc<Cell<u64>>,
    cancel: &'a CancelToken,
}

impl Read for Counted<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::other("cancelled"));
        }
        let n = self.inner.read(buf)?;
        self.read.set(self.read.get() + n as u64);
        Ok(n)
    }
}

impl Seek for Counted<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Progress through the archive file, reported in steps of at least 1% so a big archive doesn't
/// flood the UI thread.
struct Meter<'a> {
    read: Rc<Cell<u64>>,
    len: u64,
    report: &'a dyn Fn(f32),
    shown: f32,
}

impl Meter<'_> {
    fn tick(&mut self) {
        let done = (self.read.get() as f64 / self.len.max(1) as f64).min(1.0) as f32;
        if done >= self.shown + 0.01 {
            self.shown = done;
            (self.report)(done);
        }
    }
}

/// Where entries are written, and every guard on the way.
struct Sink<'a> {
    root: PathBuf,
    limits: Limits,
    cancel: &'a CancelToken,
    meter: Meter<'a>,
    entries: usize,
    unpacked: u64,
    files: usize,
    skipped: Vec<(Skip, String)>,
    buf: Vec<u8>,
}

impl Sink<'_> {
    /// Refuses at once an archive whose directory lists more entries than allowed.
    fn expect(&self, entries: usize) -> Result<(), JobError> {
        if entries > self.limits.entries {
            return Err(too_many(self.limits.entries));
        }
        Ok(())
    }

    /// Counts one header against the entry cap and polls for cancel. Every header counts, a TAR
    /// extension header included, so a run of those can't dodge the cap.
    fn count(&mut self) -> Result<(), JobError> {
        self.entries += 1;
        if self.entries > self.limits.entries {
            return Err(too_many(self.limits.entries));
        }
        if self.cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        self.meter.tick();
        Ok(())
    }

    /// Counts one entry and checks its name: the path it stands for inside the folder.
    fn entry(&mut self, name: &str) -> Result<PathBuf, JobError> {
        self.count()?;
        relative_path(name).map_err(|reason| refused(name, reason))
    }

    /// Makes a folder. A folder entry has no data, but a crafted one may claim some: it's read
    /// past against the byte cap like any other entry's.
    fn dir(&mut self, rel: &Path, name: &str, data: &mut dyn Read) -> Result<(), JobError> {
        self.drain(name, data)?;
        self.clear(rel, name)?;
        fs::create_dir_all(self.root.join(rel)).map_err(|e| unwritable(name, e))
    }

    /// Refuses a path with a link or junction already standing on it below the folder. Nothing
    /// here creates one, so this only catches something else swapping one in mid-extraction.
    fn clear(&self, rel: &Path, name: &str) -> Result<(), JobError> {
        let mut at = self.root.clone();
        for part in rel.components() {
            at.push(part);
            match fs::symlink_metadata(&at) {
                Ok(meta) if meta.file_type().is_symlink() || is_reparse_point(&meta) => {
                    return Err(refused(name, "a link appeared inside the folder"));
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => break,
                Err(e) => return Err(unwritable(name, e)),
            }
        }
        Ok(())
    }

    fn file(&mut self, rel: &Path, name: &str, data: &mut dyn Read) -> Result<(), JobError> {
        if rel.as_os_str().is_empty() {
            return Err(refused(name, "it has no file name"));
        }
        self.clear(rel, name)?;
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| unwritable(name, e))?;
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| match e.kind() {
                io::ErrorKind::AlreadyExists => failed(format!(
                    "The archive holds “{}” twice, or two names that differ only in case. \
                     Nothing was extracted.",
                    echoed(name)
                )),
                _ => unwritable(name, e),
            })?;
        self.copy(name, data, &mut out)?;
        self.files += 1;
        Ok(())
    }

    /// Reads past an entry that isn't written, against the same byte cap.
    fn drain(&mut self, name: &str, data: &mut dyn Read) -> Result<(), JobError> {
        self.copy(name, data, &mut io::sink())
    }

    /// Reads a TAR extension header, refusing one too big to be real.
    fn header(&mut self, entry: &mut tar::Entry<'_, impl Read>) -> Result<Vec<u8>, JobError> {
        if entry.size() > MAX_TAR_HEADER {
            return Err(failed(
                "Stopped: the archive has an oversized TAR header, so it may be a \
                 decompression bomb. Nothing was extracted."
                    .into(),
            ));
        }
        let mut data = Vec::new();
        self.copy("a TAR header", entry, &mut data)?;
        Ok(data)
    }

    fn skip(&mut self, kind: Skip, name: String) {
        self.skipped.push((kind, name));
    }

    /// Copies an entry's bytes, polling for cancel and stopping at the byte cap: sizes in
    /// headers are never trusted, only what decompression actually produces.
    fn copy(
        &mut self,
        name: &str,
        from: &mut dyn Read,
        to: &mut dyn Write,
    ) -> Result<(), JobError> {
        loop {
            if self.cancel.is_cancelled() {
                return Err(JobError::Cancelled);
            }
            let n = match from.read(&mut self.buf) {
                Ok(0) => return Ok(()),
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    return Err(failed(format!(
                        "Couldn't read “{}” from the archive: {e}",
                        echoed(name)
                    )));
                }
            };
            self.unpacked += n as u64;
            if self.unpacked > self.limits.bytes {
                return Err(failed(format!(
                    "Stopped: the archive unpacks to more than {}, so it may be a \
                     decompression bomb. Nothing was extracted.",
                    format_size(self.limits.bytes)
                )));
            }
            to.write_all(&self.buf[..n])
                .map_err(|e| unwritable(name, e))?;
            self.meter.tick();
        }
    }
}

/// The path an entry name stands for inside the destination folder, or why it can't be written
/// there. Archives store `/` as the separator; empty and `.` parts are dropped.
fn relative_path(name: &str) -> Result<PathBuf, &'static str> {
    if name.contains('\\') {
        return Err("it uses backslashes, which Windows reads as folder separators");
    }
    if name.starts_with('/') {
        return Err("it's an absolute path, outside the folder");
    }
    let mut path = PathBuf::new();
    for part in name.split('/').filter(|p| !p.is_empty() && *p != ".") {
        check_part(part)?;
        path.push(part);
    }
    Ok(path)
}

/// One folder or file name, checked against what Windows would misread. Checked on every
/// platform, so an archive extracts the same everywhere.
fn check_part(part: &str) -> Result<(), &'static str> {
    if part == ".." {
        return Err("it points outside the folder with “..”");
    }
    if part.contains(':') {
        return Err("“:” names a drive or a hidden data stream on Windows");
    }
    if part
        .chars()
        .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err("it has characters a file name can't hold");
    }
    if part.ends_with(['.', ' ']) {
        return Err("Windows drops a trailing dot or space, so it could stand for another file");
    }
    // Windows reserves these device names with any extension: "CON", "nul.txt", "COM1.tar.gz".
    let stem = part
        .split('.')
        .next()
        .unwrap_or(part)
        .trim_end()
        .to_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            n.chars().count() == 1 && n.chars().all(|c| c.is_ascii_digit() || "¹²³".contains(c))
        })
    };
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || numbered("COM")
        || numbered("LPT")
    {
        return Err("Windows reserves that name for a device");
    }
    Ok(())
}

/// A Windows junction or symbolic link, as the attribute says; false elsewhere, where
/// `is_symlink` already sees links.
#[cfg(windows)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}

fn zip_error(e: ZipError) -> JobError {
    match e {
        ZipError::UnsupportedArchive(reason) if reason == ZipError::PASSWORD_REQUIRED => {
            failed(LOCKED.into())
        }
        e => unreadable(e),
    }
}

fn seven_error(e: sevenz_rust2::Error) -> JobError {
    if is_locked_7z(&e) {
        failed(LOCKED.into())
    } else {
        unreadable(e)
    }
}

fn is_locked_7z(e: &sevenz_rust2::Error) -> bool {
    matches!(
        e,
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_)
    )
}

const LOCKED: &str = "This archive is password-protected, which isn't supported.";

fn unreadable(e: impl Display) -> JobError {
    failed(format!("Couldn't read the archive: {e}"))
}

fn unwritable(name: &str, e: io::Error) -> JobError {
    failed(format!("Couldn't write “{}”: {e}", echoed(name)))
}

fn unsupported(what: &str) -> JobError {
    failed(format!(
        "The archive uses {what}, which isn't supported. Nothing was extracted."
    ))
}

fn refused(name: &str, reason: &str) -> JobError {
    failed(format!(
        "Refused “{}”: {reason}. Nothing was extracted.",
        echoed(name)
    ))
}

fn too_many(limit: usize) -> JobError {
    failed(format!(
        "Stopped: the archive holds more than {} items, so it may be a decompression bomb. \
         Nothing was extracted.",
        thousands(limit)
    ))
}

fn failed(message: String) -> JobError {
    JobError::Failed(message)
}

/// A name as one line of text: control characters (a newline in a crafted name) become “�”.
pub fn shown(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

/// The most characters of an archive-supplied name that an error or note repeats.
const MAX_ECHOED: usize = 200;

/// `shown`, cut to `MAX_ECHOED` characters with “…”: a crafted name can be megabytes, and a
/// message repeating it whole would swamp the row that shows it.
pub fn echoed(name: &str) -> String {
    let line = shown(&name.chars().take(MAX_ECHOED + 1).collect::<String>());
    match line.char_indices().nth(MAX_ECHOED) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line,
    }
}

/// "1 file", "12 files", "1,200 files".
pub fn plural(n: usize, one: &str) -> String {
    match n {
        1 => format!("1 {one}"),
        n => format!("{} {one}s", thousands(n)),
    }
}

/// 100000 → "100,000".
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ---- Creating ------------------------------------------------------------------------------

/// The formats Archiver writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Zip,
    TarGz,
    SevenZ,
}

impl Target {
    pub const ALL: [Self; 3] = [Self::Zip, Self::TarGz, Self::SevenZ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::TarGz => "TAR.GZ",
            Self::SevenZ => "7z",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
            Self::SevenZ => "7z",
        }
    }
}

/// One file or folder to store, in the order it's written.
struct Item {
    path: PathBuf,
    /// Its path inside the archive: the root's name, then `/`-separated names below it.
    name: String,
    dir: bool,
    size: u64,
    mode: u32,
    mtime: u64,
    /// Which of the chosen roots it came from, for that root's progress.
    root: usize,
}

/// Everything archiving some roots would store, walked without following links.
#[derive(Default)]
pub struct Plan {
    items: Vec<Item>,
    root_bytes: Vec<u64>,
    /// Links (and anything else neither a file nor a folder), by archive name.
    pub skipped: Vec<String>,
    pub files: usize,
    pub folders: usize,
    pub bytes: u64,
}

/// Walks `roots` (files and folders) into a `Plan`. Each root is stored under its own name and
/// keeps its structure below; links, Windows junctions included, are listed in `skipped` and
/// never followed. `exclude` (the archive being written, when it's inside a root) is left out.
pub fn plan(
    roots: &[PathBuf],
    exclude: Option<&Path>,
    max_entries: usize,
    cancel: &CancelToken,
) -> Result<Plan, JobError> {
    let mut plan = Plan::default();
    for (root, path) in roots.iter().enumerate() {
        plan.root_bytes.push(0);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| not_unicode(&path.to_string_lossy()))?;
        plan.add(root, path, name.to_string(), exclude, max_entries, cancel)?;
    }
    Ok(plan)
}

impl Plan {
    fn add(
        &mut self,
        root: usize,
        path: &Path,
        name: String,
        exclude: Option<&Path>,
        max_entries: usize,
        cancel: &CancelToken,
    ) -> Result<(), JobError> {
        if cancel.is_cancelled() {
            return Err(JobError::Cancelled);
        }
        // `name` is the root's name, then one `/` per level below it. Leaving the deep part out
        // would store a quietly partial archive, so it's refused.
        if name.matches('/').count() > MAX_DEPTH {
            return Err(failed(format!(
                "“{}” is nested more than {MAX_DEPTH} folders deep, which is too deep to archive.",
                echoed(&name)
            )));
        }
        let unreadable = |e: io::Error| failed(format!("Couldn't read “{}”: {e}", echoed(&name)));
        let meta = fs::symlink_metadata(path).map_err(unreadable)?;
        let kind = meta.file_type();
        if kind.is_symlink() || !(kind.is_dir() || kind.is_file()) {
            self.skipped.push(name);
            return Ok(());
        }
        if self.items.len() >= max_entries {
            return Err(failed(format!(
                "There are more than {} items to archive. Choose fewer folders.",
                thousands(max_entries)
            )));
        }
        self.items.push(Item {
            path: path.to_path_buf(),
            name: name.clone(),
            dir: kind.is_dir(),
            size: if kind.is_file() { meta.len() } else { 0 },
            mode: mode(&meta),
            mtime: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs()),
            root,
        });
        if kind.is_file() {
            self.files += 1;
            self.bytes += meta.len();
            self.root_bytes[root] += meta.len();
            return Ok(());
        }
        self.folders += 1;
        let mut children = fs::read_dir(path)
            .and_then(|dir| dir.collect::<io::Result<Vec<_>>>())
            .map_err(unreadable)?;
        children.sort_by_key(|c| c.file_name());
        for child in children {
            let child_name = child.file_name();
            let excluded = exclude.is_some_and(|x| {
                x.file_name() == Some(child_name.as_os_str()) && files::same_file(&child.path(), x)
            });
            if excluded {
                continue;
            }
            let child_name = child_name
                .into_string()
                .map_err(|n| not_unicode(&n.to_string_lossy()))?;
            let full = format!("{name}/{child_name}");
            self.add(root, &child.path(), full, exclude, max_entries, cancel)?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn mode(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode(meta: &fs::Metadata) -> u32 {
    if meta.is_dir() { 0o755 } else { 0o644 }
}

fn not_unicode(name: &str) -> JobError {
    failed(format!(
        "“{}” has a name that isn't valid Unicode, so it can't be stored.",
        echoed(name)
    ))
}

/// Writes `plan` to `out` as `target` at `level` (0 stores, 9 compresses most), reporting each
/// root's progress (its index, 0..1) and polling `cancel`. Returns the files left out because
/// they changed after the plan was made (swapped for a link, or resized). `file` is the open
/// handle of the reserved output, which is written through as given and never reopened by path,
/// so a link swapped in after the reservation can't make this write somewhere else. On an error
/// the caller drops the reservation, which deletes the partial archive.
pub fn write(
    plan: &Plan,
    target: Target,
    level: u32,
    file: File,
    cancel: &CancelToken,
    progress: &dyn Fn(usize, f32),
) -> Result<Vec<String>, JobError> {
    let feeder = Feeder {
        cancel,
        report: progress,
        root_bytes: &plan.root_bytes,
        root: Cell::new(None),
        done: Cell::new(0),
        shown: Cell::new(0.0),
        failure: RefCell::new(None),
        changed: RefCell::new(Vec::new()),
    };
    let written = match target {
        Target::Zip => write_zip(plan, level, file, &feeder),
        Target::TarGz => write_tar_gz(plan, level, file, &feeder),
        Target::SevenZ => write_7z(plan, level, file, &feeder),
    };
    match (feeder.failure.take(), written) {
        (Some(e), _) => Err(e),
        (None, Err(_)) if cancel.is_cancelled() => Err(JobError::Cancelled),
        (None, Err(e)) => Err(failed(format!("Couldn't write the archive: {e}"))),
        (None, Ok(())) => Ok(feeder.changed.take()),
    }
}

fn write_zip(plan: &Plan, level: u32, file: File, feeder: &Feeder) -> Result<(), String> {
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let (method, level) = match level {
        0 => (CompressionMethod::Stored, None),
        n => (CompressionMethod::Deflated, Some(i64::from(n))),
    };
    for item in &plan.items {
        feeder.begin(item)?;
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .compression_level(level)
            .unix_permissions(item.mode);
        if item.dir {
            zip.add_directory(item.name.as_str(), options)
                .map_err(text)?;
        } else {
            let Some(mut feed) = feeder.open(item)? else {
                continue;
            };
            zip.start_file(
                &item.name,
                options.large_file(item.size >= u64::from(u32::MAX)),
            )
            .map_err(text)?;
            io::copy(&mut feed, &mut zip).map_err(text)?;
        }
    }
    zip.finish().map_err(text)?.flush().map_err(text)
}

fn write_tar_gz(plan: &Plan, level: u32, file: File, feeder: &Feeder) -> Result<(), String> {
    let gz = GzEncoder::new(BufWriter::new(file), Compression::new(level));
    let mut tar = tar::Builder::new(gz);
    for item in &plan.items {
        feeder.begin(item)?;
        let mut header = Header::new_gnu();
        header.set_mode(item.mode);
        header.set_mtime(item.mtime);
        let appended = if item.dir {
            header.set_entry_type(EntryType::Directory);
            header.set_size(0);
            tar.append_data(&mut header, &item.name, io::empty())
        } else {
            let Some(feed) = feeder.open(item)? else {
                continue;
            };
            header.set_entry_type(EntryType::Regular);
            header.set_size(item.size);
            tar.append_data(&mut header, &item.name, feed)
        };
        appended.map_err(text)?;
    }
    let gz = tar.into_inner().map_err(text)?;
    gz.finish().map_err(text)?.flush().map_err(text)
}

fn write_7z(plan: &Plan, level: u32, file: File, feeder: &Feeder) -> Result<(), String> {
    let mut archive = ArchiveWriter::new(BufWriter::new(file)).map_err(text)?;
    let method = match level {
        0 => EncoderConfiguration::new(EncoderMethod::COPY),
        n => Lzma2Options::from_level(n).into(),
    };
    archive.set_content_methods(vec![method]);
    for item in &plan.items {
        feeder.begin(item)?;
        let entry = ArchiveEntry::from_path(&item.path, item.name.clone());
        let pushed = if item.dir {
            archive.push_archive_entry(entry, None::<io::Empty>)
        } else {
            let Some(feed) = feeder.open(item)? else {
                continue;
            };
            archive.push_archive_entry(entry, Some(feed))
        };
        pushed.map_err(text)?;
    }
    archive.finish().map_err(text)?.flush().map_err(text)
}

fn text(e: impl Display) -> String {
    e.to_string()
}

/// Feeds the files being archived to every format's writer: counts each root's progress,
/// stops on cancel, and keeps the precise reason when reading fails, since the writers wrap
/// read errors in their own.
struct Feeder<'a> {
    cancel: &'a CancelToken,
    report: &'a dyn Fn(usize, f32),
    root_bytes: &'a [u64],
    root: Cell<Option<usize>>,
    done: Cell<u64>,
    shown: Cell<f32>,
    failure: RefCell<Option<JobError>>,
    /// Files that were no longer what the plan saw when they were opened, by archive name.
    changed: RefCell<Vec<String>>,
}

impl Feeder<'_> {
    /// Before each item: polls for cancel, and starts the progress of a new root.
    fn begin(&self, item: &Item) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err(self.fail(JobError::Cancelled).to_string());
        }
        if self.root.get() != Some(item.root) {
            self.root.set(Some(item.root));
            self.done.set(0);
            self.shown.set(0.0);
            (self.report)(item.root, 0.0);
        }
        Ok(())
    }

    /// Opens a file for archiving, or `None` (noted in `changed`) when what was opened isn't
    /// the regular file of the planned length any more: something swapped or resized it
    /// between the walk and now.
    fn open<'f>(&'f self, item: &'f Item) -> Result<Option<Feed<'f>>, String> {
        let opened = File::open(&item.path).and_then(|file| Ok((file.metadata()?, file)));
        let (meta, file) = opened.map_err(|e| self.fail(cant_read(&item.name, e)).to_string())?;
        if !meta.is_file() || meta.len() != item.size || is_reparse_point(&meta) {
            self.changed.borrow_mut().push(item.name.clone());
            return Ok(None);
        }
        Ok(Some(Feed {
            file,
            left: item.size,
            name: &item.name,
            feeder: self,
        }))
    }

    fn advance(&self, n: usize) {
        let Some(root) = self.root.get() else { return };
        self.done.set(self.done.get() + n as u64);
        let total = self.root_bytes.get(root).copied().unwrap_or(0).max(1);
        let done = (self.done.get() as f64 / total as f64).min(1.0) as f32;
        if done >= self.shown.get() + 0.01 {
            self.shown.set(done);
            (self.report)(root, done);
        }
    }

    /// Records the first failure, returning an error for the writer to pass up.
    fn fail(&self, e: JobError) -> io::Error {
        self.failure.borrow_mut().get_or_insert(e);
        io::Error::other("stopped reading the input")
    }
}

/// One file being archived: exactly the size the plan recorded, so a tar header never
/// disagrees with its data.
struct Feed<'a> {
    file: File,
    left: u64,
    name: &'a str,
    feeder: &'a Feeder<'a>,
}

impl Read for Feed<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.feeder.cancel.is_cancelled() {
            return Err(self.feeder.fail(JobError::Cancelled));
        }
        let max = self.left.min(buf.len() as u64) as usize;
        if max == 0 {
            return Ok(0);
        }
        let n = match self.file.read(&mut buf[..max]) {
            Ok(0) => {
                let message = format!(
                    "“{}” got shorter while it was being archived. Try again.",
                    echoed(self.name)
                );
                return Err(self.feeder.fail(failed(message)));
            }
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
            Err(e) => return Err(self.feeder.fail(cant_read(self.name, e))),
        };
        self.left -= n as u64;
        self.feeder.advance(n);
        Ok(n)
    }
}

fn cant_read(name: &str, e: io::Error) -> JobError {
    failed(format!("Couldn't read “{}”: {e}", echoed(name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn temp_dir(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("open-tools-archive-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn roomy() -> Limits {
        Limits {
            entries: 1000,
            bytes: 10_000_000,
        }
    }

    /// Extracts `archive` beside itself with `limits`, returning the output and its row line.
    fn extract_with(archive: &Path, limits: Limits) -> Result<(PathBuf, String), String> {
        let format = Format::detect(archive).unwrap().unwrap();
        let cancel = CancelToken::default();
        match extract(archive, format, None, limits, &cancel, &|_| {}) {
            Ok(done) => Ok(done.keep().unwrap()),
            Err(JobError::Failed(message)) => Err(message),
            Err(JobError::Cancelled) => Err("cancelled".into()),
        }
    }

    fn extract_roomy(archive: &Path) -> Result<(PathBuf, String), String> {
        extract_with(archive, roomy())
    }

    /// Every file (with its bytes) and folder (`None`) below `dir`, by relative `/` path.
    fn tree(dir: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
        fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
                if entry.file_type().unwrap().is_dir() {
                    out.insert(name.clone(), None);
                    walk(&entry.path(), &format!("{name}/"), out);
                } else {
                    out.insert(name, Some(fs::read(entry.path()).unwrap()));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(dir, "", &mut out);
        out
    }

    /// A ZIP holding `entries` under their names exactly as given.
    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    /// A TAR whose entries carry `name` byte for byte (no path checks), with `kind` and `data`.
    fn tar_bytes(entries: &[(&[u8], EntryType, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, kind, data) in entries {
            let mut header = Header::new_gnu();
            header.as_old_mut().name[..name.len()].copy_from_slice(name);
            header.set_entry_type(*kind);
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append(&header, *data).unwrap();
        }
        tar.into_inner().unwrap()
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(data).unwrap();
        gz.finish().unwrap()
    }

    /// A 7z holding `entries` exactly as given (name, Windows attributes, data).
    fn seven_of(path: &Path, entries: &[(&str, u32, &[u8])]) {
        let mut archive = ArchiveWriter::new(File::create(path).unwrap()).unwrap();
        for (name, attributes, data) in entries {
            let mut entry = ArchiveEntry::new_file(name);
            entry.has_windows_attributes = *attributes != 0;
            entry.windows_attributes = *attributes;
            archive.push_archive_entry(entry, Some(*data)).unwrap();
        }
        archive.finish().unwrap();
    }

    /// A folder to archive: text, binary, a nested file and an empty folder.
    fn fixture(dir: &Path) -> PathBuf {
        let root = dir.join("project");
        fs::create_dir_all(root.join("src/empty")).unwrap();
        fs::write(root.join("readme.txt"), "Hello, archive.\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let binary: Vec<u8> = (0..=255u8).cycle().take(77_777).collect();
        fs::write(root.join("data.bin"), binary).unwrap();
        root
    }

    /// Archives `roots` as `target` into `dir`/out.<ext>, returning the archive's path.
    fn archive_of(dir: &Path, roots: &[PathBuf], target: Target, level: u32) -> PathBuf {
        let out = dir.join(format!("out.{}", target.extension()));
        let cancel = CancelToken::default();
        let plan = plan(roots, None, MAX_ENTRIES, &cancel).unwrap();
        let file = File::create(&out).unwrap();
        write(&plan, target, level, file, &cancel, &|_, _| {}).unwrap();
        out
    }

    // ---- Names -----------------------------------------------------------------------------

    #[test]
    fn plain_names_become_relative_paths() {
        assert_eq!(
            relative_path("./docs//guide.md"),
            Ok(PathBuf::from("docs").join("guide.md"))
        );
    }

    #[test]
    fn reserved_device_names_are_refused_with_any_extension() {
        assert!(relative_path("src/aux.c").is_err());
        assert!(relative_path("Com1.tar.gz").is_err());
        assert!(relative_path("lpt¹").is_err());
        assert!(relative_path("CONOUT$").is_err());
    }

    #[test]
    fn names_that_only_start_like_devices_are_allowed() {
        assert!(relative_path("console.log").is_ok());
        assert!(relative_path("COM10").is_ok());
        assert!(relative_path("nullable/aux_data.txt").is_ok());
    }

    #[test]
    fn a_trailing_dot_or_space_is_refused() {
        assert!(relative_path("notes.").is_err());
        assert!(relative_path("dir /a").is_err());
    }

    #[test]
    fn characters_windows_forbids_are_refused() {
        assert!(relative_path("what?.txt").is_err());
        assert!(relative_path("a\u{0}b").is_err());
        assert!(relative_path("line\nbreak").is_err());
    }

    // ---- Formats ---------------------------------------------------------------------------

    #[test]
    fn formats_are_read_from_content_not_the_extension() {
        let dir = temp_dir("detect");
        let disguised = dir.join("photos.zip");
        fs::write(
            &disguised,
            gzip(&tar_bytes(&[(b"a.txt", EntryType::Regular, b"a")])),
        )
        .unwrap();

        let format = Format::detect(&disguised).unwrap();

        assert_eq!(format, Some(Format::TarGz));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_is_no_archive_is_named_so() {
        let dir = temp_dir("not-archive");
        let path = dir.join("notes.zip");
        fs::write(&path, b"just some text").unwrap();

        let probed = probe(&path);

        assert_eq!(probed, Err("Not a ZIP, TAR, GZ or 7z archive.".into()));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_probe_counts_its_entries() {
        let dir = temp_dir("probe-zip");
        let path = dir.join("a.zip");
        zip_of(&path, &[("a.txt", b"a"), ("b/c.txt", b"c")]);

        let probed = probe(&path).unwrap();

        assert_eq!(probed.summary(), "ZIP · 2 items");
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- ZIP guards ------------------------------------------------------------------------

    #[test]
    fn a_zip_entry_climbing_out_with_dot_dot_is_refused() {
        let dir = temp_dir("zip-dotdot");
        let path = dir.join("evil.zip");
        zip_of(&path, &[("../escaped.txt", b"x")]);

        let result = extract_roomy(&path);

        assert_eq!(
            result,
            Err(
                "Refused “../escaped.txt”: it points outside the folder with “..”. \
                 Nothing was extracted."
                    .into()
            )
        );
        assert!(!dir.join("escaped.txt").exists());
        assert!(!dir.join("evil").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_entry_with_an_absolute_path_is_refused() {
        let dir = temp_dir("zip-absolute");
        let path = dir.join("evil.zip");
        zip_of(&path, &[("/tmp/escaped.txt", b"x")]);

        let result = extract_roomy(&path);

        assert_eq!(
            result,
            Err(
                "Refused “/tmp/escaped.txt”: it's an absolute path, outside the folder. \
                 Nothing was extracted."
                    .into()
            )
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_entry_with_a_drive_letter_is_refused() {
        let dir = temp_dir("zip-drive");
        let path = dir.join("evil.zip");
        zip_of(&path, &[("C:/Windows/escaped.txt", b"x")]);

        let result = extract_roomy(&path);

        assert_eq!(
            result,
            Err(
                "Refused “C:/Windows/escaped.txt”: “:” names a drive or a hidden data stream \
                 on Windows. Nothing was extracted."
                    .into()
            )
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_entry_with_a_unc_path_is_refused() {
        let dir = temp_dir("zip-unc");
        let path = dir.join("evil.zip");
        zip_of(&path, &[("//server/share/escaped.txt", b"x")]);

        let result = extract_roomy(&path);

        assert!(result.unwrap_err().contains("absolute path"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_entry_using_backslashes_is_refused() {
        let dir = temp_dir("zip-backslash");
        let path = dir.join("evil.zip");
        zip_of(&path, &[("..\\..\\escaped.txt", b"x")]);

        let result = extract_roomy(&path);

        assert_eq!(
            result,
            Err(
                "Refused “..\\..\\escaped.txt”: it uses backslashes, which Windows reads as \
                 folder separators. Nothing was extracted."
                    .into()
            )
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_entry_with_a_reserved_name_is_refused() {
        let dir = temp_dir("zip-reserved");
        let path = dir.join("evil.zip");
        zip_of(&path, &[("docs/CON.txt", b"x")]);

        let result = extract_roomy(&path);

        assert_eq!(
            result,
            Err(
                "Refused “docs/CON.txt”: Windows reserves that name for a device. \
                 Nothing was extracted."
                    .into()
            )
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_entry_naming_an_alternate_data_stream_is_refused() {
        let dir = temp_dir("zip-ads");
        let path = dir.join("evil.zip");
        zip_of(&path, &[("readme.txt:hidden", b"x")]);

        let result = extract_roomy(&path);

        assert!(result.unwrap_err().contains("hidden data stream"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_symlink_is_skipped_and_listed() {
        let dir = temp_dir("zip-symlink");
        let path = dir.join("links.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.add_symlink("passwd", "../../etc/passwd", SimpleFileOptions::default())
            .unwrap();
        zip.start_file("real.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"real").unwrap();
        zip.finish().unwrap();

        let (out, note) = extract_roomy(&path).unwrap();

        assert_eq!(note, "Extracted 1 file to links · skipped 1 link: “passwd”");
        assert_eq!(
            tree(&out),
            BTreeMap::from([("real.txt".into(), Some(b"real".to_vec()))])
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_with_more_entries_than_allowed_is_stopped() {
        let dir = temp_dir("zip-count");
        let path = dir.join("many.zip");
        zip_of(&path, &[("a", b"a"), ("b", b"b"), ("c", b"c")]);

        let result = extract_with(
            &path,
            Limits {
                entries: 2,
                bytes: 1000,
            },
        );

        assert_eq!(
            result,
            Err(
                "Stopped: the archive holds more than 2 items, so it may be a decompression \
                 bomb. Nothing was extracted."
                    .into()
            )
        );
        assert!(!dir.join("many").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_zip_bomb_is_stopped_at_the_byte_cap() {
        let dir = temp_dir("zip-bomb");
        let path = dir.join("bomb.zip");
        zip_of(&path, &[("zeros.bin", &[0; 100_000])]);

        let result = extract_with(
            &path,
            Limits {
                entries: 10,
                bytes: 1000,
            },
        );

        assert_eq!(
            result,
            Err(
                "Stopped: the archive unpacks to more than 1 KB, so it may be a decompression \
                 bomb. Nothing was extracted."
                    .into()
            )
        );
        assert!(!dir.join("bomb").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_password_protected_zip_is_named_so() {
        let dir = temp_dir("zip-locked");
        let path = dir.join("locked.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        let options =
            SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "secret");
        zip.start_file("a.txt", options).unwrap();
        zip.write_all(b"secret").unwrap();
        zip.finish().unwrap();

        let result = extract_roomy(&path);

        assert_eq!(result, Err(LOCKED.into()));
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- TAR guards ------------------------------------------------------------------------

    #[test]
    fn a_tar_entry_climbing_out_with_dot_dot_is_refused() {
        let dir = temp_dir("tar-dotdot");
        let path = dir.join("evil.tar");
        fs::write(
            &path,
            tar_bytes(&[(b"a/../../escaped.txt", EntryType::Regular, b"x")]),
        )
        .unwrap();

        let result = extract_roomy(&path);

        assert!(result.unwrap_err().contains("points outside the folder"));
        assert!(!dir.join("escaped.txt").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tar_entry_with_an_absolute_path_is_refused() {
        let dir = temp_dir("tar-absolute");
        let path = dir.join("evil.tar");
        fs::write(
            &path,
            tar_bytes(&[(b"/etc/escaped", EntryType::Regular, b"x")]),
        )
        .unwrap();

        let result = extract_roomy(&path);

        assert!(result.unwrap_err().contains("absolute path"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tar_symlinks_and_hard_links_are_skipped_and_listed() {
        let dir = temp_dir("tar-links");
        let path = dir.join("links.tar");
        fs::write(
            &path,
            tar_bytes(&[
                (b"soft", EntryType::Symlink, b""),
                (b"hard", EntryType::Link, b""),
                (b"real.txt", EntryType::Regular, b"real"),
            ]),
        )
        .unwrap();

        let (out, note) = extract_roomy(&path).unwrap();

        assert_eq!(
            note,
            "Extracted 1 file to links · skipped 2 links: “soft”, “hard”"
        );
        assert_eq!(
            tree(&out),
            BTreeMap::from([("real.txt".into(), Some(b"real".to_vec()))])
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tar_device_files_are_skipped_and_listed() {
        let dir = temp_dir("tar-device");
        let path = dir.join("dev.tar");
        fs::write(&path, tar_bytes(&[(b"null", EntryType::Char, b"")])).unwrap();

        let (_, note) = extract_roomy(&path).unwrap();

        assert_eq!(
            note,
            "Extracted 0 files to dev · skipped 1 special file: “null”"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_tar_entry_with_the_same_name_never_overwrites_the_first() {
        let dir = temp_dir("tar-twice");
        let path = dir.join("twice.tar");
        fs::write(
            &path,
            tar_bytes(&[
                (b"a.txt", EntryType::Regular, b"first"),
                (b"a.txt", EntryType::Regular, b"second"),
            ]),
        )
        .unwrap();

        let result = extract_roomy(&path);

        assert_eq!(
            result,
            Err(
                "The archive holds “a.txt” twice, or two names that differ only in case. \
                 Nothing was extracted."
                    .into()
            )
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_oversized_tar_long_name_header_is_refused_before_it_is_read() {
        let dir = temp_dir("tar-longname");
        let path = dir.join("bomb.tar.gz");
        let huge = vec![b'a'; MAX_TAR_HEADER as usize + 1];
        let tar = tar_bytes(&[(b"././@LongLink", EntryType::GNULongName, &huge)]);
        fs::write(&path, gzip(&tar)).unwrap();

        let result = extract_roomy(&path);

        assert_eq!(
            result,
            Err(
                "Stopped: the archive has an oversized TAR header, so it may be a \
                 decompression bomb. Nothing was extracted."
                    .into()
            )
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tar_long_name_is_checked_like_any_other_name() {
        let dir = temp_dir("tar-longname-escape");
        let path = dir.join("evil.tar");
        let tar = tar_bytes(&[
            (
                b"././@LongLink",
                EntryType::GNULongName,
                b"../escaped.txt\0",
            ),
            (b"harmless.txt", EntryType::Regular, b"x"),
        ]);
        fs::write(&path, tar).unwrap();

        let result = extract_roomy(&path);

        assert!(result.unwrap_err().starts_with("Refused “../escaped.txt”"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tar_gz_bomb_is_stopped_at_the_byte_cap() {
        let dir = temp_dir("targz-bomb");
        let path = dir.join("bomb.tar.gz");
        let tar = tar_bytes(&[(b"zeros.bin", EntryType::Regular, &[0; 100_000])]);
        fs::write(&path, gzip(&tar)).unwrap();

        let result = extract_with(
            &path,
            Limits {
                entries: 10,
                bytes: 1000,
            },
        );

        assert!(
            result
                .unwrap_err()
                .starts_with("Stopped: the archive unpacks to more than 1 KB")
        );
        assert!(!dir.join("bomb").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_gz_bomb_is_stopped_and_leaves_no_file() {
        let dir = temp_dir("gz-bomb");
        let path = dir.join("zeros.bin.gz");
        fs::write(&path, gzip(&[0; 100_000])).unwrap();

        let result = extract_with(
            &path,
            Limits {
                entries: 10,
                bytes: 1000,
            },
        );

        assert!(
            result
                .unwrap_err()
                .starts_with("Stopped: the archive unpacks to more than 1 KB")
        );
        assert!(!dir.join("zeros.bin").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- 7z guards -------------------------------------------------------------------------

    #[test]
    fn a_7z_entry_climbing_out_with_dot_dot_is_refused() {
        let dir = temp_dir("7z-dotdot");
        let path = dir.join("evil.7z");
        seven_of(&path, &[("../escaped.txt", 0, b"x")]);

        let result = extract_roomy(&path);

        assert!(result.unwrap_err().starts_with("Refused “../escaped.txt”"));
        assert!(!dir.join("escaped.txt").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_7z_entry_with_a_drive_letter_is_refused() {
        let dir = temp_dir("7z-drive");
        let path = dir.join("evil.7z");
        seven_of(&path, &[("C:/escaped.txt", 0, b"x")]);

        let result = extract_roomy(&path);

        assert!(result.unwrap_err().contains("names a drive"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_7z_symlink_is_skipped_and_the_next_entry_still_reads_right() {
        let dir = temp_dir("7z-symlink");
        let path = dir.join("links.7z");
        let link = UNIX_EXTENSION | ((S_IFLNK | 0o777) << 16);
        seven_of(
            &path,
            &[("passwd", link, b"/etc/passwd"), ("real.txt", 0, b"real")],
        );

        let (out, note) = extract_roomy(&path).unwrap();

        assert_eq!(note, "Extracted 1 file to links · skipped 1 link: “passwd”");
        assert_eq!(
            tree(&out),
            BTreeMap::from([("real.txt".into(), Some(b"real".to_vec()))])
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_7z_with_more_entries_than_allowed_is_stopped() {
        let dir = temp_dir("7z-count");
        let path = dir.join("many.7z");
        seven_of(&path, &[("a", 0, b"a"), ("b", 0, b"b"), ("c", 0, b"c")]);

        let result = extract_with(
            &path,
            Limits {
                entries: 2,
                bytes: 1000,
            },
        );

        assert!(
            result
                .unwrap_err()
                .starts_with("Stopped: the archive holds more than 2 items")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- Cancel and failure ----------------------------------------------------------------

    #[test]
    fn a_cancelled_extraction_leaves_no_folder() {
        let dir = temp_dir("cancelled");
        let path = dir.join("a.zip");
        zip_of(&path, &[("a.txt", b"a")]);
        let cancel = CancelToken::default();
        cancel.cancel();

        let result = extract(&path, Format::Zip, None, roomy(), &cancel, &|_| {});

        assert!(matches!(result, Err(JobError::Cancelled)));
        assert!(!dir.join("a").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failure_partway_deletes_what_was_already_extracted() {
        let dir = temp_dir("partway");
        let path = dir.join("half.zip");
        zip_of(&path, &[("good.txt", b"good"), ("../escaped.txt", b"x")]);

        let result = extract_roomy(&path);

        assert!(result.is_err());
        assert!(!dir.join("half").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extracting_never_writes_into_an_existing_folder() {
        let dir = temp_dir("existing");
        let path = dir.join("a.zip");
        zip_of(&path, &[("a.txt", b"new")]);
        fs::create_dir(dir.join("a")).unwrap();
        fs::write(dir.join("a/a.txt"), b"old").unwrap();

        let (out, _) = extract_roomy(&path).unwrap();

        assert_eq!(out, dir.join("a (1)"));
        assert_eq!(fs::read(dir.join("a/a.txt")).unwrap(), b"old");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cancelled_archive_write_reports_cancelled() {
        let dir = temp_dir("write-cancelled");
        let root = fixture(&dir);
        let cancel = CancelToken::default();
        let plan = plan(&[root], None, MAX_ENTRIES, &cancel).unwrap();
        cancel.cancel();

        let result = write(
            &plan,
            Target::Zip,
            6,
            File::create(dir.join("out.zip")).unwrap(),
            &cancel,
            &|_, _| {},
        );

        assert!(matches!(result, Err(JobError::Cancelled)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writing_goes_through_the_handle_it_is_given() {
        let dir = temp_dir("write-handle");
        let root = fixture(&dir);
        let cancel = CancelToken::default();
        let plan = plan(&[root], None, MAX_ENTRIES, &cancel).unwrap();
        let out = dir.join("out.zip");
        let file = File::create(&out).unwrap();
        // Replacing the path behind the handle doesn't redirect the write.
        fs::rename(&out, dir.join("moved.zip")).unwrap();

        write(&plan, Target::Zip, 6, file, &cancel, &|_, _| {}).unwrap();

        assert!(!out.exists());
        assert!(fs::metadata(dir.join("moved.zip")).unwrap().len() > 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tar_folder_entry_carrying_a_payload_trips_the_byte_cap() {
        let dir = temp_dir("tar-dir-payload");
        let path = dir.join("sneaky.tar");
        fs::write(
            &path,
            tar_bytes(&[(b"folder/", EntryType::Directory, &[0; 100_000])]),
        )
        .unwrap();

        let result = extract_with(
            &path,
            Limits {
                entries: 10,
                bytes: 1000,
            },
        );

        assert!(
            result
                .unwrap_err()
                .starts_with("Stopped: the archive unpacks to more than 1 KB")
        );
        assert!(!dir.join("sneaky").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn declared_memory_is_read_from_each_methods_properties() {
        let lzma = declared_memory(&[0x03, 0x01, 0x01], &[0x5D, 0, 0, 0, 0x40]);
        let lzma2_small = declared_memory(&[0x21], &[18]);
        let lzma2_max = declared_memory(&[0x21], &[40]);
        let ppmd = declared_memory(&[0x03, 0x04, 0x01], &[6, 0, 0, 0, 0x10]);
        let copy = declared_memory(&[0x00], &[]);

        assert_eq!(
            [lzma, lzma2_small, lzma2_max, ppmd, copy],
            [
                Some(0x4000_0000),
                Some(2 << 20),
                Some(u64::from(u32::MAX)),
                Some(0x1000_0000),
                None
            ]
        );
    }

    #[test]
    fn a_7z_declaring_a_huge_dictionary_is_refused_before_decoding() {
        let dir = temp_dir("7z-dictionary");
        let path = dir.join("big-window.7z");
        let mut archive = ArchiveWriter::new(File::create(&path).unwrap()).unwrap();
        let mut options = Lzma2Options::from_level(1);
        options.set_dictionary_size(4 << 20);
        archive.set_content_methods(vec![options.into()]);
        archive
            .push_archive_entry(ArchiveEntry::new_file("a.txt"), Some(&b"abc"[..]))
            .unwrap();
        archive.finish().unwrap();
        let reader = sevenz_rust2::Archive::open(&path).unwrap();

        let result = check_7z_methods(&reader, 1 << 20, u64::MAX);

        assert!(matches!(result, Err(JobError::Failed(m)) if m.contains("of memory to unpack")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_7z_declaring_more_unpacked_bytes_than_the_cap_is_refused_before_decoding() {
        let dir = temp_dir("7z-declared");
        let path = dir.join("declared.7z");
        seven_of(&path, &[("a.bin", 0, &[0; 5000])]);
        let reader = sevenz_rust2::Archive::open(&path).unwrap();

        let result = check_7z_methods(&reader, MAX_7Z_DICTIONARY, 1000);

        assert!(matches!(result, Err(JobError::Failed(m)) if m.contains("unpacks to more than")));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A 7z of just a start header and an encoded next header whose one LZMA coder declares a
    /// `dictionary` window and a header of `unpacked` bytes (a 7z number). There is no packed
    /// data, so anything that tried to decode it would fail differently from the guard.
    fn crafted_7z(dictionary: u32, unpacked: &[u8]) -> Vec<u8> {
        let mut streams = vec![
            0x17, 0x06, 0, 1, 0x09, 10, 0x00, 0x07, 0x0B, 1, 0, 1, 0x23, 3, 1, 1, 5, 0x5D,
        ];
        streams.extend(dictionary.to_le_bytes());
        streams.push(0x0C);
        streams.extend(unpacked);
        streams.extend([0, 0]);
        let mut file = b"7z\xBC\xAF\x27\x1C".to_vec();
        file.extend([0, 4, 0, 0, 0, 0]);
        file.extend(0u64.to_le_bytes());
        file.extend((streams.len() as u64).to_le_bytes());
        file.extend([0; 4]);
        file.extend(streams);
        file
    }

    #[test]
    fn an_encoded_7z_header_declaring_a_huge_dictionary_is_refused_before_it_is_decoded() {
        let mut file = io::Cursor::new(crafted_7z(0x4000_0000, &[100]));

        let result = check_7z_header(&mut file, MAX_7Z_DICTIONARY);

        assert!(matches!(result, Err(m) if m.contains("of memory to unpack")));
    }

    #[test]
    fn an_encoded_7z_header_with_an_ordinary_dictionary_passes() {
        let mut file = io::Cursor::new(crafted_7z(4 << 20, &[100]));

        assert_eq!(check_7z_header(&mut file, MAX_7Z_DICTIONARY), Ok(()));
    }

    #[test]
    fn an_encoded_7z_header_unpacking_to_a_huge_size_is_refused() {
        let mut file = io::Cursor::new(crafted_7z(4 << 20, &[0xE8, 0, 0, 0]));

        let result = check_7z_header(&mut file, MAX_7Z_DICTIONARY);

        assert!(matches!(result, Err(m) if m.contains("header unpacks to more than")));
    }

    #[test]
    fn a_7z_start_header_pointing_past_the_end_of_the_file_is_refused() {
        let mut bytes = crafted_7z(4 << 20, &[100]);
        bytes[12..20].copy_from_slice(&1_000_000u64.to_le_bytes());
        let mut file = io::Cursor::new(bytes);

        let result = check_7z_header(&mut file, MAX_7Z_DICTIONARY);

        assert!(matches!(result, Err(m) if m.contains("header is damaged")));
    }

    #[test]
    fn a_7z_start_header_with_an_overflowing_offset_or_zero_size_is_refused() {
        let mut overflowing = crafted_7z(4 << 20, &[100]);
        overflowing[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
        let mut empty = crafted_7z(4 << 20, &[100]);
        empty[20..28].copy_from_slice(&0u64.to_le_bytes());

        for bytes in [overflowing, empty] {
            let result = check_7z_header(&mut io::Cursor::new(bytes), MAX_7Z_DICTIONARY);

            assert!(matches!(result, Err(m) if m.contains("header is damaged")));
        }
    }

    #[test]
    fn a_7z_signature_without_a_whole_start_header_is_refused() {
        let mut file = io::Cursor::new(b"7z\xBC\xAF\x27\x1C\0\x04".to_vec());

        let result = check_7z_header(&mut file, MAX_7Z_DICTIONARY);

        assert!(matches!(result, Err(m) if m.contains("header is damaged")));
    }

    #[test]
    fn a_damaged_7z_start_header_stops_extract_before_the_reader_can_scan() {
        let dir = temp_dir("7z-start-header");
        let path = dir.join("lost.7z");
        let mut bytes = crafted_7z(4 << 20, &[100]);
        bytes[20..28].copy_from_slice(&0u64.to_le_bytes());
        fs::write(&path, bytes).unwrap();

        let extracted = extract_roomy(&path);

        assert!(matches!(extracted, Err(m) if m.contains("header is damaged")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_encoded_7z_header_cut_short_is_reported_as_damaged() {
        // A number that announces eight more bytes than are left.
        let mut file = io::Cursor::new(crafted_7z(4 << 20, &[0xFF]));

        let result = check_7z_header(&mut file, MAX_7Z_DICTIONARY);

        assert!(matches!(result, Err(m) if m.contains("header is damaged")));
    }

    #[test]
    fn a_7z_with_a_huge_header_dictionary_is_stopped_by_extract_and_probe() {
        let dir = temp_dir("7z-header-window");
        let path = dir.join("bomb.7z");
        fs::write(&path, crafted_7z(0x4000_0000, &[100])).unwrap();

        let extracted = extract_roomy(&path);
        let probed = probe(&path);

        assert!(matches!(extracted, Err(m) if m.contains("of memory to unpack")));
        assert!(matches!(probed, Err(m) if m.contains("of memory to unpack")));
        assert!(!dir.join("bomb").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_real_encoded_7z_header_is_read_and_passes_the_guard() {
        let dir = temp_dir("7z-real-header");
        let path = dir.join("many.7z");
        let names: Vec<String> = (0..200)
            .map(|i| format!("folder/file-{i:04}.txt"))
            .collect();
        let entries: Vec<_> = names.iter().map(|n| (n.as_str(), 0, &b"x"[..])).collect();
        seven_of(&path, &entries);
        let mut file = File::open(&path).unwrap();

        // Many similar names make the header worth compressing, so the writer encodes it.
        assert!(encoded_header_streams(&mut file).unwrap().is_some());
        assert_eq!(check_7z_header(&mut file, MAX_7Z_DICTIONARY), Ok(()));
        assert!(matches!(check_7z_header(&mut file, 1), Err(m) if m.contains("of memory")));
        assert_eq!(probe(&path).unwrap().entries, Some(200));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_7z_number_reads_every_width() {
        let read = |bytes: &[u8]| Bytes(bytes).number();

        assert_eq!(read(&[0x7F]), Some(0x7F));
        assert_eq!(read(&[0x81, 0x02]), Some(0x102));
        assert_eq!(read(&[0xE8, 0, 0, 0]), Some(8 << 24));
        assert_eq!(read(&[0xFF, 1, 0, 0, 0, 0, 0, 0, 0]), Some(1));
        assert_eq!(read(&[0x81]), None);
    }

    #[test]
    fn tar_extension_headers_count_toward_the_entry_cap() {
        let dir = temp_dir("tar-ext-count");
        let path = dir.join("longnames.tar");
        let long_name = (
            b"././@LongLink".as_slice(),
            EntryType::GNULongName,
            b"a\0".as_slice(),
        );
        let mut entries = vec![long_name; 5];
        entries.push((b"file.txt", EntryType::Regular, b"x"));
        fs::write(&path, tar_bytes(&entries)).unwrap();
        let limits = Limits {
            entries: 3,
            bytes: 10_000_000,
        };

        let result = extract_with(&path, limits);

        assert!(matches!(result, Err(m) if m.contains("more than 3 items")));
        assert!(!dir.join("longnames").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_nested_past_the_depth_cap_is_refused_when_planning() {
        let dir = temp_dir("plan-deep");
        let mut deep = dir.join("project");
        for _ in 0..MAX_DEPTH + 40 {
            deep.push("d");
        }
        fs::create_dir_all(&deep).unwrap();

        let result = plan(
            &[dir.join("project")],
            None,
            MAX_ENTRIES,
            &CancelToken::default(),
        );

        assert!(matches!(result, Err(JobError::Failed(m)) if m.contains("too deep to archive")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn echoed_names_are_cut_to_the_limit_and_keep_short_ones() {
        let long = echoed(&"a".repeat(500));
        let wide = echoed(&"é".repeat(300));

        assert_eq!(long, format!("{}…", "a".repeat(MAX_ECHOED)));
        assert_eq!(wide.chars().count(), MAX_ECHOED + 1);
        assert_eq!(echoed("a\nb"), "a\u{FFFD}b");
        assert_eq!(echoed(&"b".repeat(MAX_ECHOED)), "b".repeat(MAX_ECHOED));
    }

    #[test]
    fn a_refusal_echoes_a_crafted_name_only_in_part() {
        let message = match refused(&"x".repeat(5000), "because") {
            JobError::Failed(message) => message,
            JobError::Cancelled => unreachable!(),
        };

        assert!(message.len() < 400);
    }

    #[test]
    fn a_file_resized_after_the_plan_is_left_out_and_listed() {
        let dir = temp_dir("write-resized");
        let root = fixture(&dir);
        let cancel = CancelToken::default();
        let plan = plan(std::slice::from_ref(&root), None, MAX_ENTRIES, &cancel).unwrap();
        fs::write(root.join("readme.txt"), "changed after the walk\n").unwrap();

        let out = File::create(dir.join("out.zip")).unwrap();
        let left_out = write(&plan, Target::Zip, 6, out, &cancel, &|_, _| {}).unwrap();

        assert_eq!(left_out, ["project/readme.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- Round trips -----------------------------------------------------------------------

    #[test]
    fn a_zip_round_trip_is_byte_identical() {
        let dir = temp_dir("round-zip");
        let root = fixture(&dir);
        let archive = archive_of(&dir, std::slice::from_ref(&root), Target::Zip, 9);

        let (out, _) = extract_roomy(&archive).unwrap();

        assert_eq!(tree(&out.join("project")), tree(&root));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stored_zip_round_trip_is_byte_identical() {
        let dir = temp_dir("round-zip-store");
        let root = fixture(&dir);
        let archive = archive_of(&dir, std::slice::from_ref(&root), Target::Zip, 0);

        let (out, _) = extract_roomy(&archive).unwrap();

        assert_eq!(tree(&out.join("project")), tree(&root));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tar_gz_round_trip_is_byte_identical() {
        let dir = temp_dir("round-targz");
        let root = fixture(&dir);
        let archive = archive_of(&dir, std::slice::from_ref(&root), Target::TarGz, 6);

        let (out, _) = extract_roomy(&archive).unwrap();

        assert_eq!(out, dir.join("out"));
        assert_eq!(tree(&out.join("project")), tree(&root));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_7z_round_trip_is_byte_identical() {
        let dir = temp_dir("round-7z");
        let root = fixture(&dir);
        let archive = archive_of(&dir, std::slice::from_ref(&root), Target::SevenZ, 6);

        let (out, _) = extract_roomy(&archive).unwrap();

        assert_eq!(tree(&out.join("project")), tree(&root));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stored_7z_round_trip_is_byte_identical() {
        let dir = temp_dir("round-7z-store");
        let root = fixture(&dir);
        let archive = archive_of(&dir, std::slice::from_ref(&root), Target::SevenZ, 0);

        let (out, _) = extract_roomy(&archive).unwrap();

        assert_eq!(tree(&out.join("project")), tree(&root));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_plain_tar_round_trip_is_byte_identical() {
        let dir = temp_dir("round-tar");
        let root = fixture(&dir);
        let path = dir.join("out.tar");
        let mut tar = tar::Builder::new(File::create(&path).unwrap());
        tar.append_dir_all("project", &root).unwrap();
        tar.finish().unwrap();

        let (out, _) = extract_roomy(&path).unwrap();

        assert_eq!(tree(&out.join("project")), tree(&root));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_gz_round_trip_restores_the_one_file() {
        let dir = temp_dir("round-gz");
        let path = dir.join("notes.txt.gz");
        fs::write(&path, gzip(b"line one\nline two\n")).unwrap();

        let (out, note) = extract_roomy(&path).unwrap();

        assert_eq!(out, dir.join("notes.txt"));
        assert_eq!(note, "Saved as notes.txt");
        assert_eq!(fs::read(&out).unwrap(), b"line one\nline two\n");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn several_roots_keep_their_own_names() {
        let dir = temp_dir("round-roots");
        let root = fixture(&dir);
        let loose = dir.join("loose.txt");
        fs::write(&loose, b"loose").unwrap();
        let archive = archive_of(&dir, &[root, loose], Target::Zip, 6);

        let (out, note) = extract_roomy(&archive).unwrap();

        assert_eq!(note, "Extracted 4 files to out");
        assert_eq!(fs::read(out.join("loose.txt")).unwrap(), b"loose");
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---- Planning --------------------------------------------------------------------------

    #[test]
    fn a_plan_counts_files_folders_and_bytes() {
        let dir = temp_dir("plan-count");
        let root = fixture(&dir);

        let plan = plan(&[root], None, MAX_ENTRIES, &CancelToken::default()).unwrap();

        assert_eq!((plan.files, plan.folders, plan.bytes), (3, 3, 77_806));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_plan_leaves_out_the_archive_being_written() {
        let dir = temp_dir("plan-exclude");
        let root = fixture(&dir);
        let out = root.join("project.partial.zip");
        fs::write(&out, b"").unwrap();

        let plan = plan(&[root], Some(&out), MAX_ENTRIES, &CancelToken::default()).unwrap();

        assert_eq!(plan.files, 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_plan_over_the_item_cap_is_refused() {
        let dir = temp_dir("plan-cap");
        let root = fixture(&dir);

        let result = plan(&[root], None, 4, &CancelToken::default());

        assert!(matches!(
            result,
            Err(JobError::Failed(m)) if m == "There are more than 4 items to archive. Choose fewer folders."
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Makes `link` point at `target`, or `None` where this account can't create symlinks
    /// (Windows without Developer Mode).
    fn symlink(target: &Path, link: &Path) -> Option<()> {
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(target, link);
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(target, link);
        made.map_err(|e| {
            eprintln!("skipped: creating a symlink failed ({e}); needs Developer Mode on Windows")
        })
        .ok()
    }

    #[test]
    fn a_plan_lists_symlinks_without_following_them() {
        let dir = temp_dir("plan-symlink");
        let root = fixture(&dir);
        let outside = dir.join("outside.txt");
        fs::write(&outside, b"outside").unwrap();
        let Some(()) = symlink(&outside, &root.join("link.txt")) else {
            fs::remove_dir_all(&dir).unwrap();
            return;
        };

        let plan = plan(&[root], None, MAX_ENTRIES, &CancelToken::default()).unwrap();

        assert_eq!(plan.skipped, ["project/link.txt"]);
        assert_eq!(plan.files, 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn thousands_get_separators() {
        assert_eq!(thousands(100_000), "100,000");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
