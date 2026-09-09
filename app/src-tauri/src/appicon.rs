use std::fs;
use std::path::{Path, PathBuf};

use crate::compositor::{self, Compositor};

const MAX_DEPTH: usize = 5;
const MAX_BYTES: u64 = 512 * 1024;

pub fn for_pid(pid: u32) -> Option<String> {
    let class = window_class(pid)?;
    let entry = desktop_entry(&class)?;
    let name = icon_name(&entry)?;
    let path = icon_path(&name)?;
    data_uri(&path)
}

fn window_class(pid: u32) -> Option<String> {
    compositor::current().window_class(pid)
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(home));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }
    let shared =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
    dirs.extend(
        shared
            .split(':')
            .filter(|part| !part.is_empty())
            .map(PathBuf::from),
    );
    dirs
}

fn desktop_entry(class: &str) -> Option<PathBuf> {
    let roots: Vec<PathBuf> = data_dirs()
        .into_iter()
        .map(|dir| dir.join("applications"))
        .filter(|dir| dir.is_dir())
        .collect();
    for root in &roots {
        let direct = root.join(format!("{class}.desktop"));
        if direct.is_file() {
            return Some(direct);
        }
    }
    let lowered = class.to_ascii_lowercase();
    for root in &roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path
                .extension()
                .is_none_or(|extension| extension != "desktop")
            {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("");
            if stem.eq_ignore_ascii_case(class) || stem.to_ascii_lowercase().ends_with(&lowered) {
                return Some(path);
            }
            if reads_key(&path, "StartupWMClass")
                .is_some_and(|value| value.eq_ignore_ascii_case(class))
            {
                return Some(path);
            }
        }
    }
    None
}

fn reads_key(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let prefix = format!("{key}=");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn icon_name(entry: &Path) -> Option<String> {
    reads_key(entry, "Icon")
}

fn current_theme() -> Option<String> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim().trim_matches('\'').to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn icon_path(name: &str) -> Option<PathBuf> {
    let direct = Path::new(name);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct.to_path_buf());
    }
    let theme = current_theme().unwrap_or_default();
    let mut best: Option<(u32, PathBuf)> = None;
    for base in data_dirs() {
        collect(&base.join("icons"), name, 0, &theme, &mut best);
    }
    if let Some(home) = std::env::var_os("HOME") {
        collect(
            &PathBuf::from(home).join(".icons"),
            name,
            0,
            &theme,
            &mut best,
        );
    }
    for base in data_dirs() {
        let pixmaps = base.join("pixmaps");
        for extension in ["png", "svg"] {
            let candidate = pixmaps.join(format!("{name}.{extension}"));
            if candidate.is_file() {
                score_into(&candidate, &theme, &mut best);
            }
        }
    }
    best.map(|(_, path)| path)
}

fn collect(dir: &Path, name: &str, depth: usize, theme: &str, best: &mut Option<(u32, PathBuf)>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, name, depth + 1, theme, best);
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if stem == name && (extension == "png" || extension == "svg") {
            score_into(&path, theme, best);
        }
    }
}

fn score_into(path: &Path, theme: &str, best: &mut Option<(u32, PathBuf)>) {
    let score = score(path, theme);
    if best.as_ref().is_none_or(|(current, _)| score > *current) {
        *best = Some((score, path.to_path_buf()));
    }
}

fn score(path: &Path, theme: &str) -> u32 {
    let text = path.to_string_lossy();
    let mut score = 0;
    if !theme.is_empty() && text.contains(&format!("/{theme}/")) {
        score += 10_000;
    }
    if text.contains("/hicolor/") {
        score += 1_000;
    }
    if text.contains("/scalable/") {
        return score + 512;
    }
    let size = text
        .split('/')
        .find_map(|part| {
            part.split_once('x')
                .and_then(|(left, _)| left.parse::<u32>().ok())
        })
        .unwrap_or(48);
    score + size.min(512)
}

fn data_uri(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > MAX_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mime = match path.extension().and_then(|value| value.to_str()) {
        Some("svg") => "image/svg+xml",
        _ => "image/png",
    };
    Some(format!("data:{mime};base64,{}", base64(&bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let block = chunk.iter().enumerate().fold(0u32, |acc, (index, byte)| {
            acc | (u32::from(*byte) << (16 - 8 * index))
        });
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(ALPHABET[((block >> (18 - 6 * index)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "appicon_tests.rs"]
mod tests;
