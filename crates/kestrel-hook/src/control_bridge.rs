use crate::state::STATE;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct RadarEntity {
    pub kind: String, // "mob", "item", "orb", "player"
    pub label: String,
    pub pos: [f64; 3],
    pub distance: f64,
    pub health: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OreKind {
    Diamond,
    Debris,
    Emerald,
    Gold,
    Iron,
    Lapis,
    Redstone,
    Coal,
    Copper,
    Quartz,
    Chest,
    Spawner,
}

impl OreKind {
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Diamond => "Diamond",
            Self::Debris => "Debris",
            Self::Emerald => "Emerald",
            Self::Gold => "Gold",
            Self::Iron => "Iron",
            Self::Lapis => "Lapis",
            Self::Redstone => "Redstone",
            Self::Coal => "Coal",
            Self::Copper => "Copper",
            Self::Quartz => "Quartz",
            Self::Chest => "Chest",
            Self::Spawner => "Spawner",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OreMarker {
    pub pos: [i32; 3],
    pub kind: OreKind,
    pub distance: f64,
}

#[derive(Clone, Debug, Default)]
pub struct LiveGameState {
    pub socket_connected: bool,
    pub in_world: bool,
    pub world_name: String,
    pub screen: String,
    pub dimension: String,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub eye: [f64; 3],
    pub pitch: f32,
    pub health: f32,
    pub absorption: f32,
    pub food: i32,
    pub saturation: f32,
    pub air: i32,
    pub xp_level: i32,
    pub day: i64,
    pub time_of_day: i64,
    pub raining: bool,
    pub thundering: bool,
    pub sprinting: bool,
    pub sneaking: bool,
    pub held_item: Option<String>,
    pub held_raw: Option<String>,
    pub held_damage: i32,
    pub looking_at_kind: String,
    pub looking_at_label: Option<String>,
    pub looking_at_hp: Option<f32>,
    pub looking_at_dist: f64,
    pub effects: Vec<(String, i32, i32)>,
    pub armor: [Option<(String, i32)>; 4],
    pub totem_count: i64,
    pub arrow_count: i64,
    pub offhand_item: Option<String>,
    pub hotbar_items: [Option<String>; 9],
    pub first_totem_slot: Option<usize>,
    pub nearby_entities: Vec<RadarEntity>,
    pub ore_markers: Vec<OreMarker>,
    pub chest_markers: Vec<OreMarker>,
    pub horizon_gamma: Option<f32>,
    pub base_fov: Option<u32>,
}

pub enum ControlCmd {
    SetZoom(bool, u32),
    SetGamma(f32),
}

static CMD_TX: OnceLock<Sender<ControlCmd>> = OnceLock::new();

pub fn start_bridge() {
    if CMD_TX.get().is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel::<ControlCmd>();
    if CMD_TX.set(tx).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("kestrel-ctl".to_string())
        .spawn(move || bridge_loop(rx))
        .ok();
}

pub fn send_cmd(cmd: ControlCmd) {
    if let Some(tx) = CMD_TX.get() {
        let _ = tx.send(cmd);
    }
}

fn socket_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("HORIZON_CONTROL_SOCKET")
        && !p.is_empty()
    {
        out.push(PathBuf::from(p));
    }
    let uid = unsafe { libc::geteuid() };
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR")
        && !rt.is_empty()
    {
        out.push(PathBuf::from(rt).join("horizon-control.sock"));
    }
    out.push(PathBuf::from(format!("/run/user/{uid}/horizon-control.sock")));
    out.push(PathBuf::from("/tmp/horizon-control.sock"));
    out
}

fn connect_socket() -> Option<(BufReader<UnixStream>, UnixStream)> {
    for path in socket_candidates() {
        if !path.exists() {
            continue;
        }
        if let Ok(stream) = UnixStream::connect(&path) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(120)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(120)));
            if let Ok(cloned) = stream.try_clone() {
                return Some((BufReader::new(cloned), stream));
            }
        }
    }
    None
}

fn send_line(reader: &mut BufReader<UnixStream>, writer: &mut UnixStream, cmd: &str) -> Option<Value> {
    if writeln!(writer, "{cmd}").is_err() || writer.flush().is_err() {
        return None;
    }
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => serde_json::from_str::<Value>(line.trim()).ok(),
    }
}

