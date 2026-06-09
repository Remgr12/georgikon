use std::path::PathBuf;

#[cfg(windows)]
const BINARY_NAME: &str = "georgikon.exe";
#[cfg(not(windows))]
const BINARY_NAME: &str = "georgikon";

/// Searches PATH entries for the game binary.
fn find_in_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(BINARY_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Returns the binary to use: configured path → same-dir sibling → PATH.
pub fn resolve_binary(configured: &str) -> Option<PathBuf> {
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        return p.is_file().then_some(p);
    }
    // Same directory as the launcher executable
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name(BINARY_NAME);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    find_in_path()
}

pub fn launch(configured: &str) -> Result<(), String> {
    let bin = resolve_binary(configured)
        .ok_or_else(|| format!("Could not find the georgikon binary. Set its path in Settings."))?;
    std::process::Command::new(bin)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
