# Kestrel Horizon Client

In-process Vulkan + X11 survival client (`libkestrel.so` + `kestrel-inject`) for **Horizon** (Minecraft: Java Edition rewritten in Rust).

## Features

- **Menu Toggle (`~` or `` ` ``)**: In-game Vulkan overlay UI with keyboard navigation (`Tab`, `Enter`, `Arrows`, `Esc`), search bar, live config persistence (`~/.local/share/horizon/kestrel-client.json`), and **Panic** instant-disable button.
- **Fullbright**: Lights up dark caves and night areas without torches.
- **X-Ray Vision**:
  - **Ore X-Ray (3D ESP)**: Highlights buried ores (Diamond, Ancient Debris, Emerald, Gold, Iron, Lapis, Redstone, Chests, Spawners) with distance tags.
  - **Cave Wireframe**: Reveals underground caves and lava pockets.
  - **Chest & Mob Through-Walls**: Renders entities and chests behind walls.
- **ESP, Tracers & Freecam**:
  - **Entity ESP & Tracers**: Color-coded 3D boxes and guide lines for Players (yellow), Hostile Mobs (red), Passive Mobs (green), and Items.
  - **Freecam**: Fly the camera through walls to scout while your character stays safely in place.
- **Combat & Survival Helpers**:
  - **AimAssist** (configurable target filter, FOV, range, and smoothness), **Criticals** (micro-jump critical hits), **AutoMine**, **SpeedMine**, **AutoTool**, **TriggerBot**, **AutoClicker**, **AutoSprint**, **AutoRespawn**, **AutoEat**, **AutoTotem**, **AutoFish**, and **AntiAFK**.
- **Survival HUD & Layout Editor**:
  - Draggable **Entity Radar** (White: You, Yellow: Players, Red: Hostile, Green: Safe), **Coordinates + Nether Converter**, **FPS & Status**, **Keystrokes & CPS**, **Vitals & Gear**, **Waypoint Compass**, **Session Clock**, and **Crosshair+**.
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