fn short_item_name(id: &str) -> String {
    id.strip_prefix("minecraft:")
        .unwrap_or(id)
        .replace('_', " ")
}

fn is_edible_food(item: &str) -> bool {
    matches!(
        item,
        "minecraft:cooked_beef"
            | "minecraft:cooked_porkchop"
            | "minecraft:cooked_mutton"
            | "minecraft:cooked_chicken"
            | "minecraft:cooked_salmon"
            | "minecraft:cooked_cod"
            | "minecraft:cooked_rabbit"
            | "minecraft:golden_carrot"
            | "minecraft:bread"
            | "minecraft:baked_potato"
            | "minecraft:apple"
            | "minecraft:golden_apple"
            | "minecraft:enchanted_golden_apple"
            | "minecraft:carrot"
            | "minecraft:melon_slice"
            | "minecraft:pumpkin_pie"
            | "minecraft:sweet_berries"
            | "minecraft:glow_berries"
            | "minecraft:dried_kelp"
            | "minecraft:mushroom_stew"
            | "minecraft:rabbit_stew"
            | "minecraft:beetroot_soup"
            | "minecraft:honey_bottle"
            | "minecraft:beef"
            | "minecraft:porkchop"
            | "minecraft:chicken"
            | "minecraft:mutton"
    )
}

fn parse_state(v: &Value, dst: &mut LiveGameState) {
    dst.socket_connected = true;
    dst.screen = v["screen"].as_str().unwrap_or("unknown").to_string();
    dst.dimension = v["dimension"].as_str().unwrap_or("minecraft:overworld").to_string();
    dst.world_name = v["world"].as_str().unwrap_or("").to_string();

    if let Some(p) = v.get("player").and_then(|x| x.as_object()) {
        dst.in_world = true;
        if let Some(arr) = p.get("position").and_then(|x| x.as_array())
            && arr.len() >= 3
        {
            dst.pos = [
                arr[0].as_f64().unwrap_or(0.0),
                arr[1].as_f64().unwrap_or(0.0),
                arr[2].as_f64().unwrap_or(0.0),
            ];
        }
        if let Some(arr) = p.get("eye").and_then(|x| x.as_array())
            && arr.len() >= 3
        {
            dst.eye = [
                arr[0].as_f64().unwrap_or(dst.pos[0]),
                arr[1].as_f64().unwrap_or(dst.pos[1] + 1.62),
                arr[2].as_f64().unwrap_or(dst.pos[2]),
            ];
        } else {
            dst.eye = [dst.pos[0], dst.pos[1] + 1.62, dst.pos[2]];
        }
        dst.yaw = p.get("yaw").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        dst.pitch = p.get("pitch").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        dst.health = p.get("health").and_then(|x| x.as_f64()).unwrap_or(20.0) as f32;
        dst.absorption = p.get("absorption").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        dst.food = p.get("food").and_then(|x| x.as_i64()).unwrap_or(20) as i32;
        dst.saturation = p.get("saturation").and_then(|x| x.as_f64()).unwrap_or(5.0) as f32;
        dst.air = p.get("air").and_then(|x| x.as_i64()).unwrap_or(300) as i32;
        dst.xp_level = p
            .get("xp")
            .and_then(|x| x.get("level"))
            .and_then(|x| x.as_i64())
            .unwrap_or(0) as i32;
        dst.sprinting = p.get("sprinting").and_then(|x| x.as_bool()).unwrap_or(false);
        dst.sneaking = p.get("sneaking").and_then(|x| x.as_bool()).unwrap_or(false);

        dst.effects.clear();
        if let Some(effs) = p.get("effects").and_then(|x| x.as_array()) {
            for e in effs {
                let id = e["id"].as_str().unwrap_or("").to_string();
                let ticks = e["duration_ticks"].as_i64().unwrap_or(0) as i32;
                let amp = e["amplifier"].as_i64().unwrap_or(0) as i32;
                dst.effects.push((id, (ticks + 19) / 20, amp));
            }
        }
    } else {
        dst.in_world = false;
    }

    dst.day = v["time"]["day"].as_i64().unwrap_or(0);
    dst.time_of_day = v["time"]["time_of_day"].as_i64().unwrap_or(0);
    dst.raining = v["weather"]["raining"].as_bool().unwrap_or(false);
    dst.thundering = v["weather"]["thundering"].as_bool().unwrap_or(false);

    if let Some(held) = v.get("held").and_then(|x| x.as_object()) {
        let raw = held.get("item").and_then(|x| x.as_str()).map(str::to_string);
        dst.held_item = raw.as_deref().map(short_item_name);
        dst.held_raw = raw;
        dst.held_damage = held.get("damage").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
    } else {
        dst.held_item = None;
        dst.held_raw = None;
        dst.held_damage = 0;
    }

    let look_kind = v["looking_at"]["kind"].as_str().unwrap_or("none");
    dst.looking_at_kind = look_kind.to_string();
    match look_kind {
        "entity" => {
            let ty = v["looking_at"]["entity"]["type"]
                .as_str()
                .map(short_item_name);
            let hp = v["looking_at"]["entity"]["health"]
                .as_f64()
                .map(|x| x as f32);
            let d = v["looking_at"]["entity"]["distance"]
                .as_f64()
                .unwrap_or(99.0);
            dst.looking_at_label = ty;
            dst.looking_at_hp = hp;
            dst.looking_at_dist = d;
        }
        "block" => {
            dst.looking_at_label = v["looking_at"]["block"]["block"]
                .as_str()
                .map(short_item_name);
            dst.looking_at_hp = None;
            dst.looking_at_dist = v["looking_at"]["block"]["distance"]
                .as_f64()
                .unwrap_or(99.0);
        }
        _ => {
            dst.looking_at_label = None;
            dst.looking_at_hp = None;
            dst.looking_at_dist = 99.0;
        }
    }
}

