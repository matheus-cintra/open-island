# open-island — agent instructions

## Shell: never pipe a test run

`cargo test` starts the `open-islandd` daemon and a `dbus-daemon`. Those children inherit
the pipeline's stdout, so the write end of the pipe stays open after `cargo` exits. Any
reader — `grep`, `tail`, `head` — then blocks in `read()` forever and the tool call never
returns. A `timeout` on `cargo` does not help: it kills `cargo`, not its grandchildren, and
not the readers.

Redirect to a file, then filter the file:

```sh
cd app && setsid timeout --kill-after=30 600 cargo test -p open-islandd > /tmp/test.log 2>&1
grep -E "test result:|error|FAILED" /tmp/test.log | head -40
```

- `> file 2>&1` instead of `| grep`. A leaked child inherits a file descriptor, which blocks nothing.
- `setsid` puts the run in its own process group so the whole tree can be killed.
- `timeout --kill-after=30` sends SIGKILL to whatever ignores SIGTERM.

This applies to every long-running command that can spawn a background process: `cargo test`,
`cargo run`, `tauri dev`.

## Tests that spawn a process

Own the child with a guard that kills it in `Drop`, and give it `Stdio::null()`. A manual
`cleanup()` at the end of the test is skipped whenever an assertion panics, and the process
leaks. See `Daemon` in `app/crates/open-islandd/tests/socket.rs` and `TestBus` in
`app/crates/open-islandd/tests/support/notification_bus.rs`.

## Check for leaks after a test run

```sh
pgrep -af "open-islandd --socket /tmp/open-island-test" || echo clean
```

## `bun run tauri build` does not rebuild the daemon

The Tauri CLI compiles `src-tauri` and its dependencies, and `open-islandd` is a separate binary
crate. A change in `open-island-core` that the daemon must see — a config field, a session field,
a protocol change — is invisible until the daemon itself is rebuilt and the unit restarted:

```sh
cd app && cargo build --release -p open-islandd
systemctl --user restart open-islandd.service
```

Proof it bites: the release daemon on this machine ran for four hours past the commit that added
`display.ui_scale`, parsing the key away on every reload, while every front-end test against it
failed for no visible reason. When something the daemon owns does not arrive, compare the binary's
mtime with the commit before reading any more code.
