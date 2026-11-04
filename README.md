# nSound

Desktop app (Rust + egui) that runs in the background and pauses media playback
(for example a YouTube tab) as soon as voice activity is detected in a
communicator — Discord to start with — and resumes it when the conversation
goes quiet.

The app listens to **what the communicator is playing** (other people's voices),
never to your microphone, so your own microphone does not trigger anything.

## Download

Grab the latest `nSound.exe` from the [Releases](../../releases) page — every tag
triggers the GitHub Actions workflow in
[`.github/workflows/build.yml`](.github/workflows/build.yml), which tests the
crate and publishes the release build (static CRT, icon embedded).

## How it works

Windows mixes every application's playback into a per-process audio session on
the audio endpoints. Each session exposes `IAudioMeterInformation`, so nSound can
read the peak level of *Discord's output only* without capturing the stream and
without touching the microphone.

- **Master toggle** in the GUI, persisted to `nsound_config.json`.
- **Communicator list** with per-app enable switches and live process status
  (Discord, Microsoft Teams; process names are configurable in code).
- **Audio engine**: WASAPI session peak meter for the communicator's processes
  (all `Discord.exe` children, so the voice engine is included).
- **Duplicate suppression (always on)**: if the communicator re-plays audio that
  already has another source — your own music (screen share with sound,
  activities) or your microphone (mic test, noise-suppression preview) — that
  copy is ignored. Only audio that originates on the call counts.
- **VAD with attack and hold time**: pause is triggered only after the level
  stays above the threshold for the attack delay (default 250 ms, which filters
  notification blips), and playback resumes after the hold window (default
  1200 ms) of silence — refreshed by *any* above-threshold sample, so short gaps
  between words never resume playback.
- **Media controller**: SMTC (`Windows.Media.Control`) sends an explicit
  `Pause`/`Play` to the active media session, with a media-key fallback.
- **Live level meter** in the UI with the threshold rendered as a marker line.
  Silence is shown as `0 dB` instead of a misleading `-100 dB`.
- **Start with Windows**: registers the executable in the per-user `Run` key
  (no administrator rights), toggleable from the UI or with
  `--autostart-enable` / `--autostart-disable`.

## Architecture

```
src/
├── main.rs               # entry point: config/status + background worker + GUI
├── app.rs                # eframe/egui UI, master switch, meters, sliders
├── config.rs             # AppConfig (persisted) and AppStatus (runtime)
├── audio/
│   ├── mod.rs
│   ├── monitor.rs        # polling loop: PIDs -> session peak -> VAD -> media
│   ├── session_meter.rs  # WASAPI per-process/per-session peak values
│   └── vad.rs            # RMS, dBFS, attack/hold state machine (+ unit tests)
├── media/
│   ├── mod.rs
│   └── controller.rs     # SMTC play/pause with media-key fallback
└── process/
    ├── mod.rs
    └── detector.rs       # sysinfo-based process detection (returns PIDs)
```

The worker thread refreshes the communicator process ids once per second, then
samples the peak level of their audio sessions every 25 ms and feeds it into the
VAD state machine, which drives the media controller. Nothing runs on the UI
thread.

## Build and run

Requires a Rust toolchain (1.76 or newer, edition 2021).

```powershell
cargo build --release
cargo run --release
cargo test          # VAD, WASAPI and process-detector unit tests
```

### Toolchain used on this machine

There is no MSVC linker here, so the GNU toolchain plus MinGW-w64 is used:

```powershell
winget install Rustlang.Rustup
winget install BrechtSanders.WinLibs.POSIX.UCRT   # gcc, dlltool, binutils
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
```

The linker path is pinned in `.cargo/config.toml`, which is machine-local and
intentionally git-ignored. With MSVC Build Tools installed instead, delete that
file and build with the default `stable-x86_64-pc-windows-msvc` toolchain.

## Tuning

The level meter in the UI shows what the communicator is rendering right now.
Watch it while Discord is idle and raise the threshold until the bar stays left
of the yellow marker, then confirm the bar crosses it when somebody talks. The
attack delay filters short blips; the hold time keeps playback paused through
pauses between sentences.

### Verifying the audio path

To check that audio detection works at all (for example with no one on voice),
start the app with the diagnostic switch set — it then reacts to audio from
*every* application:

```powershell
$env:NSOUND_CAPTURE_ALL_AUDIO = "1"
cargo run
```

Play any sound (a YouTube video or a Windows sound) and watch the log: it should
pause after roughly the attack delay and resume once the hold window expires.

`--probe` prints the live session levels (communicator output, other apps and
the microphone) without controlling media at all:

```powershell
nsound.exe --probe
```

## Configuration file

Created next to the executable on first run:

```json
{
  "master_enabled": true,
  "rms_threshold": 0.06,
  "attack_ms": 250,
  "hold_time_ms": 1200,
  "auto_resume": true,
  "communicators": [
    {
      "id": "discord",
      "name": "Discord",
      "process_names": ["Discord.exe", "discord", "DiscordCanary.exe", "DiscordPTB.exe"],
      "enabled": true,
      "short_label": "D",
      "brand_color": [88, 101, 242]
    }
  ]
}
```

## Known limitations

- **Windows only for now.** The level of a communicator's output audio is read
  through WASAPI sessions; Linux (PulseAudio/PipeWire monitor) and macOS would
  need their own implementation.
- **Peak meter, not a voice classifier.** Every sound the communicator renders
  counts, including join/leave and notification sounds; the 250 ms attack
  filters most short ones.
- **Duplicate suppression is level-based.** When the communicator mirrors
  another app or the microphone, the copy is ignored as long as it is not more
  than ~2x louder than that other source. Someone talking on the call is
  normally much louder than background music, but with very loud music and a
  quiet participant a detection can be missed.
- **Discord must render to an active output device** that Windows reports as an
  endpoint; sessions on disabled/unplugged devices are invisible.
- **Media transport targets the active session.** If nothing was playing before
  the pause, `auto_resume` may resume a different session than the one that was
  originally paused.