fn parse_inventory(v: &Value, dst: &mut LiveGameState) {
    if let Some(armor) = v.get("armor").and_then(|x| x.as_array()) {
        for (idx, slot) in armor.iter().take(4).enumerate() {
            if let Some(item) = slot["item"].as_str() {
                let dmg = slot["damage"].as_i64().unwrap_or(0) as i32;
                dst.armor[idx] = Some((short_item_name(item), dmg));
            } else {
                dst.armor[idx] = None;
            }
        }
    }
    dst.offhand_item = v["offhand"]["item"].as_str().map(str::to_string);
    dst.first_totem_slot = None;

    if let Some(hb) = v.get("hotbar").and_then(|x| x.as_array()) {
        for (idx, slot) in hb.iter().take(9).enumerate() {
            let item = slot["item"].as_str().map(str::to_string);
            if dst.first_totem_slot.is_none()
                && item.as_deref() == Some("minecraft:totem_of_undying")
            {
                dst.first_totem_slot = slot["slot"].as_u64().map(|s| s as usize);
            }
            dst.hotbar_items[idx] = item;
        }
    }
    if dst.first_totem_slot.is_none()
        && let Some(main) = v.get("main").and_then(|x| x.as_array())
    {
        for slot in main {
            if slot["item"].as_str() == Some("minecraft:totem_of_undying") {
                dst.first_totem_slot = slot["slot"].as_u64().map(|s| s as usize);
                break;
            }
        }
    }

    if let Some(counts) = v.get("counts").and_then(|x| x.as_object()) {
        dst.totem_count = counts
            .get("minecraft:totem_of_undying")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        let arrows = counts
            .get("minecraft:arrow")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        let spec_arrows = counts
            .get("minecraft:spectral_arrow")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        let tipped_arrows = counts
            .get("minecraft:tipped_arrow")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        dst.arrow_count = arrows + spec_arrows + tipped_arrows;
    }
}

fn parse_entities(v: &Value, dst: &mut LiveGameState) {
    dst.nearby_entities.clear();
    if let Some(list) = v.get("entities").and_then(|x| x.as_array()) {
        for e in list.iter().take(64) {
            let kind = e["kind"].as_str().unwrap_or("mob").to_string();
            let ty = e["type"].as_str().unwrap_or("").to_string();
            let dist = e["distance"].as_f64().unwrap_or(0.0);
            let health = e.get("health").and_then(|x| x.as_f64()).map(|h| h as f32);
            if let Some(arr) = e.get("pos").and_then(|x| x.as_array())
                && arr.len() >= 3
            {
                dst.nearby_entities.push(RadarEntity {
                    kind,
                    label: ty,
                    pos: [
                        arr[0].as_f64().unwrap_or(0.0),
                        arr[1].as_f64().unwrap_or(0.0),
                        arr[2].as_f64().unwrap_or(0.0),
                    ],
                    distance: dist,
                    health,
                });
            }
        }
    }
}

