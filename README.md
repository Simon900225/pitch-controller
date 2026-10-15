# Pitch Controller

It runs on macOS, Windows, and Linux.

## Run

```bash
cd rust
cargo run --release
```

`cargo run` without `--release` is usable (the dev profile compiles the pitch detector with some optimization), but `--release` is the one to play on.

## Permissions

- **macOS:** microphone, plus Accessibility so the app can press keys. The window links to System Settings when that permission is missing.
- **Windows:** microphone. Keys are sent with `SendInput`.
- **Linux:** microphone. Key injection and the global hotkeys use X11. On a Wayland session the app can still show the overlay and light up the controller, but it cannot type into another program.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl+L | Start or stop listening |
| Cmd/Ctrl+T | Toggle test mode |
| Cmd/Ctrl+Shift+P | Toggle key output, including while another app is focused |
| Cmd/Ctrl+Shift+O | Show or hide the overlay, including while another app is focused |

Test mode lights the controller without sending keys. Turn it off, turn output on, and focus the emulator.

The overlay stays above other windows. Turn on **Edit overlay position** in Controls to drag it and resize it from the corners. Otherwise clicks pass through it.

## Profiles

Profiles are JSON. On macOS that is the same file as the Swift app, `~/Library/Application Support/PitchController/profiles.json`, so either app can load a profile saved by the other. Elsewhere the file is:

- Linux: `~/.local/share/PitchController/profiles.json`
- Windows: `%APPDATA%\PitchController\profiles.json`

Key codes in that file are macOS virtual key codes, so a profile saved by either app loads in the other. The default map matches the Swift app (arrows, A, Z, S, X, Q, W, Return, Right Shift).

## Tests

```bash
cargo test
```
