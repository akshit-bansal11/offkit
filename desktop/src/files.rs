//! Files on disk and the native file dialogs, shared by every tool.
//!
//! Every output is a `Reserved` file the job created itself, so nothing else is overwritten
//! and a failed, cancelled or panicked job leaves nothing behind:
//! - `reserve_output` creates a new file next to the source or in a chosen folder, taking
//!   `name (1).ext`, `name (2).ext`, … when a name is in use.
//! - `reserve_replacing` is for a path the user picked in `choose_save_path` (the dialog has
//!   asked before replacing): the job writes a temporary file beside it, and only `keep`
//!   moves that over the user's file, once the job has succeeded.
//!
//! Never open a dialog inside a tool's `with()`: the dialog's nested event loop can run posted
//! closures, which borrow the tool's state again.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// A file-type filter for the open dialogs: a name ("Video") and extensions without the dot.
pub type Filter<'a> = (&'a str, &'a [&'a str]);

fn dialog(title: &str, filter: Option<Filter>) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new().set_title(title);
    match filter {
        Some((name, extensions)) => dialog.add_filter(name, extensions),
        None => dialog,
    }
}

/// Lets the user pick several files; `None` when they cancel.
pub fn choose_files(title: &str, filter: Option<Filter>) -> Option<Vec<PathBuf>> {
    dialog(title, filter).pick_files()
}

/// Lets the user pick one file; `None` when they cancel.
pub fn choose_file(title: &str, filter: Option<Filter>) -> Option<PathBuf> {
    dialog(title, filter).pick_file()
}

/// Lets the user pick a folder for outputs; `None` when they cancel.
pub fn choose_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

/// Asks where to save one output, suggesting `file_name`. The dialog confirms before it
/// returns an existing file; write to the path through `reserve_replacing`, never directly.
pub fn choose_save_path(title: &str, file_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .save_file()
}

/// An output file created for one job. Write to `path()`, then call `keep()` once the job has
/// succeeded. Dropping it without `keep()` (a failure, a cancel, a panic) deletes the file, so
/// no partial output survives. Only the `reserve_*` functions make one, so whoever holds a
/// `Reserved` knows the file is the job's own to delete.
#[must_use = "dropping a Reserved deletes its file"]
pub struct Reserved {
    path: PathBuf,
    /// The file the user chose to replace; `keep` moves the finished output over it.
    replaces: Option<PathBuf>,
    kept: bool,
}

impl Reserved {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keeps the finished output and returns where it now is. For `reserve_replacing`, this is
    /// the moment the user's file is replaced; if that fails, the temporary file is removed.
    pub fn keep(mut self) -> io::Result<PathBuf> {
        let path = match self.replaces.take() {
            // On Windows `rename` replaces an existing file too (MoveFileExW, REPLACE_EXISTING).
            Some(target) => {
                fs::rename(&self.path, &target)?;
                target
            }
            None => self.path.clone(),
        };
        self.kept = true;
        Ok(path)
    }
}

impl Drop for Reserved {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => eprintln!(
                "couldn't remove the unfinished {}: {e}",
                self.path.display()
            ),
        }
    }
}

/// `<stem>.<ext>`, then `<stem> (1).<ext>`, `<stem> (2).<ext>`, … (no dot when `ext` is "").
pub fn candidate_name(stem: &str, ext: &str, n: u32) -> String {
    let ext = if ext.is_empty() {
        String::new()
    } else {
        format!(".{ext}")
    };
    if n == 0 {
        format!("{stem}{ext}")
    } else {
        format!("{stem} ({n}){ext}")
    }
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into())
}

/// Creates the first free `candidate_name` in `dir` atomically (`create_new`), so two jobs
/// racing for one name never share a file.
fn create_first_free(dir: &Path, stem: &str, ext: &str) -> io::Result<PathBuf> {
    for n in 0..10_000 {
        let candidate = dir.join(candidate_name(stem, ext, n));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no free output name"))
}

/// Reserves `<input stem>.<ext>` in `dir` (default: the input's folder), numbered when taken,
/// so the input and existing files are never overwritten.
pub fn reserve_output(input: &Path, dir: Option<&Path>, ext: &str) -> io::Result<Reserved> {
    let dir = dir.or_else(|| input.parent()).unwrap_or(Path::new("."));
    Ok(Reserved {
        path: create_first_free(dir, &stem_of(input), ext)?,
        replaces: None,
        kept: false,
    })
}

/// Reserves a temporary `<stem>.partial.<ext>` beside `target`, a path the user picked in
/// `choose_save_path`. `keep` moves it over `target`; until then the user's file is untouched.
pub fn reserve_replacing(target: &Path) -> io::Result<Reserved> {
    let dir = target.parent().unwrap_or(Path::new("."));
    let ext = target
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = format!("{}.partial", stem_of(target));
    Ok(Reserved {
        path: create_first_free(dir, &stem, &ext)?,
        replaces: Some(target.to_path_buf()),
        kept: false,
    })
}

// --- reserve_dir: pasted verbatim by families that need it ---

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether two paths name the same file, following links; compares the paths as given when
/// either can't be resolved.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Decimal units, as Finder and Explorer's details show them.
pub fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} bytes"),
        1_000..1_000_000 => format!("{:.0} KB", b / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b / 1e6),
        _ => format!("{:.2} GB", b / 1e9),
    }
}

