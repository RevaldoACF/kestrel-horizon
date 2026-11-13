# Kestrel Horizon Client

Kestrel is an in-process Vulkan and X11 overlay client for Horizon (Minecraft: Java Edition rewritten in Rust) on Linux x86_64. It renders a menu and HUD directly into the game's Vulkan swapchain and communicates with Horizon over its local Unix control socket.

## Quick Start (Linux x86_64)

Download the `kestrel-horizon-linux-x86_64` artifact from GitHub Actions or build from source, then keep `libkestrel.so` and `kestrel-inject` in the same directory:

```bash
chmod +x kestrel-inject

# Launch Horizon directly with Kestrel injected (runs detached; logs to ~/.local/share/horizon/logs/kestrel.log)
./kestrel-inject play

# Or hook into Horizon Launcher's Play button and add an app menu shortcut
./kestrel-inject install

# Check detected Horizon paths, running PIDs, and control socket status
./kestrel-inject status
```

Inside the game, press `` ` `` or `~` to open the Kestrel menu.

### CLI Commands

- `kestrel-inject play [--fg] [--bin PATH] [-- ARGS...]`: Starts `horizon-client` with `LD_PRELOAD` and `--control`. Pass `--fg` to stay attached to the terminal.
- `kestrel-inject install`: Configures Horizon Launcher's `horizon` instance with `LD_PRELOAD`, enables `agentControl:true` in `horizon-extras.txt`, and writes `~/.local/share/applications/horizon-kestrel.desktop`.
- `kestrel-inject uninstall`: Removes the launcher hook and desktop entry.
- `kestrel-inject desktop`: Creates only the desktop entry.
- `kestrel-inject attach [PID]`: Injects `libkestrel.so` into an already-running `horizon-client` process via `ptrace` (note: distributions with `kernel.yama.ptrace_scope = 1` block unprivileged `ptrace` attach, so `play` or `install` is preferred).
- `kestrel-inject status`: Prints resolved binary, resource, library, PID, and socket paths.

### Controls & Config

- `` ` `` or `~`: Toggle the Kestrel menu (`Esc` closes it; `Tab`, `Shift+Tab`, arrow keys, `Enter`, and `Space` navigate without needing a mouse).
- `X`: Toggle Ore X-Ray (configurable in the menu: `X`, `B`, or `Off`).
- `C`: Hold to zoom (configurable: `C`, `V`, or `L-Alt`).
- `Panic` button: Disables active modules right away from the top bar.
- Settings and waypoints save automatically to `$HORIZON_HOME/kestrel-client.json`, or `~/.local/share/horizon/kestrel-client.json` when `HORIZON_HOME` is unset.

## Features

- **Visuals**: Fullbright (Max and Soft modes with strength and gamma sliders), Ore X-Ray (3D boxes with distance tags for Overworld and Nether ores using Valuable or All Ores presets), Cave Wireframe, Chest & Mob Through-Walls, Entity ESP & Tracers (players, hostile mobs, passive mobs, items, chests), Freecam, OptiZoom, and Crosshair+.
- **Combat & Automation**: AimAssist (target filter, FOV, range, smoothness), Criticals (micro-jump on attack), TriggerBot, AutoClicker, AutoMine, SpeedMine, AutoTool, AutoSprint, AutoEat, AutoTotem, AutoFish, AutoRespawn, and AntiAFK.
- **HUD & Waypoints**: Interactive layout editor with draggable widgets (Coordinates with Nether portal converter, FPS & Light, Keystrokes & CPS, Vitals & Gear, Waypoint Compass, Session Clock, and Entity Radar), plus per-dimension custom waypoints.

## Developer Guide

### Architecture

1. **Injection (`crates/kestrel-inject`)**: Resolves `libkestrel.so` and Horizon's `horizon-client` binary and prepared resource directory, then either spawns the game with `LD_PRELOAD` or attaches to a live PID on `x86_64` by resolving `dlopen` in `libc.so.6` and invoking it via `ptrace`. Inside the target process, `kestrel_ctor` (`.init_array`) checks `/proc/self/exe` so hooks activate only in `horizon-client` and stay inert in child processes.
2. **Vulkan & X11 Hooks (`crates/kestrel-hook/src/lib.rs`, `hooks_vk.rs`, `hooks_x11.rs`)**: Interposes `dlsym` to wrap `vkGetInstanceProcAddr`, `vkGetDeviceProcAddr`, and X11/XCB event and pointer-grab functions (`XCheckIfEvent`, `XGetEventData`, `XDefineCursor`, `XGrabPointer`, `xcb_grab_pointer`, and siblings). Vulkan dispatch pointers live in lock-free `OnceLock` statics to avoid mutex contention during rendering. Opening the menu captures input cleanly and restores cursor grab state on close.
3. **Overlay Renderer (`overlay_vk.rs`, `draw.rs`, `font.rs`, `shaders/`)**: Hooks `vkQueuePresentKHR` with triple-buffered frame slots to draw SDF rounded rectangles, borders, elevation shadows, rings, and textured glyph quads from an embedded RGBA font atlas (`ab_glyph`).
4. **Control Socket Bridge (`control_bridge.rs`)**: Runs on a dedicated `kestrel-ctl` thread connected to Horizon's Unix domain socket (`HORIZON_CONTROL_SOCKET`, `$XDG_RUNTIME_DIR/horizon-control.sock`, `/run/user/<uid>/horizon-control.sock`, or `/tmp/horizon-control.sock`) to stream world, entity, block, and inventory state and issue automation commands.

### Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/kestrel-hook` | `cdylib` (`libkestrel.so`) containing Vulkan/X11 hooks, UI, HUD, shaders, and control bridge |
| `crates/kestrel-inject` | CLI binary (`kestrel-inject`) for `LD_PRELOAD` launch, launcher integration, and `ptrace` attach |
| `crates/kestrel-hook/shaders` | GLSL 450 vertex and fragment shaders compiled to SPIR-V at build time |
| `assets/fonts` | Embedded `Bricolage Grotesque` and `Geist Mono` fonts with SIL OFL 1.1 notices |
| `.github/workflows/build.yml` | GitHub Actions workflow building `kestrel-horizon-linux-x86_64` on `ubuntu-latest` |

### Building from Source

Prerequisites on Linux x86_64:

- Rust stable toolchain (edition 2024)
- `glslc` (from `shaderc` or the Vulkan SDK; `crates/kestrel-hook/build.rs` compiles `overlay.vert` and `overlay.frag` to SPIR-V during build)

```bash
cargo build --release --workspace
```

Built artifacts are written to `target/release/libkestrel.so` and `target/release/kestrel-inject`.

## License

This project is licensed under the **GNU General Public License v3.0** (`GPL-3.0-only`). See [LICENSE](LICENSE) for the full text.

Anyone can use, study, modify, and share this codebase, provided any distributed forks or derivative works stay open source under the same GPLv3 terms. You may not take this code into closed-source or proprietary software.

Bundled fonts under `assets/fonts/` are licensed separately under the SIL Open Font License 1.1 (`OFL-BricolageGrotesque.txt` and `OFL-Geist.txt`).
