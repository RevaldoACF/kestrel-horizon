use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FullbrightMode {
    Max,
    Soft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoomKey {
    C,
    V,
    Alt,
}

impl ZoomKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::V => "V",
            Self::Alt => "L-Alt",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum XrayKey {
    X,
    B,
    None,
}

impl XrayKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::B => "B",
            Self::None => "Off",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrePreset {
    Valuable,
    All,
}

impl OrePreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Valuable => "Valuable",
            Self::All => "All Ores",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AimTarget {
    Hostile,
    Players,
    Both,
}

impl AimTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hostile => "Hostile",
            Self::Players => "Players",
            Self::Both => "Both",
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrosshairStyle {
    Cross,
    Dot,
    TBar,
}

impl CrosshairStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cross => "Cross",
            Self::Dot => "Dot",
            Self::TBar => "T-Bar",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HudWidget {
    pub enabled: bool,
    /// Normalized screen X (0.0 .. 1.0)
    pub x: f32,
    /// Normalized screen Y (0.0 .. 1.0)
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Waypoint {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub dimension: String,
    pub color_idx: u8,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct KestrelConfig {
    pub fullbright_enabled: bool,
    pub fullbright_mode: FullbrightMode,
    pub fullbright_strength: f32,

    pub xray_terrain: bool,
    pub xray_entities: bool,
    pub xray_key: XrayKey,
    pub xray_ores: bool,
    pub xray_ore_preset: OrePreset,
    pub xray_radius: i32,
    // ESP & Tracers
    pub esp_enabled: bool,
    pub tracers_enabled: bool,
    pub esp_players: bool,
    pub esp_hostile: bool,
    pub esp_passive: bool,
    pub esp_items: bool,
    pub esp_chests: bool,

    // Freecam
    pub freecam_enabled: bool,
    pub freecam_speed: f32,

    // AimAssist
    pub aim_assist: bool,
    pub aim_target: AimTarget,
    pub aim_fov: f32,
    pub aim_range: f32,
    pub aim_smooth: f32,
    pub criticals_enabled: bool,


    pub zoom_enabled: bool,
    pub zoom_fov: u32,
    pub zoom_key: ZoomKey,

    pub crosshair_enabled: bool,
    pub crosshair_style: CrosshairStyle,
    pub crosshair_size: f32,
    pub crosshair_gap: f32,

    pub hud_coords: HudWidget,
    pub hud_nether_convert: bool,
    pub hud_fps: HudWidget,
    pub hud_keystrokes: HudWidget,
    pub hud_vitals: HudWidget,
    pub hud_compass: HudWidget,
    pub hud_clock: HudWidget,
    pub hud_radar: HudWidget,

    pub auto_sprint: bool,
    pub auto_respawn: bool,
    pub auto_eat: bool,
    pub auto_eat_hunger: i32,
    pub auto_totem: bool,
    pub auto_fish: bool,
    pub trigger_bot: bool,
    pub trigger_bot_delay_ms: u32,
    pub auto_clicker: bool,
    pub auto_clicker_cps: u32,
    pub anti_afk: bool,
    pub auto_mine: bool,
    pub speed_mine: bool,
    pub speed_mine_power: u32,
    pub auto_tool: bool,

    pub waypoints: Vec<Waypoint>,
}

impl Default for KestrelConfig {
    fn default() -> Self {
        Self {
            fullbright_enabled: true,
            fullbright_mode: FullbrightMode::Max,
            xray_terrain: false,
            xray_entities: false,
            xray_key: XrayKey::X,
            xray_ores: false,
            xray_ore_preset: OrePreset::Valuable,
            xray_radius: 20,
            esp_enabled: false,
            tracers_enabled: false,
            esp_players: true,
            esp_hostile: true,
            esp_passive: false,
            esp_items: false,
            esp_chests: false,

            freecam_enabled: false,
            freecam_speed: 14.0,

            aim_assist: false,
            aim_target: AimTarget::Both,
            aim_fov: 70.0,
            aim_range: 4.5,
            aim_smooth: 0.45,
            criticals_enabled: false,


            fullbright_strength: 1.0,

            zoom_enabled: true,
            zoom_fov: 30,
            zoom_key: ZoomKey::C,

            crosshair_enabled: false,
            crosshair_style: CrosshairStyle::Cross,
            crosshair_size: 6.0,
            crosshair_gap: 3.0,

            hud_coords: HudWidget {
                enabled: true,
                x: 0.014,
                y: 0.024,
            },
            hud_nether_convert: true,
            hud_fps: HudWidget {
                enabled: true,
                x: 0.014,
                y: 0.145,
            },
            hud_keystrokes: HudWidget {
                enabled: true,
                x: 0.014,
                y: 0.230,
            },
            hud_vitals: HudWidget {
                enabled: true,
                x: 0.780,
                y: 0.024,
            },
            hud_compass: HudWidget {
                enabled: true,
                x: 0.350,
                y: 0.020,
            },
            hud_clock: HudWidget {
                enabled: false,
                x: 0.780,
                y: 0.165,
            },
            hud_radar: HudWidget {
                enabled: true,
                x: 0.850,
                y: 0.215,
            },

            auto_sprint: false,
            auto_respawn: false,
            auto_eat: false,
            auto_eat_hunger: 14,
            auto_totem: false,
            auto_fish: false,
            trigger_bot: false,
            trigger_bot_delay_ms: 650,
            auto_clicker: false,
            auto_clicker_cps: 10,
            anti_afk: false,
            auto_mine: false,
            speed_mine: false,
            speed_mine_power: 2,
            auto_tool: false,

            waypoints: Vec::new(),
        }
    }
}

impl KestrelConfig {
    pub fn config_path() -> PathBuf {
        if let Ok(home) = std::env::var("HORIZON_HOME")
            && !home.trim().is_empty()
        {
            return PathBuf::from(home).join("kestrel-client.json");
        }
        let base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(base).join(".local/share/horizon/kestrel-client.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir failed: {e}"))?;
        }
        let body = serde_json::to_string_pretty(self).map_err(|e| format!("json failed: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, body).map_err(|e| format!("write failed: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename failed: {e}"))?;
        Ok(())
    }

    pub fn reset_hud_positions(&mut self) {
        let def = Self::default();
        self.hud_coords.x = def.hud_coords.x;
        self.hud_coords.y = def.hud_coords.y;
        self.hud_fps.x = def.hud_fps.x;
        self.hud_fps.y = def.hud_fps.y;
        self.hud_keystrokes.x = def.hud_keystrokes.x;
        self.hud_keystrokes.y = def.hud_keystrokes.y;
        self.hud_vitals.x = def.hud_vitals.x;
        self.hud_vitals.y = def.hud_vitals.y;
        self.hud_compass.x = def.hud_compass.x;
        self.hud_compass.y = def.hud_compass.y;
        self.hud_clock.x = def.hud_clock.x;
        self.hud_clock.y = def.hud_clock.y;
        self.hud_radar.x = def.hud_radar.x;
        self.hud_radar.y = def.hud_radar.y;
    }
}