/// Shows the file selected in the platform file manager.
pub fn reveal(path: &Path) {
    #[cfg(windows)]
    let result = {
        use std::os::windows::process::CommandExt;
        // `raw_arg`: Explorer parses `/select,"<path>"` itself and rejects Rust's quoting.
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(drop)
    };
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(drop);
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = open::that(path.parent().unwrap_or(path));
    if let Err(e) = result {
        eprintln!("couldn't show {} in the file manager: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("open-tools-{test}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn candidate_names_count_up_from_the_bare_name() {
        assert_eq!(candidate_name("clip", "mp4", 0), "clip.mp4");
        assert_eq!(candidate_name("clip", "mp4", 2), "clip (2).mp4");
        assert_eq!(candidate_name("README", "", 1), "README (1)");
    }

    #[test]
    fn reserved_outputs_never_overwrite_the_input_or_existing_files() {
        let dir = temp_dir("names");
        let input = dir.join("clip.mp4");
        fs::write(&input, b"source").unwrap();
        fs::write(dir.join("clip (1).mp4"), b"existing").unwrap();

        // Same extension in the same folder: the input itself is taken, so is (1).
        let first = reserve_output(&input, None, "mp4").unwrap();
        let second = reserve_output(&input, None, "mp4").unwrap();

        assert_eq!(first.path(), dir.join("clip (2).mp4"));
        assert_eq!(second.path(), dir.join("clip (3).mp4"));
        assert_eq!(fs::read(&input).unwrap(), b"source");
        assert_eq!(fs::read(dir.join("clip (1).mp4")).unwrap(), b"existing");
        let webm = reserve_output(&input, None, "webm").unwrap();
        assert_eq!(webm.path(), dir.join("clip.webm"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reserved_outputs_go_to_a_chosen_folder_when_given() {
        let dir = temp_dir("custom-dir");
        let out = dir.join("out");
        fs::create_dir_all(&out).unwrap();

        let reserved = reserve_output(&dir.join("photo.png"), Some(&out), "webp").unwrap();

        assert_eq!(reserved.path(), out.join("photo.webp"));
        assert!(reserved.path().is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_reserved_output_dropped_without_keep_is_deleted() {
        let dir = temp_dir("dropped");
        let reserved = reserve_output(&dir.join("a.wav"), None, "mp3").unwrap();
        let path = reserved.path().to_path_buf();

        drop(reserved);

        assert!(!path.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_kept_output_stays() {
        let dir = temp_dir("kept");
        let reserved = reserve_output(&dir.join("a.wav"), None, "mp3").unwrap();

        let kept = reserved.keep().unwrap();

        assert!(kept.is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn replacing_leaves_the_target_alone_until_keep() {
        let dir = temp_dir("replace-dropped");
        let target = dir.join("notes.txt");
        fs::write(&target, b"old").unwrap();
        let reserved = reserve_replacing(&target).unwrap();
        fs::write(reserved.path(), b"half").unwrap();
        let temp = reserved.path().to_path_buf();

        drop(reserved);

        assert_eq!(temp, dir.join("notes.partial.txt"));
        assert!(!temp.exists());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn replacing_moves_the_finished_output_over_the_target_on_keep() {
        let dir = temp_dir("replace-kept");
        let target = dir.join("notes.txt");
        fs::write(&target, b"old").unwrap();
        let reserved = reserve_replacing(&target).unwrap();
        fs::write(reserved.path(), b"new").unwrap();
        let temp = reserved.path().to_path_buf();

        let kept = reserved.keep().unwrap();

        assert_eq!(kept, target);
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn same_file_sees_through_relative_paths() {
        let dir = temp_dir("same-file");
        let file = dir.join("a.txt");
        fs::write(&file, b"x").unwrap();

        let roundabout = dir.join(".").join("a.txt");

        assert!(same_file(&file, &roundabout));
        assert!(!same_file(&file, &dir.join("b.txt")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(format_size(999), "999 bytes");
        assert_eq!(format_size(12_400_000), "12.4 MB");
        assert_eq!(format_size(3_210_000_000), "3.21 GB");
    }

    // --- reserve_dir tests: pasted verbatim by families that need it ---
}
