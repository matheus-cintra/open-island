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

## Build do sidecar e daemon ativo

`bun run tauri build` usa `scripts/portable-build.mjs` e seu prebundle recompila
`open-islandd` para o mesmo target antes de empacotar. O build não instala o
resultado nem reinicia o daemon ativo. `cargo build` do app sozinho não recompila
o crate binário do daemon.

Para compilar apenas o daemon, em `app`:

```sh
bun scripts/portable-build.mjs cargo build --release -p open-islandd
node scripts/portable-build.mjs paths
```

Compare versão, PID e data do binário ativo ao diagnosticar comportamento antigo.
Reiniciar ou substituir um serviço instalado é uma ação separada, dependente do
pedido do usuário. QA usa daemon próprio, socket privado e processo com guard.
Nunca reinicie o serviço do usuário como etapa automática de teste.

Builds com `qa-harness`/`qa-webdriver` usam `target/portable-qa`; builds normais
usam `target/portable`. O prebundle Tauri também copia um daemon normal para o
diretório de QA: recompile explicitamente `open-islandd --features qa-harness`
antes de executar o runner nativo. Consulte `docs/native-qa.md`.
