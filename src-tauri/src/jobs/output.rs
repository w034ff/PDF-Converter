//! Writing the output files atomically per design §6.4, §6.5.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use pdfconv_core::naming::split_stem_and_ext;

use crate::AppState;
use crate::error::{ErrorCode, IpcError};
use crate::settings::{self, OutputKind};

/// Lists existing file and directory names directly under `dir` (design §5.2, §6.4).
pub fn list_existing_files(dir: &Path) -> Result<HashSet<String>, IpcError> {
    let mut set = HashSet::new();
    if !dir.exists() {
        return Ok(set);
    }
    let entries = std::fs::read_dir(dir).map_err(|e| {
        IpcError::new(
            ErrorCode::WriteFailed,
            format!("could not read output directory: {e}"),
        )
    })?;
    for entry in entries.flatten() {
        set.insert(entry.file_name().to_string_lossy().to_string());
    }
    Ok(set)
}

/// Mode a converted file is created with before the umask is applied, so a
/// saved file gets the permissions any other new file in the folder gets.
#[cfg(unix)]
const OUTPUT_FILE_MODE: u32 = 0o666;

/// Creates the dot-prefixed temporary file a conversion writes into before
/// renaming it to its final name (design §6.5).
///
/// `tempfile` creates it readable by its owner only, and the rename keeps that.
/// Unix therefore asks for [`OUTPUT_FILE_MODE`] instead; Windows files have no
/// such mode.
pub(crate) fn create_output_temp(output_dir: &Path) -> std::io::Result<tempfile::NamedTempFile> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(".");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(OUTPUT_FILE_MODE));
    }
    builder.tempfile_in(output_dir)
}

/// Checks, before a conversion starts, that the output folder of `kind` is
/// still there and that a file can be created in it (design §6.5).
///
/// A folder that is gone or is not a folder is put back to "not chosen", in
/// the state and in the settings file, since the screen has to ask for another.
/// A folder that cannot be written to stays chosen: the owner may fix its
/// permissions.
///
/// # Errors
///
/// `OutputDirMissing` if `dir` does not exist or is not a folder,
/// `OutputDirNotWritable` if a temporary file cannot be created in it.
pub fn check_output_dir(state: &AppState, kind: OutputKind, dir: &Path) -> Result<(), IpcError> {
    if !dir.is_dir() {
        // A settings file that cannot be written must not hide the real problem.
        let _ = settings::clear_output_dir(state, kind);
        return Err(IpcError::from_code(ErrorCode::OutputDirMissing));
    }
    // The error is not kept: its text names the folder, and the frontend
    // never sees paths (design §1).
    create_output_temp(dir)
        .map(drop)
        .map_err(|_| IpcError::from_code(ErrorCode::OutputDirNotWritable))
}

/// Raises `write_failed`, which stops a conversion from taking up its next
/// item or page, if `err` is a failure to write the output (design §6.5).
/// Returns whether it was one.
pub(super) fn stop_after_write_failure(err: &IpcError, write_failed: &AtomicBool) -> bool {
    let is_write_failure = err.code == ErrorCode::WriteFailed;
    if is_write_failure {
        write_failed.store(true, Ordering::SeqCst);
    }
    is_write_failure
}

/// Saves file bytes atomically into `output_dir` using a dot-prefixed temporary file
/// and `persist_noclobber` (design §6.4, §6.5).
///
/// If a collision occurs (`AlreadyExists`), increments the candidate numeric suffix
/// (`stem (1).ext`, `stem (2).ext`, ...) until an unoccupied name is found.
pub fn save_atomic(
    output_dir: &Path,
    initial_output_name: &str,
    bytes: &[u8],
    used_names_lower: &Mutex<HashSet<String>>,
) -> Result<String, IpcError> {
    let mut temp = create_output_temp(output_dir)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    temp.write_all(bytes)
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;
    temp.flush()
        .map_err(|e| IpcError::new(ErrorCode::WriteFailed, e.to_string()))?;

    persist_unique(temp, output_dir, initial_output_name, used_names_lower)
}

/// Gives the finished temporary file `temp` the name `initial_output_name` in `output_dir`,
/// without replacing a file, as [`save_atomic`] does after it has written the bytes: a name that
/// is taken at that moment gets the next number (design §6.4, §6.5).
pub fn persist_unique(
    mut temp: tempfile::NamedTempFile,
    output_dir: &Path,
    initial_output_name: &str,
    used_names_lower: &Mutex<HashSet<String>>,
) -> Result<String, IpcError> {
    let (stem, ext) = split_stem_and_ext(initial_output_name);
    let mut candidate = initial_output_name.to_string();
    let mut suffix_num = 0usize;

    loop {
        let dest_path = output_dir.join(&candidate);
        match temp.persist_noclobber(&dest_path) {
            Ok(_) => {
                let mut used = used_names_lower
                    .lock()
                    .expect("used_names_lower mutex poisoned");
                used.insert(candidate.to_lowercase());
                return Ok(candidate);
            }
            Err(persist_err) => {
                if persist_err.error.kind() == std::io::ErrorKind::AlreadyExists {
                    temp = persist_err.file;
                    let mut used = used_names_lower
                        .lock()
                        .expect("used_names_lower mutex poisoned");
                    loop {
                        suffix_num += 1;
                        let next_candidate = if ext.is_empty() {
                            format!("{stem} ({suffix_num})")
                        } else {
                            format!("{stem} ({suffix_num}).{ext}")
                        };
                        let next_lower = next_candidate.to_lowercase();
                        if !used.contains(&next_lower) && !output_dir.join(&next_candidate).exists()
                        {
                            used.insert(next_lower);
                            candidate = next_candidate;
                            break;
                        }
                    }
                } else {
                    return Err(IpcError::new(
                        ErrorCode::WriteFailed,
                        persist_err.error.to_string(),
                    ));
                }
            }
        }
    }
}
