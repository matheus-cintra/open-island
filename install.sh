#!/bin/sh
set -eu

PROGRAM_NAME=open-island
UNSUPPORTED_DOCS_URL=https://github.com/matheus-cintra/open-island#o-que-não-é-suportado-e-por-quê
RELEASE_REPOSITORY=matheus-cintra/open-island
RELEASE_API_URL=https://api.github.com/repos/$RELEASE_REPOSITORY/releases

EXIT_USAGE=64
EXIT_SESSION_NOT_WAYLAND=10
EXIT_COMPOSITOR_NOT_HYPRLAND=11
EXIT_NO_SYSTEMD_USER_SESSION=12
EXIT_UNSUPPORTED_ARCHITECTURE=13
EXIT_UNKNOWN_DISTRIBUTION=14
EXIT_MISSING_RELEASE_TOOL=15
EXIT_RELEASE_UNAVAILABLE=16
EXIT_CHECKSUM_MISMATCH=17
EXIT_PACKAGE_INSTALL_FAILED=18
EXIT_UNINSTALL_INCOMPLETE=71
EXIT_INSTALL_INCOMPLETE=72

SUDO=""
INSTALL_ROUTE=""
DAEMON_EXECUTABLE=""
RELEASE_TAG=""
RELEASE_VERSION=""
RELEASE_ASSET=""
DOWNLOAD_DIRECTORY=""
INSTALL_SUMMARY=""
REMOVED_SUMMARY=""
FAILED_SUMMARY=""

say() {
	printf '%s\n' "$*"
}

warn() {
	printf 'WARNING: %s\n' "$*" >&2
}

refuse() {
	refusal_code=$1
	shift
	printf '%s: %s\n' "$PROGRAM_NAME" "$*" >&2
	exit "$refusal_code"
}

resolve_elevation() {
	if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then
		SUDO=sudo
	else
		SUDO=""
	fi
}

detected_compositor() {
	if [ -n "${XDG_CURRENT_DESKTOP:-}" ]; then
		printf '%s' "$XDG_CURRENT_DESKTOP"
	elif [ -n "${DESKTOP_SESSION:-}" ]; then
		printf '%s' "$DESKTOP_SESSION"
	else
		printf '%s' "none reported by XDG_CURRENT_DESKTOP or DESKTOP_SESSION"
	fi
}

os_release_field() {
	sed -n "s/^$1=//p" /etc/os-release 2>/dev/null | head -n 1 | tr -d '"'
}

require_wayland_session() {
	if [ -n "${WAYLAND_DISPLAY:-}" ]; then
		return 0
	fi
	if [ "${XDG_SESSION_TYPE:-}" = wayland ]; then
		return 0
	fi
	refuse "$EXIT_SESSION_NOT_WAYLAND" \
		"this is not a Wayland session (WAYLAND_DISPLAY is unset and XDG_SESSION_TYPE is '${XDG_SESSION_TYPE:-unset}'). The island is a layer surface and the zwlr_layer_shell_v1 protocol does not exist on X11, so nothing would ever be drawn. Log into a Wayland session and run this again."
}

require_hyprland() {
	if [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
		return 0
	fi
	refuse "$EXIT_COMPOSITOR_NOT_HYPRLAND" \
		"HYPRLAND_INSTANCE_SIGNATURE is unset; the compositor found here is '$(detected_compositor)'. Hover detection, click-to-jump and the bar-hugging height talk to the Hyprland socket directly, so this release runs on Hyprland only. The README section 'O que não é suportado, e por quê' explains the three couplings: $UNSUPPORTED_DOCS_URL."
}

require_systemd_user_session() {
	if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
		return 0
	fi
	refuse "$EXIT_NO_SYSTEMD_USER_SESSION" \
		"'systemctl --user show-environment' failed: there is no systemd user session for uid $(id -u). open-islandd.service and open-island.service are user units and cannot be started without one."
}

require_supported_architecture() {
	machine=$(uname -m)
	if [ "$machine" = x86_64 ]; then
		return 0
	fi
	refuse "$EXIT_UNSUPPORTED_ARCHITECTURE" \
		"this release only ships x86_64 binaries and this machine reports '$machine'."
}

resolve_install_route() {
	if [ ! -r /etc/os-release ]; then
		refuse "$EXIT_UNKNOWN_DISTRIBUTION" \
			"/etc/os-release is missing or unreadable, so the package route cannot be chosen between deb, rpm and tarball."
	fi
	for family in $(os_release_field ID) $(os_release_field ID_LIKE); do
		case "$family" in
		debian | ubuntu | linuxmint | pop | elementary | raspbian | devuan)
			INSTALL_ROUTE=deb
			return 0
			;;
		fedora | rhel | centos | rocky | almalinux | ol | amzn | mageia | suse | opensuse*| sles)
			INSTALL_ROUTE=rpm
			return 0
			;;
		esac
	done
	INSTALL_ROUTE=tarball
}

