# Contributing

Thanks for helping! Bug reports and pull requests are welcome.

## Add a language
1. Copy `app/src/locales/en-US.json` to `app/src/locales/<code>.json` (for example `de.json`, `ar.json`).
2. Translate the values only. Keep the keys and any `{{placeholders}}` exactly as they are.
3. That's it: the app picks up every `*.json` in that folder automatically, and right-to-left languages (ar, ur, fa, he) are handled for you.
4. Run `cd app && npx vitest run`. Every code the engine emits is listed in `engine/I18N_KEYS.md`.

## Develop
- Engine: `cd engine && cargo test`
- App: `cd app && npm install && npm test && npm run build`
- Build the sidecar: `scripts/build-sidecar.sh` (or `.ps1` on Windows), then `npm run tauri build`.
