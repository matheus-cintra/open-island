use std::{fs, path::Path};

pub const NAME_MAX_CHARS: usize = 36;
pub const SUMMARY_MAX_CHARS: usize = 200;
pub const TRANSCRIPT_MAX_LINES: usize = 12;
pub const TRANSCRIPT_MAX_CHARS: usize = 600;

/// Turns injected by the harness rather than typed by the user. Claude fires
/// `UserPromptSubmit` for these too, so without the guard a background-agent completion
/// notice becomes the session title and its whole body becomes the summary.
const SYSTEM_PROMPT_TAGS: &[&str] = &[
    "<task-notification>",
    "<system-reminder>",
    "<local-command-caveat>",
    "<command-name>",
];

pub fn is_system_prompt(prompt: &str) -> bool {
    let trimmed = prompt.trim_start();
    SYSTEM_PROMPT_TAGS
        .iter()
        .any(|tag| trimmed.starts_with(tag))
}

pub fn summarize(text: &str) -> String {
    truncate(
        text.split_whitespace().collect::<Vec<_>>().join(" ").trim(),
        SUMMARY_MAX_CHARS,
    )
}

/// The agent's answer is markdown and can be kilobytes long; the island shows one line.
pub fn spoken_line(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("```"))?;
    let cleaned = line.replace(['`', '*', '#'], "");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    Some(summarize(cleaned))
}

pub fn transcript_body(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let head = trimmed
        .lines()
        .take(TRANSCRIPT_MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    Some(truncate(head.trim_end(), TRANSCRIPT_MAX_CHARS))
}

pub fn derive_name(prompt: &str) -> Option<String> {
    if is_system_prompt(prompt) {
        return None;
    }
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("```"))?;
    let cleaned = line
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned
        .trim_end_matches(['.', ',', ';', ':', '!', '?'])
        .trim();
    if cleaned.is_empty() {
        return None;
    }
    Some(truncate(cleaned, NAME_MAX_CHARS))
}

pub fn branch_of(cwd: &Path) -> Option<String> {
    let mut current = Some(cwd);
    while let Some(directory) = current {
        if let Some(branch) = read_head(&directory.join(".git")) {
            return Some(branch);
        }
        current = directory.parent();
    }
    None
}

fn read_head(git: &Path) -> Option<String> {
    let head = if git.is_dir() {
        git.join("HEAD")
    } else {
        let pointer = fs::read_to_string(git).ok()?;
        let target = Path::new(pointer.trim().strip_prefix("gitdir:")?.trim()).to_owned();
        let target = if target.is_absolute() {
            target
        } else {
            git.parent()?.join(target)
        };
        target.join("HEAD")
    };
    let contents = fs::read_to_string(head).ok()?;
    let branch = contents.trim().strip_prefix("ref: refs/heads/")?;
    (!branch.is_empty()).then(|| branch.to_owned())
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let head = text
        .char_indices()
        .nth(max_chars - 1)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let cut = text[..head].rfind(' ').unwrap_or(head);
    format!("{}…", text[..cut].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_prompt_is_the_name_unchanged() {
        assert_eq!(
            derive_name("Run the shell command `echo hi`"),
            Some("Run the shell command echo hi".to_owned())
        );
    }

    #[test]
    fn a_long_prompt_is_cut_on_a_word_boundary_within_the_codex_ruler() {
        let name = derive_name(
            "Answer agent questions from the island without ever rendering them as approvals",
        )
        .expect("a name");

        assert_eq!(name, "Answer agent questions from the…");
        assert!(name.chars().count() <= NAME_MAX_CHARS);
    }

    #[test]
    fn the_first_useful_line_wins_over_blank_lines_and_fences() {
        assert_eq!(
            derive_name("\n\n```sh\nInspect the daemon socket\n"),
            Some("Inspect the daemon socket".to_owned())
        );
    }

    #[test]
    fn whitespace_collapses_and_trailing_punctuation_goes_away() {
        assert_eq!(
            derive_name("  Fix   the   jump    resolver.  "),
            Some("Fix the jump resolver".to_owned())
        );
    }

    #[test]
    fn a_prompt_with_nothing_in_it_has_no_name() {
        assert_eq!(derive_name("   \n\n  "), None);
        assert_eq!(derive_name("..."), None);
    }

    #[test]
    fn a_harness_injected_turn_never_becomes_a_name() {
        for prompt in [
            "<task-notification>\n<task-id>a56230c066fddfd83</task-id>\n<status>completed</status>",
            "<system-reminder>\nAs you answer the user's questions\n</system-reminder>",
            "<local-command-caveat>Caveat: the messages below were generated by the user",
            "<command-name>/clear</command-name>",
        ] {
            assert_eq!(derive_name(prompt), None, "{prompt}");
        }
    }

    #[test]
    fn a_typed_prompt_that_merely_mentions_a_tag_is_still_a_name() {
        assert_eq!(
            derive_name("fix the <system-reminder> title bug"),
            Some("fix the <system-reminder> title bug".to_owned())
        );
    }

    #[test]
    fn a_summary_is_cut_to_its_own_ruler() {
        let summary = summarize(&"palavra ".repeat(200));

        assert!(summary.chars().count() <= SUMMARY_MAX_CHARS);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn a_short_summary_only_loses_its_extra_whitespace() {
        assert_eq!(
            summarize("  inspect   the\n socket  "),
            "inspect the socket"
        );
    }

    #[test]
    fn the_branch_comes_from_the_repository_head() {
        let branch = branch_of(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("a branch");

        assert!(!branch.is_empty());
        assert!(!branch.contains(char::is_whitespace));
    }

    #[test]
    fn a_directory_outside_a_repository_has_no_branch() {
        assert_eq!(branch_of(Path::new("/proc")), None);
    }
}