check_sound_playback() {
	if command -v pw-play >/dev/null 2>&1; then
		return 0
	fi
	warn "pw-play was not found, so notification sounds will stay silent. Everything else works; install pipewire to get sound back."
}

preflight() {
	require_wayland_session
	require_hyprland
	require_systemd_user_session
	require_supported_architecture
	resolve_install_route
	check_sound_playback
}

require_release_tools() {
	for tool in curl mktemp sha256sum; do
		if ! command -v "$tool" >/dev/null 2>&1; then
			refuse "$EXIT_MISSING_RELEASE_TOOL" \
				"'$tool' was not found on PATH, and it is needed to download and verify the release assets."
		fi
	done
	if [ "$INSTALL_ROUTE" = tarball ] && ! command -v tar >/dev/null 2>&1; then
		refuse "$EXIT_MISSING_RELEASE_TOOL" \
			"'tar' was not found on PATH, and the $INSTALL_ROUTE route unpacks the release archive with it."
	fi
}

cleanup_download_directory() {
	if [ -n "$DOWNLOAD_DIRECTORY" ]; then
		rm -rf "$DOWNLOAD_DIRECTORY"
	fi
}

create_download_directory() {
	DOWNLOAD_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/open-island-install.XXXXXX")
	trap cleanup_download_directory EXIT INT TERM
}

github_download() {
	if [ -n "${GITHUB_TOKEN:-}" ]; then
		curl --fail --silent --show-error --location \
			--header "Authorization: Bearer $GITHUB_TOKEN" "$@"
	else
		curl --fail --silent --show-error --location "$@"
	fi
}

release_tag_name() {
	tr ',' '\n' <"$DOWNLOAD_DIRECTORY/releases.json" |
		sed -n 's#^[^"]*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*#\1#p' |
		head -n 1
}

release_asset_url() {
	tr ',' '\n' <"$DOWNLOAD_DIRECTORY/releases.json" |
		sed -n \
			-e 's#^[^"]*"url"[[:space:]]*:[[:space:]]*"\(https://api\.github\.com/repos/[^"]*/releases/assets/[0-9][0-9]*\)".*#url|\1#p' \
			-e 's#^[^"]*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*#name|\1#p' |
		awk -F'|' -v wanted="$1" '
			$1 == "url" { candidate = $2 }
			$1 == "name" && $2 == wanted { print candidate; exit }
		'
}

resolve_release() {
	if ! github_download --header 'Accept: application/vnd.github+json' \
		--output "$DOWNLOAD_DIRECTORY/releases.json" "$RELEASE_API_URL/latest"; then
		refuse "$EXIT_RELEASE_UNAVAILABLE" \
			"the newest release at $RELEASE_API_URL/latest could not be read. GitHub answers 404 both when $RELEASE_REPOSITORY has published no stable release yet and to anyone who cannot see the repository, so if it is still private, export GITHUB_TOKEN with a token that can read it."
	fi
	RELEASE_TAG=$(release_tag_name)
	if [ -z "$RELEASE_TAG" ]; then
		refuse "$EXIT_RELEASE_UNAVAILABLE" \
			"the release document at $RELEASE_API_URL/latest carries no tag_name, so there is nothing to install."
	fi
	RELEASE_VERSION=${RELEASE_TAG#v}
	case "$INSTALL_ROUTE" in
	deb)
		RELEASE_ASSET="${PROGRAM_NAME}_${RELEASE_VERSION}_amd64.deb"
		;;
	rpm)
		RELEASE_ASSET="${PROGRAM_NAME}-${RELEASE_VERSION}-x86_64.rpm"
		;;
	*)
		RELEASE_ASSET="${PROGRAM_NAME}-${RELEASE_VERSION}-x86_64-linux.tar.gz"
		;;
	esac
	say "$PROGRAM_NAME: the newest release is $RELEASE_TAG and the $INSTALL_ROUTE route installs $RELEASE_ASSET."
}

