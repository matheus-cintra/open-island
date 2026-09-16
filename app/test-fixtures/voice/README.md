# Synthetic voice fixtures

These files contain generated speech and digital silence, never a person's
recording. Their text/audio are test material under the repository's MIT license.
`manifest.json` records hashes, the exact reference and model, generator version
and license, voice and speed. The generator is eSpeak NG 1.52.0 (GPL-3.0-or-later);
its source/model binaries are not shipped with the application.

From `app`, with eSpeak NG and ffmpeg available locally:

```sh
espeak-ng -v pt-br -s 140 -w target/voice-qa/speech-source.wav \
  'Mostre os testes do projeto e verifique se a mensagem chegou na sessão correta.'
ffmpeg -nostdin -y -i target/voice-qa/speech-source.wav -ac 1 -ar 16000 \
  -c:a pcm_s16le -map_metadata -1 test-fixtures/voice/pt-br.wav
ffmpeg -nostdin -y -f lavfi -i anullsrc=r=16000:cl=mono -t 3 \
  -c:a pcm_s16le -map_metadata -1 test-fixtures/voice/silence.wav
```

The local source build can use `ESPEAK_DATA_PATH` pointing at its build directory;
no system installation is needed. The exact source archive hash is in the
manifest. Regeneration may change container metadata across ffmpeg versions;
verify and deliberately update the manifest after checking the audio parameters.

Build `voice_probe` with `scripts/portable-build.mjs cargo build --release
-p open-island --example voice_probe`. Pass that executable and the separately
verified QA model to `python3 scripts/voice-qa.py --probe … --model … --evidence …`.
Use `--qemu …` for the restricted x86-64-v1 run; `--sysroot …` can provide baseline
system libraries without changing the host. The host and QEMU runs must use the
same executable and native archives as the product build.

The evaluator requires WER ≤ 0.35 after NFC/lowercase/punctuation normalization,
both `testes` and `sessão`, and `no_speech` for silence. A failed recognition is
a failed gate. These fixtures prove neither microphone capture nor native GUI
behavior. The initial default-speed and slower synthesis evaluations both failed
recognition; their outputs are retained in the implementation evidence.

Sources: [eSpeak NG release](https://github.com/espeak-ng/espeak-ng/releases/tag/1.52.0)
and [build instructions](https://github.com/espeak-ng/espeak-ng/blob/1.52.0/docs/building.md).

The product explicitly enables whisper.cpp's default CPU-capable flash attention;
whisper-rs 0.16.0 otherwise defaults it off. Greedy best_of=1, Portuguese,
no translation/context and the sixteen-thread cap stay unchanged. The unchanged
140 wpm fixture passed on the local Linux host after this change (WER 0.214,
both required words, silence rejected); the prior result was WER 0.429 and a
missing required word. The transcript still has errors. Host inference took
about 47 seconds in this run; this is not a general accuracy or performance claim.
Evidence: `.omo/evidence/post-mvp-evolution/goal-voice-reference` and
`goal-voice-flash`. Restricted-CPU and native microphone validation remain open.
