# Avisos de terceiros

O código do Open Island é licenciado sob a MIT License, cujo texto completo está em
[`LICENSE`](LICENSE).

Este arquivo lista o software de terceiros que o Open Island usa e explica como as
obrigações de licença desse software são cumpridas.

## Bibliotecas de sistema (ligação dinâmica)

O Open Island **não embute** nenhuma biblioteca de sistema. Ele faz ligação dinâmica
contra as bibliotecas já instaladas na máquina do usuário, resolvidas em tempo de
execução pelo carregador dinâmico do sistema.

| Biblioteca | Licença |
| --- | --- |
| GTK 3 | LGPL-2.1-or-later |
| libsoup | LGPL-2.1-or-later |
| WebKitGTK | LGPL-2.1-or-later e BSD-2-Clause (licença dupla: partes do WebKit são LGPL-2.1-or-later, outras são BSD-2-Clause) |
| gtk-layer-shell | LGPL-3.0-or-later no projeto como um todo, conforme o README dele. Os arquivos que o Open Island de fato chama (`src/api.c`) são MIT. O detector de licença do GitHub rotula o projeto como "GPL-3.0", o que é um artefato do arquivo `LICENSE_GPL.txt` na raiz do repositório, e não a licença declarada pelo projeto. Este aviso trata a obrigação como LGPL-3.0-or-later, e a GPL-3.0 é referenciada porque a LGPL-3.0 a incorpora por referência. |
| GLib (`libglib-2.0`, `libgobject-2.0`, `libgio-2.0`) | LGPL-2.1-or-later |
| gdk-pixbuf | LGPL-2.1-or-later |
| cairo | LGPL-2.1-or-later ou MPL-1.1, à escolha de quem usa |
| libdbus | AFL-2.1 ou GPL-2.0-or-later, à escolha de quem usa. O Open Island usa pela AFL-2.1, que é permissiva. |
| glibc (`libc`, `libm`, `ld-linux`) | LGPL-2.1-or-later |
| libgcc | GPL-3.0-or-later com a GCC Runtime Library Exception 3.1, que libera a ligação sem impor copyleft |

### Lista exata das bibliotecas resolvidas no binário

Saída do `ldd` sobre o binário `open-island` produzido por `bun run tauri build`, restrita às
dependências diretas de ligação, que são as entradas `DT_NEEDED` do ELF. O resto do que o `ldd`
imprime — 145 linhas no total — é o fechamento transitivo puxado por estas, resolvido pelo
carregador dinâmico do sistema.

```
	libgtk-layer-shell.so.0 => /usr/lib/libgtk-layer-shell.so.0 (0x00007f973e470000)
	libgdk-3.so.0 => /usr/lib/libgdk-3.so.0 (0x00007f973d913000)
	libgdk_pixbuf-2.0.so.0 => /usr/lib/libgdk_pixbuf-2.0.so.0 (0x00007f973e436000)
	libcairo.so.2 => /usr/lib/libcairo.so.2 (0x00007f973d7d6000)
	libgobject-2.0.so.0 => /usr/lib/libgobject-2.0.so.0 (0x00007f973d773000)
	libglib-2.0.so.0 => /usr/lib/libglib-2.0.so.0 (0x00007f973d610000)
	libdbus-1.so.3 => /usr/lib/libdbus-1.so.3 (0x00007f973e3e1000)
	libwebkit2gtk-4.1.so.0 => /usr/lib/libwebkit2gtk-4.1.so.0 (0x00007f9737c00000)
	libgtk-3.so.0 => /usr/lib/libgtk-3.so.0 (0x00007f9737400000)
	libsoup-3.0.so.0 => /usr/lib/libsoup-3.0.so.0 (0x00007f973d579000)
	libgio-2.0.so.0 => /usr/lib/libgio-2.0.so.0 (0x00007f9737221000)
	libjavascriptcoregtk-4.1.so.0 => /usr/lib/libjavascriptcoregtk-4.1.so.0 (0x00007f9734e00000)
	libgcc_s.so.1 => /usr/lib/libgcc_s.so.1 (0x00007f973d54c000)
	libm.so.6 => /usr/lib/libm.so.6 (0x00007f9734cc9000)
	libc.so.6 => /usr/lib/libc.so.6 (0x00007f9734a00000)
	/lib64/ld-linux-x86-64.so.2 => /usr/lib64/ld-linux-x86-64.so.2 (0x00007f973e4d7000)
```

Como isso casa com a tabela acima:

- `libgtk-layer-shell.so.0` → gtk-layer-shell.
- `libgtk-3.so.0`, `libgdk-3.so.0` → GTK 3.
- `libsoup-3.0.so.0` → libsoup.
- `libwebkit2gtk-4.1.so.0`, `libjavascriptcoregtk-4.1.so.0` → WebKitGTK. O JavaScriptCore é parte do WebKitGTK e vem no mesmo pacote.
- `libglib-2.0.so.0`, `libgobject-2.0.so.0`, `libgio-2.0.so.0` → GLib.
- `libgdk_pixbuf-2.0.so.0` → gdk-pixbuf.
- `libcairo.so.2` → cairo.
- `libdbus-1.so.3` → libdbus.
- `libc.so.6`, `libm.so.6`, `ld-linux-x86-64.so.2` → glibc.
- `libgcc_s.so.1` → libgcc.

Nenhuma biblioteca da tabela deixou de ser resolvida pelo `ldd`, e nenhuma dependência direta
resolvida pelo `ldd` está fora da tabela.

## Como a obrigação da LGPL é cumprida

A ligação é dinâmica, e os pacotes `.deb` e `.rpm` **declaram** essas bibliotecas como
dependências em vez de embuti-las. As declarações ficam em `app/src-tauri/tauri.conf.json`,
nos campos `bundle.linux.deb.depends` e `bundle.linux.rpm.depends`.

Com isso, o usuário pode substituir qualquer uma dessas bibliotecas por uma versão
modificada, sem recompilar o Open Island, apenas trocando a biblioteca compartilhada
instalada no sistema. É exatamente o que a LGPL-2.1 §6(b) e a LGPL-3.0 §4(d)(1) pedem:
"use a suitable shared library mechanism for linking with the Library".

Os textos das licenças LGPL-2.1, LGPL-3.0, GPL-3.0, BSD-2-Clause e MIT dessas
bibliotecas acompanham os pacotes instalados no sistema, normalmente em
`/usr/share/licenses/<pacote>/` ou `/usr/share/doc/<pacote>/copyright`. Também estão
disponíveis nos endereços oficiais:

- LGPL-2.1: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html
- LGPL-3.0: https://www.gnu.org/licenses/lgpl-3.0.html
- GPL-3.0: https://www.gnu.org/licenses/gpl-3.0.html
- BSD-2-Clause: https://opensource.org/license/bsd-2-clause
- MIT: https://opensource.org/license/mit

## Dependências Rust e JavaScript

As dependências Rust (`app/Cargo.lock`) e JavaScript (`app/bun.lock`) são
todas permissivas: MIT, Apache-2.0, BSD, ISC, Unicode ou equivalentes. Nenhuma delas é
GPL-only ou LGPL-only, então nenhuma impõe copyleft ao código do Open Island.

## Fonte

A fonte DepartureMono, em `app/src/assets/fonts/`, é distribuída sob a SIL Open Font
License. O texto completo dessa licença está em
[`app/src/assets/fonts/OFL.txt`](app/src/assets/fonts/OFL.txt).