download_release_asset() {
	asset_name=$1
	asset_url=$(release_asset_url "$asset_name")
	if [ -z "$asset_url" ]; then
		refuse "$EXIT_RELEASE_UNAVAILABLE" \
			"release $RELEASE_TAG carries no asset named '$asset_name'."
	fi
	if ! github_download --header 'Accept: application/octet-stream' \
		--output "$DOWNLOAD_DIRECTORY/$asset_name" "$asset_url"; then
		refuse "$EXIT_RELEASE_UNAVAILABLE" \
			"downloading '$asset_name' from $asset_url failed."
	fi
}

download_release() {
	say "$PROGRAM_NAME: downloading $RELEASE_ASSET and SHA256SUMS into $DOWNLOAD_DIRECTORY."
	download_release_asset "$RELEASE_ASSET"
	download_release_asset SHA256SUMS
}

verify_release() {
	if ! grep "  $RELEASE_ASSET\$" "$DOWNLOAD_DIRECTORY/SHA256SUMS" \
		>"$DOWNLOAD_DIRECTORY/expected.sha256"; then
		refuse "$EXIT_CHECKSUM_MISMATCH" \
			"SHA256SUMS in release $RELEASE_TAG has no line for '$RELEASE_ASSET', so the download cannot be verified. Nothing was installed."
	fi
	if ! (cd "$DOWNLOAD_DIRECTORY" && sha256sum -c expected.sha256); then
		refuse "$EXIT_CHECKSUM_MISMATCH" \
			"'$RELEASE_ASSET' does not match the SHA256 checksum published in SHA256SUMS. The download was discarded and nothing was installed."
	fi
}

install_release() {
	case "$INSTALL_ROUTE" in
	deb)
		say "$PROGRAM_NAME: installing $RELEASE_ASSET with apt-get."
		if ! (cd "$DOWNLOAD_DIRECTORY" && elevated apt-get install -y "./$RELEASE_ASSET"); then
			refuse "$EXIT_PACKAGE_INSTALL_FAILED" \
				"'apt-get install -y ./$RELEASE_ASSET' failed."
		fi
		;;
	rpm)
		say "$PROGRAM_NAME: installing $RELEASE_ASSET with dnf."
		if ! (cd "$DOWNLOAD_DIRECTORY" && elevated dnf install -y "./$RELEASE_ASSET"); then
			refuse "$EXIT_PACKAGE_INSTALL_FAILED" \
				"'dnf install -y ./$RELEASE_ASSET' failed."
		fi
		;;
	*)
		say "$PROGRAM_NAME: unpacking $RELEASE_ASSET into $HOME/.local."
		mkdir -p "$HOME/.local"
		if ! tar -xzf "$DOWNLOAD_DIRECTORY/$RELEASE_ASSET" -C "$HOME/.local"; then
			refuse "$EXIT_PACKAGE_INSTALL_FAILED" \
				"unpacking '$RELEASE_ASSET' into $HOME/.local failed."
		fi
		case ":$PATH:" in
		*":$HOME/.local/bin:"*) ;;
		*)
			warn "$HOME/.local/bin is not on your PATH, so 'open-island' and 'open-islandd' will not be found by name until you add it to your shell profile."
			;;
		esac
		;;
	esac
}

resolve_installed_daemon() {
	case "$INSTALL_ROUTE" in
	deb | rpm)
		DAEMON_EXECUTABLE=/usr/bin/open-islandd
		;;
	*)
		DAEMON_EXECUTABLE="$HOME/.local/bin/open-islandd"
		;;
	esac
	if [ ! -x "$DAEMON_EXECUTABLE" ]; then
		refuse "$EXIT_PACKAGE_INSTALL_FAILED" \
			"$DAEMON_EXECUTABLE is missing after the install, so the autostart units, the agent hooks and the hotkey cannot be configured."
	fi
}

note_outcome() {
	INSTALL_SUMMARY="$INSTALL_SUMMARY  - $1
"
}

run_daemon_install() {
	label=$1
	shift
	daemon_status=0
	"$DAEMON_EXECUTABLE" "$@" || daemon_status=$?
	if [ "$daemon_status" -eq 0 ]; then
		note_outcome "$label: installed"
	else
		note_outcome "$label: FAILED, '$DAEMON_EXECUTABLE $*' exited $daemon_status"
		note_failed "$label"
	fi
}

install_integrations() {
	say "$PROGRAM_NAME: using $DAEMON_EXECUTABLE to install the autostart units, the agent hooks and the hotkey."
	run_daemon_install "the autostart units" autostart install
	run_daemon_install "the agent hooks" hooks install --agent all
	run_daemon_install "the hotkey bind" hotkey install
}

