# Shared agent settings must resolve the daemon on the executing machine.
if [ -n "${OPEN_ISLAND_DAEMON:-}" ]; then
    exec "$OPEN_ISLAND_DAEMON" "$@"
fi
case "$(uname -s)" in
    Darwin)
        for oi_daemon in "/Applications/Open Island.app/Contents/MacOS/open-islandd" "$HOME/Applications/Open Island.app/Contents/MacOS/open-islandd"; do
            if [ -x "$oi_daemon" ]; then exec "$oi_daemon" "$@"; fi
        done
        ;;
esac
for oi_daemon in "$HOME/.local/bin/open-islandd" /usr/local/bin/open-islandd /opt/homebrew/bin/open-islandd /usr/bin/open-islandd; do
    if [ -x "$oi_daemon" ]; then exec "$oi_daemon" "$@"; fi
done
exec open-islandd "$@"
