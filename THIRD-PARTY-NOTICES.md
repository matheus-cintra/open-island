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
| ALSA (`libasound.so.2`) | LGPL-2.1-or-later |
| libstdc++ | GPL-3.0-or-later com GCC Runtime Library Exception 3.1 |
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

### Conferência do binário gerado

As dependências diretas do ELF mudam conforme o build. Confira o artefato real
com `readelf -d <open-island>` e as linhas `NEEDED`; não use uma saída antiga de
`ldd` como inventário da versão atual. O build Linux com áudio acrescenta
`libasound.so.2` e `libstdc++.so.6`. Os pacotes declaram `libasound2`/`libstdc++6`
(DEB) e `alsa-lib`/`libstdc++` (RPM), além das dependências gráficas existentes.

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

As versões resolvidas estão em `app/Cargo.lock` e `app/bun.lock`. O ditado local
acrescenta as seguintes bibliotecas e dependências de áudio/FFT. As licenças da
tabela foram conferidas nos manifests dos crates dessas versões:

| Componente | Versão | Licença declarada |
| --- | --- | --- |
| cpal | 0.18.2 | Apache-2.0 |
| whisper-rs | 0.16.0 | Unlicense |
| whisper-rs-sys | 0.15.0 | Unlicense; whisper.cpp/ggml vendorizado sob MIT |
| rubato | 4.0.0 | MIT OR Apache-2.0 |
| audioadapter / audioadapter-buffers | 4.0.0 | MIT OR Apache-2.0 |
| alsa | 0.11.0 | Apache-2.0 / MIT |
| alsa-sys | 0.4.0 | MIT |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 |
| realfft | 3.5.0 | MIT |
| rustfft | 6.4.1 | MIT OR Apache-2.0 |
| primal-check | 0.3.4 | MIT OR Apache-2.0 |
| strength_reduce | 0.2.4 | MIT OR Apache-2.0 |
| transpose | 0.2.3 | MIT OR Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |

O inventário completo das 41 dependências de áudio para os targets publicados,
incluindo dependências transitivas e as atribuições fornecidas pelos projetos,
está em [`audio-manifest.json`](app/src-tauri/resources/licenses/audio-manifest.json).
Os textos legíveis estão em [`AUDIO-LICENSES.txt`](app/src-tauri/resources/licenses/AUDIO-LICENSES.txt).
Ambos acompanham o bundle em `licenses/`. O inventário registra checksums dos
crates, revisões upstream e hashes dos textos; `scripts/verify-audio-licenses.py`
confere a cobertura contra o grafo resolvido e o lockfile, sem rede.

Quando o pacote só fornece uma declaração MIT, os avisos preservam a declaração
e os autores informados e acrescentam os termos padrão, sem inventar titular ou
ano de copyright. Os resumos de licenciamento e observações dos upstreams são
mantidos integralmente. O modelo escolhido pelo usuário não faz parte desse
inventário de código.

O código C/C++ do whisper.cpp é ligado estaticamente; os frameworks CoreAudio,
AVFoundation e Accelerate no macOS são fornecidos pelo sistema. O modelo Whisper
é escolhido pelo usuário e não acompanha os pacotes do aplicativo.

O driver `tauri-plugin-wdio-webdriver` 1.4.0 (MIT) é exclusivo dos builds opcionais
de QA. O protocolo de ponteiro virtual preserva a licença MIT no XML em
`app/scripts/qa/protocols`. Ferramentas, compositor e fixtures de QA não entram no
bundle normal.

## Fonte

A fonte DepartureMono, em `app/src/assets/fonts/`, é distribuída sob a SIL Open Font
License. O texto completo dessa licença está em
[`app/src/assets/fonts/OFL.txt`](app/src/assets/fonts/OFL.txt).