fn bridge_loop(rx: mpsc::Receiver<ControlCmd>) {
    let mut conn: Option<(BufReader<UnixStream>, UnixStream)> = None;
    let mut last_inv = Instant::now() - Duration::from_secs(5);
    let mut last_ent = Instant::now() - Duration::from_secs(5);
    let mut last_opts = Instant::now() - Duration::from_secs(10);
    let mut last_ores = Instant::now() - Duration::from_secs(5);
    let mut ore_scan_idx: usize = 0;
    let mut ore_cache: std::collections::HashMap<String, Vec<OreMarker>> =
        std::collections::HashMap::new();
    let mut last_chests = Instant::now() - Duration::from_secs(5);
    let mut chest_scan_idx: usize = 0;
    let mut chest_cache: std::collections::HashMap<String, Vec<OreMarker>> =
        std::collections::HashMap::new();
    let mut last_dim = String::new();
    let mut saved_fov: Option<u32> = None;
    let mut zoomed_active = false;

    // Survival & combat helper state
    let mut sprint_held_by_bot = false;
    let mut last_respawn = Instant::now() - Duration::from_secs(5);
    let mut last_eat = Instant::now() - Duration::from_secs(5);
    let mut last_totem = Instant::now() - Duration::from_secs(5);
    let mut last_trigger = Instant::now() - Duration::from_secs(5);
    let mut last_clicker = Instant::now() - Duration::from_secs(5);
    let mut last_move_pos: Option<[f64; 3]> = None;
    let mut last_move_at = Instant::now();
    let mut last_afk_pulse = Instant::now();
    let mut last_bobber_y: Option<f64> = None;
    let mut bobber_seen_at: Option<Instant> = None;
    let mut last_fish_reel = Instant::now() - Duration::from_secs(5);

    loop {
        if conn.is_none() {
            conn = connect_socket();
            if conn.is_none() {
                {
                    let mut st = STATE.lock();
                    st.game.socket_connected = false;
                    st.game.in_world = false;
                }
                while rx.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            }
        }

        let (reader, writer) = conn.as_mut().unwrap();

        let mut disconnected = false;
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                ControlCmd::SetZoom(active, target_fov) => {
                    if active && !zoomed_active {
                        if let Some(res) = send_line(reader, writer, "options-get fov") {
                            if let Some(cur) = res["options"]["fov"]["value"].as_u64() {
                                saved_fov = Some(cur as u32);
                            }
                        } else {
                            disconnected = true;
                            break;
                        }
                        if send_line(reader, writer, &format!("options-set fov {target_fov}"))
                            .is_none()
                        {
                            disconnected = true;
                            break;
                        }
                        zoomed_active = true;
                    } else if !active && zoomed_active {
                        let restore = saved_fov.unwrap_or(85);
                        if send_line(reader, writer, &format!("options-set fov {restore}")).is_none()
                        {
                            disconnected = true;
                            break;
                        }
                        zoomed_active = false;
                    }
                }
                ControlCmd::SetGamma(g) => {
                    let clamped = g.clamp(0.0, 1.0);
                    if send_line(reader, writer, &format!("options-set gamma {clamped:.2}")).is_none()
                    {
                        disconnected = true;
                        break;
                    }
                    let mut st = STATE.lock();
                    st.game.horizon_gamma = Some(clamped);
                }
            }
        }

        if disconnected {
            conn = None;
            continue;
        }

        let (mut live_snapshot, cfg, key_w, lmb_down, menu_open) = {
            let st = STATE.lock();
            (
                st.game.clone(),
                st.config.clone(),
                st.key_w,
                st.lmb_down,
                st.menu_open || st.hud_edit_mode,
            )
        };

        if let Some(v) = send_line(reader, writer, "state") {
            parse_state(&v, &mut live_snapshot);
        } else {
            conn = None;
            continue;
        }

        if live_snapshot.in_world && last_inv.elapsed() >= Duration::from_millis(400) {
            last_inv = Instant::now();
            if let Some(v) = send_line(reader, writer, "inventory") {
                if v.get("error").is_none() {
                    parse_inventory(&v, &mut live_snapshot);
                }
            } else {
                conn = None;
                continue;
            }
        }

        let need_entities = cfg.hud_radar.enabled
            || cfg.auto_fish
            || cfg.esp_enabled
            || cfg.tracers_enabled
            || cfg.aim_assist;
        let ent_interval = if cfg.aim_assist {
            Duration::from_millis(85)
        } else {
            Duration::from_millis(180)
        };
        if live_snapshot.in_world && need_entities && last_ent.elapsed() >= ent_interval {
            last_ent = Instant::now();
            if let Some(v) = send_line(reader, writer, "entities 48") {
                if v.get("error").is_none() {
                    parse_entities(&v, &mut live_snapshot);
                }
            } else {
                conn = None;
                continue;
            }
        }

        if live_snapshot.dimension != last_dim {
            last_dim = live_snapshot.dimension.clone();
            ore_cache.clear();
            chest_cache.clear();
            live_snapshot.ore_markers.clear();
            live_snapshot.chest_markers.clear();
        }

        if live_snapshot.in_world && cfg.xray_ores {
            if last_ores.elapsed() >= Duration::from_millis(140) {
                last_ores = Instant::now();
                let targets = ore_targets_for(&live_snapshot.dimension, cfg.xray_ore_preset);
                let rad = cfg.xray_radius.clamp(12, 48);
                let mut lost_conn = false;
                let batch = if live_snapshot.ore_markers.is_empty() { 4 } else { 2 };
                for _ in 0..batch {
                    if targets.is_empty() {
                        break;
                    }
                    let (block_id, kind) = targets[ore_scan_idx % targets.len()];
                    ore_scan_idx = ore_scan_idx.wrapping_add(1);
                    if let Some(v) = send_line(reader, writer, &format!("find-block {block_id} {rad}")) {
                        let mut list = Vec::new();
                        if let Some(arr) = v.get("nearest").and_then(|x| x.as_array()) {
                            for item in arr {
                                if let Some(p) = item.get("pos").and_then(|x| x.as_array())
                                    && p.len() >= 3
                                {
                                    let pos = [
                                        p[0].as_i64().unwrap_or(0) as i32,
                                        p[1].as_i64().unwrap_or(0) as i32,
                                        p[2].as_i64().unwrap_or(0) as i32,
                                    ];
                                    let d = dist3(
                                        live_snapshot.eye,
                                        [pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5],
                                    );
                                    list.push(OreMarker { pos, kind, distance: d });
                                }
                            }
                        }
                        ore_cache.insert(block_id.to_string(), list);
                    } else {
                        lost_conn = true;
                        break;
                    }
                }
                if lost_conn {
                    conn = None;
                    continue;
                }
                let max_d = f64::from(rad) + 6.0;
                let mut merged: Vec<OreMarker> = ore_cache
                    .iter()
                    .filter(|(k, _)| targets.iter().any(|(bid, _)| bid == &k.as_str()))
                    .flat_map(|(_, v)| v.iter().copied())
                    .filter(|m| m.distance <= max_d)
                    .collect();
                merged.sort_by(|a, b| a.distance.total_cmp(&b.distance));
                merged.truncate(64);
                live_snapshot.ore_markers = merged;
            }
        } else if !live_snapshot.ore_markers.is_empty() {
            live_snapshot.ore_markers.clear();
            ore_cache.clear();
        }

        if live_snapshot.in_world && cfg.esp_enabled && cfg.esp_chests {
            if last_chests.elapsed() >= Duration::from_millis(240) {
                last_chests = Instant::now();
                let targets = storage_targets();
                let rad = 32;
                let mut lost_conn = false;
                for _ in 0..2 {
                    let (block_id, kind) = targets[chest_scan_idx % targets.len()];
                    chest_scan_idx = chest_scan_idx.wrapping_add(1);
                    if let Some(v) = send_line(reader, writer, &format!("find-block {block_id} {rad}")) {
                        let mut list = Vec::new();
                        if let Some(arr) = v.get("nearest").and_then(|x| x.as_array()) {
                            for item in arr {
                                if let Some(p) = item.get("pos").and_then(|x| x.as_array())
                                    && p.len() >= 3
                                {
                                    let pos = [
                                        p[0].as_i64().unwrap_or(0) as i32,
                                        p[1].as_i64().unwrap_or(0) as i32,
                                        p[2].as_i64().unwrap_or(0) as i32,
                                    ];
                                    let d = dist3(
                                        live_snapshot.eye,
                                        [pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5],
                                    );
                                    list.push(OreMarker { pos, kind, distance: d });
                                }
                            }
                        }
                        chest_cache.insert(block_id.to_string(), list);
                    } else {
                        lost_conn = true;
                        break;
                    }
                }
                if lost_conn {
                    conn = None;
                    continue;
                }
                let mut merged: Vec<OreMarker> = chest_cache
                    .values()
                    .flat_map(|v| v.iter().copied())
                    .filter(|m| m.distance <= 38.0)
                    .collect();
                merged.sort_by(|a, b| a.distance.total_cmp(&b.distance));
                merged.truncate(48);
                live_snapshot.chest_markers = merged;
            }
        } else if !live_snapshot.chest_markers.is_empty() {
            live_snapshot.chest_markers.clear();
            chest_cache.clear();
        }

        if last_opts.elapsed() >= Duration::from_secs(3) {
            last_opts = Instant::now();
            if let Some(v) = send_line(reader, writer, "options-get gamma fov") {
                if let Some(g) = v["options"]["gamma"]["value"].as_f64() {
                    live_snapshot.horizon_gamma = Some(g as f32);
                }
                if !zoomed_active
                    && let Some(f) = v["options"]["fov"]["value"].as_u64()
                {
                    live_snapshot.base_fov = Some(f as u32);
                }
            }
        }

        // ---------------------------------------------------------
        // Survival & Combat Helpers
        // ---------------------------------------------------------
        let playing_now = live_snapshot.in_world && live_snapshot.screen == "playing" && !menu_open;

        // 1. AutoRespawn
        if cfg.auto_respawn
            && live_snapshot.screen == "dead"
            && last_respawn.elapsed() >= Duration::from_secs(1)
        {
            last_respawn = Instant::now();
            let _ = send_line(reader, writer, "respawn");
        }

        // 2. AutoSprint
        if cfg.auto_sprint && playing_now && key_w && live_snapshot.food > 6 {
            if !live_snapshot.sprinting && !sprint_held_by_bot {
                let _ = send_line(reader, writer, "key ctrl down");
                sprint_held_by_bot = true;
            }
        } else if sprint_held_by_bot {
            let _ = send_line(reader, writer, "key ctrl up");
            sprint_held_by_bot = false;
        }

        // 3. AutoTotem
        if cfg.auto_totem
            && playing_now
            && live_snapshot.offhand_item.as_deref() != Some("minecraft:totem_of_undying")
            && let Some(slot) = live_snapshot.first_totem_slot
            && last_totem.elapsed() >= Duration::from_millis(1500)
        {
            last_totem = Instant::now();
            let _ = send_line(reader, writer, "open-inventory");
            let _ = send_line(reader, writer, &format!("click-slot {slot}"));
            let _ = send_line(reader, writer, "click-slot 504");
            let _ = send_line(reader, writer, &format!("click-slot {slot}"));
            let _ = send_line(reader, writer, "close");
        }

        // 4. AutoEat
        if cfg.auto_eat
            && playing_now
            && live_snapshot.food <= cfg.auto_eat_hunger
            && last_eat.elapsed() >= Duration::from_millis(2200)
        {
            if let Some((slot_idx, _)) = live_snapshot
                .hotbar_items
                .iter()
                .enumerate()
                .find(|(_, it)| it.as_deref().is_some_and(is_edible_food))
            {
                last_eat = Instant::now();
                let _ = send_line(reader, writer, &format!("select {}", slot_idx + 1));
                let _ = send_line(reader, writer, "use-item --hold 1.7");
            }
        }

        // 5. AutoFish
        if cfg.auto_fish
            && playing_now
            && live_snapshot.held_raw.as_deref() == Some("minecraft:fishing_rod")
        {
            let bobber = live_snapshot
                .nearby_entities
                .iter()
                .find(|e| e.label.contains("fishing_bobber"));
            if let Some(b) = bobber {
                let now = Instant::now();
                let settled = bobber_seen_at.get_or_insert(now).elapsed() >= Duration::from_millis(1200);
                if let Some(prev_y) = last_bobber_y
                    && settled
                    && (prev_y - b.pos[1]) > 0.14
                    && last_fish_reel.elapsed() >= Duration::from_millis(1400)
                {
                    last_fish_reel = now;
                    let _ = send_line(reader, writer, "mouse right click");
                    std::thread::sleep(Duration::from_millis(450));
                    let _ = send_line(reader, writer, "mouse right click");
                    last_bobber_y = None;
                    bobber_seen_at = None;
                } else {
                    last_bobber_y = Some(b.pos[1]);
                }
            } else {
                last_bobber_y = None;
                bobber_seen_at = None;
            }
        } else {
            last_bobber_y = None;
            bobber_seen_at = None;
        }

        // 6. TriggerBot
        if cfg.trigger_bot
            && playing_now
            && live_snapshot.looking_at_kind == "entity"
            && live_snapshot.looking_at_dist <= 3.2
            && last_trigger.elapsed() >= Duration::from_millis(cfg.trigger_bot_delay_ms as u64)
        {
            last_trigger = Instant::now();
            let _ = send_line(reader, writer, "mouse left click");
        }

        // 7. AutoClicker
        if cfg.auto_clicker && playing_now && lmb_down {
            let interval_ms = (1000 / cfg.auto_clicker_cps.clamp(4, 20)) as u64;
            if last_clicker.elapsed() >= Duration::from_millis(interval_ms) {
                last_clicker = Instant::now();
                let _ = send_line(reader, writer, "mouse left click");
            }
        }

        // 8. AntiAFK
        if cfg.anti_afk && playing_now {
            let moved = match last_move_pos {
                Some(prev) => {
                    (prev[0] - live_snapshot.pos[0]).abs() > 0.05
                        || (prev[2] - live_snapshot.pos[2]).abs() > 0.05
                }
                None => true,
            };
            if moved {
                last_move_pos = Some(live_snapshot.pos);
                last_move_at = Instant::now();
            } else if last_move_at.elapsed() >= Duration::from_secs(30)
                && last_afk_pulse.elapsed() >= Duration::from_secs(25)
            {
                last_afk_pulse = Instant::now();
                let _ = send_line(reader, writer, "key shift tap");
            }
        }

        // 9. AimAssist
        if cfg.aim_assist && playing_now && !cfg.freecam_enabled {
            let mut best: Option<(f32, f32, f32)> = None;
            let eye = live_snapshot.eye;
            for e in &live_snapshot.nearby_entities {
                if e.kind != "mob" || e.distance > f64::from(cfg.aim_range) || e.distance < 0.35 {
                    continue;
                }
                let is_player = e.label == "minecraft:player";
                let is_hostile = is_hostile_mob_id(&e.label);
                let allowed = match cfg.aim_target {
                    crate::config::AimTarget::Players => is_player,
                    crate::config::AimTarget::Hostile => !is_player && is_hostile,
                    crate::config::AimTarget::Both => is_player || is_hostile,
                };
                if !allowed {
                    continue;
                }
                let dx = e.pos[0] - eye[0];
                let dy = (e.pos[1] + 1.05) - eye[1];
                let dz = e.pos[2] - eye[2];
                let horiz = (dx * dx + dz * dz).sqrt().max(0.001);
                let target_yaw = (-dx).atan2(dz).to_degrees() as f32;
                let target_pitch = (-dy).atan2(horiz).to_degrees() as f32;
                let mut dyaw = (target_yaw - live_snapshot.yaw) % 360.0;
                if dyaw > 180.0 {
                    dyaw -= 360.0;
                }
                if dyaw < -180.0 {
                    dyaw += 360.0;
                }
                let dpitch = target_pitch - live_snapshot.pitch;
                let ang = (dyaw * dyaw + dpitch * dpitch).sqrt();
                if ang <= cfg.aim_fov && ang > 1.2 && best.is_none_or(|b| ang < b.0) {
                    best = Some((ang, dyaw, dpitch));
                }
            }
            if let Some((_, dyaw, dpitch)) = best {
                let s = cfg.aim_smooth.clamp(0.12, 0.90);
                let sy = (dyaw * s).clamp(-22.0, 22.0);
                let sp = (dpitch * s).clamp(-16.0, 16.0);
                let _ = send_line(reader, writer, &format!("turn {sy:.2} {sp:.2} --over 0.05"));
            }
        }

        {
            let mut st = STATE.lock();
            st.game = live_snapshot;
        }

        std::thread::sleep(Duration::from_millis(130));
    }
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn storage_targets() -> &'static [(&'static str, OreKind)] {
    &[
        ("minecraft:chest", OreKind::Chest),
        ("minecraft:trapped_chest", OreKind::Chest),
        ("minecraft:ender_chest", OreKind::Chest),
        ("minecraft:barrel", OreKind::Chest),
        ("minecraft:spawner", OreKind::Spawner),
    ]
}

