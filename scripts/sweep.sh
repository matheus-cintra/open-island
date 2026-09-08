#!/bin/sh
set -eu

PROGRAM_NAME=sweep
SCRIPT_DIRECTORY=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
PATTERN_FILE="$SCRIPT_DIRECTORY/sweep-patterns.txt"

EXIT_USAGE=64
EXIT_CONTAMINATED=1
EXIT_UNUSABLE=65

REPOSITORY_ROOT=""
PATTERN_PATHSPEC=""
PATTERN_BLOB=""
PATTERN_TOTAL=0
WORK_DIRECTORY=""
TOTAL_HITS=0
SURFACE_TOTAL=0
SURFACE_RAN=0
SURFACE_SKIPPED=0

say() {
	printf '%s\n' "$*"
}

refuse() {
	printf '%s: %s\n' "$PROGRAM_NAME" "$*" >&2
	exit "$EXIT_UNUSABLE"
}

usage() {
	say "usage: sweep.sh [REPOSITORY]"
	say ""
	say "  REPOSITORY  git repository to sweep; defaults to the one this script lives in"
}

cleanup_work_directory() {
	if [ -n "$WORK_DIRECTORY" ]; then
		rm -rf "$WORK_DIRECTORY"
	fi
}

count_lines() {
	wc -l <"$1" | tr -d ' \t'
}

guard_errors() {
	if [ -s "$1" ]; then
		printf '%s: %s could not be read:\n' "$PROGRAM_NAME" "$2" >&2
		cat "$1" >&2
		exit "$EXIT_UNUSABLE"
	fi
}

report_surface() {
	surface_label=$1
	surface_note=$2
	surface_output=$3
	surface_hits=$(count_lines "$surface_output")
	SURFACE_RAN=$((SURFACE_RAN + 1))
	say "$surface_label: $surface_hits hits ($surface_note)"
	if [ "$surface_hits" -ne 0 ]; then
		sed 's/^/    /' "$surface_output"
		TOTAL_HITS=$((TOTAL_HITS + surface_hits))
	fi
}

report_not_applicable() {
	SURFACE_SKIPPED=$((SURFACE_SKIPPED + 1))
	say "$1: NOT APPLICABLE ($2)"
}

resolve_repository() {
	candidate=${1:-$SCRIPT_DIRECTORY}
	if [ ! -d "$candidate" ]; then
		refuse "'$candidate' is not a directory."
	fi
	if ! REPOSITORY_ROOT=$(git -C "$candidate" rev-parse --show-toplevel 2>/dev/null); then
		refuse "'$candidate' is not inside a git repository with a work tree."
	fi
}

