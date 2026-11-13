# Kestrel Horizon Client

In-process Vulkan + X11 survival client (`libkestrel.so` + `kestrel-inject`) for **Horizon** (Minecraft: Java Edition rewritten in Rust).

## Features

- **Menu Toggle (`~` or `` ` ``)**: In-game Vulkan overlay UI with keyboard navigation (`Tab`, `Enter`, `Arrows`, `Esc`), search bar, live config persistence (`~/.local/share/horizon/kestrel-client.json`), and **Panic** instant-disable button.
- **Fullbright**: Direct Vulkan `16x16` lightmap `VkImage` interception (`Max 100%` or `Soft Blend`), toggling instantaneously even while standing still.
- **X-Ray Vision**:
  - **Cave Wireframe**: Swaps Horizon terrain solid/opaque pipelines to `VK_POLYGON_MODE_LINE` + `CULL_NONE` at `vkCmdBindPipeline`.
  - **Chests & Mobs ESP**: Renders entities and block entities with `VK_COMPARE_OP_ALWAYS` depth bypass through walls.
  - Configurable quick-toggle hotkey (`X` / `B` / `Off`).
- **Wurst7-Inspired Survival & Utility Modules** (via `horizon-control.sock` when `agentControl:true` in `horizon-extras.txt`):
  - **Entity Radar HUD**: Rotating top-down minimap plotting nearby mobs, players, dropped items, and XP orbs.
  - **AutoSprint**, **AutoRespawn**, **AutoEat** (configurable hunger threshold), **AutoTotem**, **AutoFish**, **TriggerBot** (configurable cooldown), **AutoClicker** (configurable CPS), **AntiAFK**.
- **Survival HUD & Layout Editor**:
  - Draggable **Coordinates + Nether Portal Converter**, **FPS / 1% Low / Fullbright & X-Ray badge**, **Keystrokes + LMB/RMB CPS**, **Vitals / Totem / Target**, **Waypoint Compass**, **Session Clock**, and **Crosshair+**.

## Building on GitHub Actions

Push to `main` or trigger the **Build Kestrel Horizon Client** workflow in `.github/workflows/build.yml` to build `libkestrel.so` and `kestrel-inject` on `ubuntu-latest` and download the `kestrel-horizon-linux-x86_64` artifact.

## Usage (Linux x86_64)

```bash
# Place libkestrel.so and kestrel-inject in the same directory, then:
chmod +x kestrel-inject

# Launch Horizon directly with Kestrel injected:
./kestrel-inject play

# Or hook into Horizon Launcher's Play button:
./kestrel-inject install

# Check status:
./kestrel-inject status
```

Inside Horizon, press `` ` `` or `~` to open the Kestrel Client menu.