fn ore_targets_for(
    dimension: &str,
    preset: crate::config::OrePreset,
) -> &'static [(&'static str, OreKind)] {
    if dimension.contains("nether") {
        match preset {
            crate::config::OrePreset::Valuable => &[
                ("minecraft:ancient_debris", OreKind::Debris),
                ("minecraft:nether_gold_ore", OreKind::Gold),
            ],
            crate::config::OrePreset::All => &[
                ("minecraft:ancient_debris", OreKind::Debris),
                ("minecraft:nether_gold_ore", OreKind::Gold),
                ("minecraft:nether_quartz_ore", OreKind::Quartz),
            ],
        }
    } else {
        match preset {
            crate::config::OrePreset::Valuable => &[
                ("minecraft:diamond_ore", OreKind::Diamond),
                ("minecraft:deepslate_diamond_ore", OreKind::Diamond),
                ("minecraft:emerald_ore", OreKind::Emerald),
                ("minecraft:deepslate_emerald_ore", OreKind::Emerald),
                ("minecraft:gold_ore", OreKind::Gold),
                ("minecraft:deepslate_gold_ore", OreKind::Gold),
            ],
            crate::config::OrePreset::All => &[
                ("minecraft:diamond_ore", OreKind::Diamond),
                ("minecraft:deepslate_diamond_ore", OreKind::Diamond),
                ("minecraft:emerald_ore", OreKind::Emerald),
                ("minecraft:deepslate_emerald_ore", OreKind::Emerald),
                ("minecraft:gold_ore", OreKind::Gold),
                ("minecraft:deepslate_gold_ore", OreKind::Gold),
                ("minecraft:iron_ore", OreKind::Iron),
                ("minecraft:deepslate_iron_ore", OreKind::Iron),
                ("minecraft:lapis_ore", OreKind::Lapis),
                ("minecraft:deepslate_lapis_ore", OreKind::Lapis),
                ("minecraft:redstone_ore", OreKind::Redstone),
                ("minecraft:deepslate_redstone_ore", OreKind::Redstone),
                ("minecraft:copper_ore", OreKind::Copper),
                ("minecraft:deepslate_copper_ore", OreKind::Copper),
                ("minecraft:coal_ore", OreKind::Coal),
                ("minecraft:deepslate_coal_ore", OreKind::Coal),
            ],
        }
    }
}

pub fn is_hostile_mob_id(id: &str) -> bool {
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    matches!(
        name,
        "zombie"
            | "zombie_villager"
            | "husk"
            | "drowned"
            | "skeleton"
            | "stray"
            | "bogged"
            | "wither_skeleton"
            | "creeper"
            | "spider"
            | "cave_spider"
            | "enderman"
            | "endermite"
            | "silverfish"
            | "witch"
            | "slime"
            | "magma_cube"
            | "blaze"
            | "ghast"
            | "phantom"
            | "pillager"
            | "vindicator"
            | "evoker"
            | "ravager"
            | "vex"
            | "warden"
            | "guardian"
            | "elder_guardian"
            | "shulker"
            | "piglin_brute"
            | "hoglin"
            | "zoglin"
            | "zombified_piglin"
            | "piglin"
            | "breeze"
            | "creaking"
            | "ender_dragon"
            | "wither"
    )
}