resolve_patterns() {
	if [ ! -r "$PATTERN_FILE" ]; then
		refuse "the pattern list $PATTERN_FILE is missing or unreadable."
	fi
	if grep -q '^[[:space:]]*$' "$PATTERN_FILE"; then
		refuse "the pattern list $PATTERN_FILE has a blank line, which would match every line of every file."
	fi
	PATTERN_TOTAL=$(count_lines "$PATTERN_FILE")
	PATTERN_BLOB=$(git -C "$REPOSITORY_ROOT" hash-object -- "$PATTERN_FILE")
	case "$PATTERN_FILE" in
	"$REPOSITORY_ROOT"/*)
		PATTERN_PATHSPEC=":(exclude)${PATTERN_FILE#"$REPOSITORY_ROOT"/}"
		;;
	*)
		PATTERN_PATHSPEC=''
		;;
	esac
}

sweep_file_content() {
	label='surface 1 (file content)'
	output="$WORK_DIRECTORY/content"
	errors="$WORK_DIRECTORY/content.err"
	if [ -n "$PATTERN_PATHSPEC" ]; then
		git -C "$REPOSITORY_ROOT" ls-files -z --cached --others --exclude-standard \
			-- . "$PATTERN_PATHSPEC" >"$WORK_DIRECTORY/tracked" 2>"$errors"
	else
		git -C "$REPOSITORY_ROOT" ls-files -z --cached --others --exclude-standard \
			>"$WORK_DIRECTORY/tracked" 2>"$errors"
	fi
	guard_errors "$errors" 'the file list'
	file_total=$(tr -dc '\000' <"$WORK_DIRECTORY/tracked" | wc -c | tr -d ' \t')
	if [ "$file_total" -eq 0 ]; then
		report_not_applicable "$label" \
			'git ls-files --cached --others --exclude-standard listed no file, so there is nothing to read'
		return 0
	fi
	scope="$file_total files from git ls-files --cached --others --exclude-standard, binaries included"
	if [ -n "$PATTERN_PATHSPEC" ]; then
		scope="$scope, ${PATTERN_PATHSPEC#:(exclude)} excluded as the pattern list itself"
	else
		scope="$scope, the pattern list is outside this repository so nothing is excluded"
	fi
	(cd "$REPOSITORY_ROOT" && xargs -0 grep -aHniE -f "$PATTERN_FILE" /dev/null) \
		<"$WORK_DIRECTORY/tracked" >"$output" 2>"$errors" || true
	guard_errors "$errors" 'the file contents'
	report_surface "$label" "$scope" "$output"
}

sweep_file_names() {
	label='surface 2 (file names)'
	output="$WORK_DIRECTORY/names"
	errors="$WORK_DIRECTORY/names.err"
	if git -C "$REPOSITORY_ROOT" rev-parse --verify --quiet HEAD >/dev/null; then
		source_command='git ls-tree -r --name-only HEAD'
		git -C "$REPOSITORY_ROOT" ls-tree -r --name-only HEAD \
			>"$WORK_DIRECTORY/name-list" 2>"$errors"
	else
		source_command='git ls-files --cached --others --exclude-standard, because HEAD does not exist yet'
		git -C "$REPOSITORY_ROOT" ls-files --cached --others --exclude-standard \
			>"$WORK_DIRECTORY/name-list" 2>"$errors"
	fi
	guard_errors "$errors" 'the name list'
	name_total=$(count_lines "$WORK_DIRECTORY/name-list")
	if [ "$name_total" -eq 0 ]; then
		report_not_applicable "$label" "$source_command listed no file, so there is no name to check"
		return 0
	fi
	grep -iE -f "$PATTERN_FILE" "$WORK_DIRECTORY/name-list" >"$output" 2>"$errors" || true
	guard_errors "$errors" 'the name list'
	report_surface "$label" "$name_total names from $source_command" "$output"
}

sweep_commit_metadata() {
	label='surface 3 (commit metadata)'
	output="$WORK_DIRECTORY/commits"
	errors="$WORK_DIRECTORY/commits.err"
	git -C "$REPOSITORY_ROOT" rev-list --all >"$WORK_DIRECTORY/commit-list" 2>"$errors"
	guard_errors "$errors" 'the commit list'
	commit_total=$(count_lines "$WORK_DIRECTORY/commit-list")
	if [ "$commit_total" -eq 0 ]; then
		report_not_applicable "$label" 'the repository has no commits on any ref'
		return 0
	fi
	: >"$output"
	while read -r commit; do
		git -C "$REPOSITORY_ROOT" log -1 \
			--format='%an%n%ae%n%cn%n%ce%n%s%n%b' "$commit" |
			grep -aniE -f "$PATTERN_FILE" |
			sed "s|^|$commit:|" >>"$output"
	done <"$WORK_DIRECTORY/commit-list"
	report_surface "$label" "author, committer, subject and body of $commit_total commits on all refs" "$output"
}

sweep_object_database() {
	label='surface 4 (object database)'
	output="$WORK_DIRECTORY/objects"
	errors="$WORK_DIRECTORY/objects.err"
	git -C "$REPOSITORY_ROOT" cat-file --batch-all-objects \
		--batch-check='%(objectname) %(objecttype)' \
		>"$WORK_DIRECTORY/object-list" 2>"$errors"
	guard_errors "$errors" 'the object database'
	object_total=$(count_lines "$WORK_DIRECTORY/object-list")
	if [ "$object_total" -eq 0 ]; then
		report_not_applicable "$label" 'the object database is empty'
		return 0
	fi
	: >"$output"
	blob_total=0
	blob_skipped=0
	while read -r object_name object_type; do
		if [ "$object_type" != blob ]; then
			continue
		fi
		if [ "$object_name" = "$PATTERN_BLOB" ]; then
			blob_skipped=$((blob_skipped + 1))
			continue
		fi
		blob_total=$((blob_total + 1))
		git -C "$REPOSITORY_ROOT" cat-file blob "$object_name" |
			grep -aniE -f "$PATTERN_FILE" |
			sed "s|^|$object_name:|" >>"$output"
	done <"$WORK_DIRECTORY/object-list"
	report_surface "$label" \
		"$object_total objects, $blob_total blobs read, $blob_skipped identical to the pattern list and skipped, unreachable objects included" \
		"$output"
}

report() {
	say ""
	if [ "$TOTAL_HITS" -ne 0 ]; then
		say "$PROGRAM_NAME: FAIL, $TOTAL_HITS hits across $SURFACE_RAN of $SURFACE_TOTAL surfaces in $REPOSITORY_ROOT."
		return "$EXIT_CONTAMINATED"
	fi
	if [ "$SURFACE_SKIPPED" -ne 0 ]; then
		say "$PROGRAM_NAME: PASS, 0 hits on $SURFACE_RAN of $SURFACE_TOTAL surfaces in $REPOSITORY_ROOT; $SURFACE_SKIPPED reported as not applicable above."
		return 0
	fi
	say "$PROGRAM_NAME: PASS, 0 hits on all $SURFACE_TOTAL surfaces in $REPOSITORY_ROOT."
	say "$PROGRAM_NAME: the four surfaces read bytes and text only. Text drawn into an image, a video or"
	say "$PROGRAM_NAME: a PDF is compressed pixel data and no surface can see it. Review those by eye."
}

main() {
	while [ "$#" -gt 0 ]; do
		case "$1" in
		-h | --help)
			usage
			return 0
			;;
		-*)
			printf '%s: unknown option %s\n' "$PROGRAM_NAME" "$1" >&2
			usage >&2
			return "$EXIT_USAGE"
			;;
		*)
			break
			;;
		esac
		shift
	done
	if [ "$#" -gt 1 ]; then
		printf '%s: expected at most one repository path, got %s\n' "$PROGRAM_NAME" "$#" >&2
		usage >&2
		return "$EXIT_USAGE"
	fi

	resolve_repository "${1:-}"
	resolve_patterns

	WORK_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/open-island-sweep.XXXXXX")
	trap cleanup_work_directory EXIT INT TERM

	say "$PROGRAM_NAME: repository $REPOSITORY_ROOT"
	say "$PROGRAM_NAME: $PATTERN_TOTAL case-insensitive patterns from $PATTERN_FILE"
	say ""

	SURFACE_TOTAL=4
	sweep_file_content
	sweep_file_names
	sweep_commit_metadata
	sweep_object_database

	report
}

main "$@"