report_install() {
	say ""
	say "$PROGRAM_NAME: $RELEASE_TAG is installed through the $INSTALL_ROUTE route."
	say "$PROGRAM_NAME: the three installers reported:"
	printf '%s' "$INSTALL_SUMMARY"
	say "$PROGRAM_NAME: run this script again whenever you want to update to the newest release."
	if [ -n "$FAILED_SUMMARY" ]; then
		warn "these were not configured:"
		printf '%s' "$FAILED_SUMMARY" >&2
		exit "$EXIT_INSTALL_INCOMPLETE"
	fi
}

run_install() {
	require_release_tools
	create_download_directory
	resolve_release
	download_release
	verify_release
	install_release
	resolve_installed_daemon
	install_integrations
	report_install
}

elevated() {
	if [ -n "$SUDO" ]; then
		"$SUDO" "$@"
	else
		"$@"
	fi
}

note_removed() {
	REMOVED_SUMMARY="$REMOVED_SUMMARY  - $1
"
}

note_failed() {
	FAILED_SUMMARY="$FAILED_SUMMARY  - $1
"
}

resolve_daemon_executable() {
	if command -v open-islandd >/dev/null 2>&1; then
		DAEMON_EXECUTABLE=$(command -v open-islandd)
		return 0
	fi
	for candidate in "$HOME/.local/bin/open-islandd" /usr/bin/open-islandd; do
		if [ -x "$candidate" ]; then
			DAEMON_EXECUTABLE=$candidate
			return 0
		fi
	done
	DAEMON_EXECUTABLE=""
}

run_daemon_uninstall() {
	label=$1
	shift
	daemon_status=0
	"$DAEMON_EXECUTABLE" "$@" || daemon_status=$?
	if [ "$daemon_status" -eq 0 ]; then
		note_removed "$label"
	else
		note_failed "$label: '$DAEMON_EXECUTABLE $*' exited $daemon_status"
	fi
}

count_if_present() {
	if [ -e "$HOME/$1" ] || [ -L "$HOME/$1" ]; then
		leftover_total=$((leftover_total + 1))
	fi
}

count_integration_leftovers() {
	leftover_total=0
	count_if_present .config/systemd/user/open-islandd.service
	count_if_present .config/systemd/user/open-island.service
	count_if_present .local/share/applications/open-island-settings.desktop
	count_if_present .config/hypr/conf/open-island.lua
	count_if_present .config/hypr/conf/open-island.conf
	printf '%s' "$leftover_total"
}

uninstall_integrations() {
	resolve_daemon_executable
	if [ -n "$DAEMON_EXECUTABLE" ]; then
		say "$PROGRAM_NAME: using $DAEMON_EXECUTABLE to remove the agent hooks, the hotkey and the autostart units."
		run_daemon_uninstall "the agent hooks" hooks uninstall --agent all
		run_daemon_uninstall "the hotkey bind" hotkey uninstall
		run_daemon_uninstall "the autostart units" autostart uninstall
		return 0
	fi
	if [ "$(count_integration_leftovers)" -eq 0 ]; then
		say "$PROGRAM_NAME: open-islandd was not found, and it left no agent hooks, hotkey or autostart files behind."
		return 0
	fi
	note_failed "the agent hooks, the hotkey bind and the autostart units: open-islandd was not found on PATH, in $HOME/.local/bin or in /usr/bin, so they were left exactly as they are"
}

remove_file() {
	target="$HOME/$1"
	if [ ! -e "$target" ] && [ ! -L "$target" ]; then
		return 0
	fi
	if rm -f "$target"; then
		note_removed "$target"
	else
		note_failed "$target"
	fi
}

remove_tarball_files() {
	remove_file .local/bin/open-island
	remove_file .local/bin/open-islandd
	remove_file .local/share/applications/open-island.desktop
	remove_file .local/share/icons/hicolor/32x32/apps/open-island.png
	remove_file .local/share/icons/hicolor/128x128/apps/open-island.png
	remove_file .local/share/icons/hicolor/256x256@2/apps/open-island.png
}

remove_with_package_manager() {
	manager=$1
	shift
	if ! command -v "$manager" >/dev/null 2>&1; then
		note_failed "the $PROGRAM_NAME package: $manager was not found"
		return 0
	fi
	package_status=0
	elevated "$manager" "$@" || package_status=$?
	if [ "$package_status" -eq 0 ]; then
		note_removed "the $PROGRAM_NAME package"
	else
		note_failed "the $PROGRAM_NAME package: '$manager $*' exited $package_status"
	fi
}

remove_package() {
	case "$INSTALL_ROUTE" in
	deb)
		if ! command -v dpkg-query >/dev/null 2>&1; then
			note_failed "the $PROGRAM_NAME package: dpkg-query was not found"
			return 0
		fi
		if ! dpkg-query -W -f '${Status}' "$PROGRAM_NAME" 2>/dev/null | grep -q '^install ok installed$'; then
			return 0
		fi
		remove_with_package_manager apt-get remove -y "$PROGRAM_NAME"
		;;
	rpm)
		if ! command -v rpm >/dev/null 2>&1; then
			note_failed "the $PROGRAM_NAME package: rpm was not found"
			return 0
		fi
		if ! rpm -q "$PROGRAM_NAME" >/dev/null 2>&1; then
			return 0
		fi
		remove_with_package_manager dnf remove -y "$PROGRAM_NAME"
		;;
	*)
		remove_tarball_files
		;;
	esac
}

report_uninstall() {
	say ""
	if [ -z "$REMOVED_SUMMARY" ] && [ -z "$FAILED_SUMMARY" ]; then
		say "$PROGRAM_NAME: there was nothing to remove."
		say "$PROGRAM_NAME: your configuration in $HOME/.config/$PROGRAM_NAME was left in place."
		return 0
	fi
	if [ -n "$REMOVED_SUMMARY" ]; then
		say "$PROGRAM_NAME: removed:"
		printf '%s' "$REMOVED_SUMMARY"
	fi
	say "$PROGRAM_NAME: your configuration in $HOME/.config/$PROGRAM_NAME was left in place, along with everything else you wrote yourself."
	if [ -n "$FAILED_SUMMARY" ]; then
		warn "these were not removed:"
		printf '%s' "$FAILED_SUMMARY" >&2
		exit "$EXIT_UNINSTALL_INCOMPLETE"
	fi
}

run_uninstall() {
	resolve_install_route
	uninstall_integrations
	remove_package
	report_uninstall
}

run_darwin() {
    case "$(uname -m)" in
        arm64) mac_arch=aarch64 ;;
        x86_64) mac_arch=x86_64 ;;
        *) refuse "$EXIT_UNSUPPORTED_ARCHITECTURE" "Arquitetura macOS sem suporte." ;;
    esac
    mac_major=$(sw_vers -productVersion | cut -d. -f1)
    [ "$mac_major" -ge 12 ] || refuse "$EXIT_USAGE" "Open Island requer macOS 12 ou superior."
    case "$verb" in
        preflight) say "route: macOS experimental ($mac_arch), DMG manual" ;;
        uninstall)
            mac_daemon="/Applications/Open Island.app/Contents/MacOS/open-islandd"
            [ -x "$mac_daemon" ] || mac_daemon="$HOME/Applications/Open Island.app/Contents/MacOS/open-islandd"
            if [ -x "$mac_daemon" ]; then
                "$mac_daemon" hooks uninstall
                "$mac_daemon" input uninstall
                "$mac_daemon" autostart uninstall
                "$mac_daemon" stop
            fi
            say "Mova Open Island de Aplicativos para a Lixeira. Suas configurações foram preservadas." ;;
        install)
            open "https://github.com/$RELEASE_REPOSITORY/releases/latest/download/open-island-macos-$mac_arch.dmg"
            say "macOS experimental: arraste Open Island para Aplicativos e abra o aplicativo."
            say "Sem notarização: autorize a abertura em Ajustes do Sistema → Privacidade e Segurança."
            say "Atualize pelo aplicativo; se a pasta não permitir escrita, substitua-o pelo DMG." ;;
    esac
}

usage() {
	say "usage: install.sh [--uninstall | --preflight-only]"
	say ""
	say "  (no argument)     install open-island"
	say "  --uninstall       remove open-island; skips the pre-flight"
	say "  --preflight-only  run the pre-flight and report the route it picked"
}

main() {
	verb=install
	while [ "$#" -gt 0 ]; do
		case "$1" in
		--uninstall)
			verb=uninstall
			;;
		--preflight-only)
			verb=preflight
			;;
		-h | --help)
			usage
			return 0
			;;
		*)
			printf '%s: unknown argument %s\n' "$PROGRAM_NAME" "$1" >&2
			usage >&2
			return "$EXIT_USAGE"
			;;
		esac
		shift
	done

	if [ "$(uname -s)" = Darwin ]; then
		run_darwin
		return 0
	fi

	resolve_elevation

	case "$verb" in
	uninstall)
		run_uninstall
		;;
	preflight)
		preflight
		say "route: $INSTALL_ROUTE"
		say "elevation: ${SUDO:-none}"
		;;
	install)
		preflight
		run_install
		;;
	esac
}

main "$@"
