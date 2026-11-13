use crate::config::{CrosshairStyle, HudWidget};
use crate::control_bridge::{is_hostile_mob_id, OreKind};
use crate::hooks_vk::latest_view_proj;
use crate::draw::{palette, Color, DrawList, Rect};
use crate::font::{FontAtlas, FontKind};
use crate::state::{AppState, DragWidget};

const WAYPOINT_COLORS: [Color; 5] = [
    Color::hex(0xE5A93B, 1.0), // Amber
    Color::hex(0x2EC27E, 1.0), // Emerald
    Color::hex(0x52A8FF, 1.0), // Sky
    Color::hex(0xE05A47, 1.0), // Cinnabar
    Color::hex(0xD88CFF, 1.0), // Orchid
];

pub fn waypoint_color(idx: u8) -> Color {
    WAYPOINT_COLORS[(idx as usize) % WAYPOINT_COLORS.len()]
}

fn facing_label(yaw: f32) -> (&'static str, &'static str) {
    // Minecraft yaw: 0 = +Z (South), 90 = -X (West), 180/-180 = -Z (North), -90 = +X (East)
    let norm = ((yaw % 360.0) + 360.0) % 360.0;
    if !(22.5..337.5).contains(&norm) {
        ("S", "South")
    } else if norm < 67.5 {
        ("SW", "South-West")
    } else if norm < 112.5 {
        ("W", "West")
    } else if norm < 157.5 {
        ("NW", "North-West")
    } else if norm < 202.5 {
        ("N", "North")
    } else if norm < 247.5 {
        ("NE", "North-East")
    } else if norm < 292.5 {
        ("E", "East")
    } else {
        ("SE", "South-East")
    }
}

fn format_mc_time(time_of_day: i64) -> String {
    let total_minutes = ((time_of_day + 6000) % 24000) * 1440 / 24000;
    let h = total_minutes / 60;
    let m = total_minutes % 60;
    format!("{h:02}:{m:02}")
}

fn widget_rect(w_norm: &HudWidget, box_w: f32, box_h: f32, sw: f32, sh: f32) -> Rect {
    let x = (w_norm.x * sw).clamp(6.0, (sw - box_w - 6.0).max(6.0));
    let y = (w_norm.y * sh).clamp(6.0, (sh - box_h - 6.0).max(6.0));
    Rect::new(x, y, box_w, box_h)
}

pub fn render_hud(st: &mut AppState, dl: &mut DrawList, atlas: &FontAtlas) {
    let sw = st.screen_size[0].max(1) as f32;
    let sh = st.screen_size[1].max(1) as f32;
    let editing = st.hud_edit_mode;

    // Show HUD only inside an active world (or during HUD layout editing)
    if !st.game.in_world && !editing {
        return;
    }

    // 3D Ore X-Ray ESP boxes (when inside world)
    let (ore_lbl, ore_max, chest_lbl, chest_max) = match st.config.perf_profile {
        crate::config::PerfProfile::Full => (32.0, 48, 36.0, 48),
        crate::config::PerfProfile::Balanced => (28.0, 32, 32.0, 32),
        crate::config::PerfProfile::Lite => (24.0, 20, 28.0, 16),
    };
    if st.config.xray_ores && st.game.in_world {
        draw_block_markers(st, dl, atlas, sw, sh, &st.game.ore_markers, ore_lbl, ore_max);
    }
    if st.config.esp_enabled && st.config.esp_chests && st.game.in_world {
        draw_block_markers(st, dl, atlas, sw, sh, &st.game.chest_markers, chest_lbl, chest_max);
    }
    if (st.config.esp_enabled || st.config.tracers_enabled) && st.game.in_world {
        draw_entity_esp_and_tracers(st, dl, atlas, sw, sh);
    }
    if st.config.freecam_enabled && st.game.in_world {
        let txt = "Freecam Active";
        let tw = atlas.measure(FontKind::UiSemi, txt) + 24.0;
        let br = Rect::new((sw - tw) * 0.5, 68.0, tw, 26.0);
        dl.rect_border(br, 6.0, palette::HUD_BG, 1.0, palette::ACCENT);
        dl.text(atlas, FontKind::UiSemi, br.x + 12.0, br.y + 5.0, txt, palette::ACCENT);
    }


    // Crosshair+ (only while playing with menu closed)
    if st.config.crosshair_enabled && !st.menu_open && st.game.screen == "playing" {
        draw_crosshair(st, dl, sw * 0.5, sh * 0.5);
    }

    let mut active_rects: Vec<(DragWidget, &'static str, Rect)> = Vec::new();

    if st.config.hud_coords.enabled || editing {
        let r = draw_coords_widget(st, dl, atlas, sw, sh, editing);
        active_rects.push((DragWidget::Coords, "Coordinates", r));
    }

    if st.config.hud_fps.enabled || editing {
        let r = draw_fps_widget(st, dl, atlas, sw, sh, editing);
        active_rects.push((DragWidget::Fps, "FPS & Light", r));
    }

    if st.config.hud_keystrokes.enabled || editing {
        let r = draw_keystrokes_widget(st, dl, atlas, sw, sh, editing);
        active_rects.push((DragWidget::Keystrokes, "Keystrokes & CPS", r));
    }

    if st.config.hud_vitals.enabled || editing {
        let r = draw_vitals_widget(st, dl, atlas, sw, sh, editing);
        active_rects.push((DragWidget::Vitals, "Vitals & Gear", r));
    }

    if st.config.hud_compass.enabled || editing {
        let r = draw_compass_widget(st, dl, atlas, sw, sh, editing);
        active_rects.push((DragWidget::Compass, "Waypoints", r));
    }

    if st.config.hud_clock.enabled || editing {
        let r = draw_clock_widget(st, dl, atlas, sw, sh, editing);
        active_rects.push((DragWidget::Clock, "Session Clock", r));
    }
    if st.config.hud_radar.enabled || editing {
        let r = draw_radar_widget(st, dl, atlas, sw, sh, editing);
        active_rects.push((DragWidget::Radar, "Entity Radar", r));
    }


    if editing {
        handle_hud_drag(st, dl, atlas, &active_rects, sw, sh);
    }
}

fn handle_hud_drag(
    st: &mut AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    rects: &[(DragWidget, &'static str, Rect)],
    sw: f32,
    sh: f32,
) {
    let [mx, my] = st.cursor_pos;

    let banner_text = "Drag panels to move · Press Esc or ` to finish";
    let bw = atlas.measure(FontKind::UiSemi, banner_text) + 32.0;
    let br = Rect::new((sw - bw) * 0.5, sh - 54.0, bw, 34.0);
    dl.shadow(br, 8.0, 12.0, palette::SHADOW);
    dl.rect_border(br, 8.0, palette::CANVAS, 1.5, palette::ACCENT);
    dl.text(
        atlas,
        FontKind::UiSemi,
        br.x + 16.0,
        br.y + 8.0,
        banner_text,
        palette::TEXT_PRIMARY,
    );

    if st.mouse_just_pressed {
        for &(kind, _, r) in rects.iter().rev() {
            if r.contains(mx, my) {
                st.dragging_widget = Some((kind, [mx - r.x, my - r.y]));
                break;
            }
        }
    }

    if st.mouse_just_released {
        if st.dragging_widget.is_some() {
            let _ = st.config.save();
        }
        st.dragging_widget = None;
    }

    if let Some((kind, grab_off)) = st.dragging_widget
        && st.mouse_down
    {
        let nx = ((mx - grab_off[0]) / sw).clamp(0.002, 0.96);
        let ny = ((my - grab_off[1]) / sh).clamp(0.002, 0.94);
        let target = match kind {
            DragWidget::Coords => &mut st.config.hud_coords,
            DragWidget::Fps => &mut st.config.hud_fps,
            DragWidget::Keystrokes => &mut st.config.hud_keystrokes,
            DragWidget::Vitals => &mut st.config.hud_vitals,
            DragWidget::Compass => &mut st.config.hud_compass,
            DragWidget::Clock => &mut st.config.hud_clock,
            DragWidget::Radar => &mut st.config.hud_radar,
        };
        target.x = nx;
        target.y = ny;
    }

    for &(kind, label, r) in rects {
        let is_drag = st.dragging_widget.is_some_and(|(k, _)| k == kind);
        let is_hover = r.contains(mx, my);
        let border = if is_drag || is_hover {
            palette::ACCENT
        } else {
            palette::BORDER_STRONG
        };
        dl.rect_border(r, 8.0, Color::hex(0x000000, 0.0), 1.5, border);
        let tw = atlas.measure(FontKind::UiSmall, label) + 12.0;
        let tag = Rect::new(r.x, (r.y - 20.0).max(2.0), tw, 18.0);
        dl.rect(tag, 4.0, palette::ACCENT);
        dl.text(
            atlas,
            FontKind::UiSmall,
            tag.x + 6.0,
            tag.y + 2.0,
            label,
            palette::CANVAS_SOLID,
        );
    }
}

fn draw_crosshair(st: &AppState, dl: &mut DrawList, cx: f32, cy: f32) {
    let size = st.config.crosshair_size;
    let gap = st.config.crosshair_gap;
    let thick = 2.0;
    let col = palette::TEXT_PRIMARY;
    let outline = Color::hex(0x000000, 0.75);

    match st.config.crosshair_style {
        CrosshairStyle::Dot => {
            dl.rect(
                Rect::new(cx - 2.5, cy - 2.5, 5.0, 5.0),
                2.5,
                outline,
            );
            dl.rect(Rect::new(cx - 1.5, cy - 1.5, 3.0, 3.0), 1.5, col);
        }
        CrosshairStyle::Cross => {
            // Left, Right, Top, Bottom
            let segs = [
                Rect::new(cx - gap - size, cy - thick * 0.5, size, thick),
                Rect::new(cx + gap, cy - thick * 0.5, size, thick),
                Rect::new(cx - thick * 0.5, cy - gap - size, thick, size),
                Rect::new(cx - thick * 0.5, cy + gap, thick, size),
            ];
            for s in segs {
                dl.rect(
                    Rect::new(s.x - 1.0, s.y - 1.0, s.w + 2.0, s.h + 2.0),
                    1.0,
                    outline,
                );
                dl.rect(s, 0.5, col);
            }
        }
        CrosshairStyle::TBar => {
            let segs = [
                Rect::new(cx - gap - size, cy - thick * 0.5, size, thick),
                Rect::new(cx + gap, cy - thick * 0.5, size, thick),
                Rect::new(cx - thick * 0.5, cy + gap, thick, size),
            ];
            for s in segs {
                dl.rect(
                    Rect::new(s.x - 1.0, s.y - 1.0, s.w + 2.0, s.h + 2.0),
                    1.0,
                    outline,
                );
                dl.rect(s, 0.5, col);
            }
        }
    }
}

fn draw_coords_widget(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    _editing: bool,
) -> Rect {
    let show_nether = st.config.hud_nether_convert;
    let h = if show_nether { 66.0 } else { 46.0 };
    let w = 236.0;
    let r = widget_rect(&st.config.hud_coords, w, h, sw, sh);
    let alpha = if st.config.hud_coords.enabled { 1.0 } else { 0.45 };

    dl.rect_border(
        r,
        8.0,
        palette::HUD_BG.with_alpha(alpha),
        1.0,
        palette::BORDER.with_alpha(alpha),
    );

    let (line1, line2, sub) = if st.game.in_world {
        let [x, y, z] = st.game.pos;
        let (_, dir_full) = facing_label(st.game.yaw);
        let is_nether = st.game.dimension.contains("nether");
        let is_end = st.game.dimension.contains("end");
        let dim_tag = if is_nether {
            "Nether"
        } else if is_end {
            "The End"
        } else {
            "Overworld"
        };
        let l1 = format!("{:.0}, {:.0}, {:.0}", x.floor(), y.floor(), z.floor());
        let l2 = format!("{dir_full} · {dim_tag}");
        let sub = if is_nether {
            format!("Overworld: {:.0}, {:.0}", x * 8.0, z * 8.0)
        } else if !is_end {
            format!("Nether: {:.0}, {:.0}", x / 8.0, z / 8.0)
        } else {
            String::new()
        };
        (l1, l2, sub)
    } else {
        (
            String::from("128, 64, -240"),
            String::from("North · Overworld"),
            String::from("Nether: 16, -30"),
        )
    };

    dl.text(
        atlas,
        FontKind::MonoBody,
        r.x + 10.0,
        r.y + 7.0,
        &line1,
        palette::TEXT_PRIMARY.with_alpha(alpha),
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        r.x + 10.0,
        r.y + 26.0,
        &line2,
        palette::ACCENT.with_alpha(alpha),
    );
    if show_nether && !sub.is_empty() {
        dl.text(
            atlas,
            FontKind::MonoSmall,
            r.x + 10.0,
            r.y + 44.0,
            &sub,
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );
    }
    r
}

fn draw_fps_widget(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    _editing: bool,
) -> Rect {
    let w = 152.0;
    let h = 48.0;
    let r = widget_rect(&st.config.hud_fps, w, h, sw, sh);
    let alpha = if st.config.hud_fps.enabled { 1.0 } else { 0.45 };

    dl.rect_border(
        r,
        8.0,
        palette::HUD_BG.with_alpha(alpha),
        1.0,
        palette::BORDER.with_alpha(alpha),
    );

    let fps_str = format!("{:.0} FPS", st.fps);
    dl.text(
        atlas,
        FontKind::MonoBody,
        r.x + 10.0,
        r.y + 7.0,
        &fps_str,
        palette::TEXT_PRIMARY.with_alpha(alpha),
    );

    let xray_on = st.config.xray_ores || st.config.xray_terrain || st.config.xray_entities;
    let fb_status = match (st.config.fullbright_enabled, xray_on) {
        (true, true) => "Fullbright · X-Ray",
        (true, false) => "Fullbright",
        (false, true) => "X-Ray",
        (false, false) => "Standard Light",
    };
    let fb_col = if st.config.fullbright_enabled || xray_on {
        palette::SUCCESS
    } else {
        palette::TEXT_SECONDARY
    };
    dl.text(
        atlas,
        FontKind::UiSmall,
        r.x + 10.0,
        r.y + 26.0,
        fb_status,
        fb_col.with_alpha(alpha),
    );
    r
}

fn draw_keystrokes_widget(
    st: &mut AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    _editing: bool,
) -> Rect {
    let w = 136.0;
    let h = 148.0;
    let r = widget_rect(&st.config.hud_keystrokes, w, h, sw, sh);
    let alpha = if st.config.hud_keystrokes.enabled {
        1.0
    } else {
        0.45
    };
    let (l_cps, r_cps) = st.cps();

    let key_size = 40.0;
    let gap = 4.0;
    let x0 = r.x + 4.0;
    let y0 = r.y + 4.0;

    let mut draw_key = |kx: f32, ky: f32, kw: f32, kh: f32, label: &str, sub: Option<&str>, active: bool| {
        let bg = if active {
            palette::ACCENT
        } else {
            palette::HUD_BG
        };
        let fg = if active {
            palette::CANVAS_SOLID
        } else {
            palette::TEXT_PRIMARY
        };
        dl.rect_border(
            Rect::new(kx, ky, kw, kh),
            6.0,
            bg.with_alpha(alpha),
            1.0,
            palette::BORDER.with_alpha(alpha),
        );
        let tw = atlas.measure(FontKind::UiSemi, label);
        let ty = if sub.is_some() { ky + 4.0 } else { ky + (kh - 16.0) * 0.5 };
        dl.text(
            atlas,
            FontKind::UiSemi,
            kx + (kw - tw) * 0.5,
            ty,
            label,
            fg.with_alpha(alpha),
        );
        if let Some(sub_txt) = sub {
            let sw2 = atlas.measure(FontKind::MonoSmall, sub_txt);
            let scol = if active {
                palette::CANVAS_SOLID
            } else {
                palette::TEXT_SECONDARY
            };
            dl.text(
                atlas,
                FontKind::MonoSmall,
                kx + (kw - sw2) * 0.5,
                ky + 21.0,
                sub_txt,
                scol.with_alpha(alpha),
            );
        }
    };

    draw_key(x0 + key_size + gap, y0, key_size, key_size, "W", None, st.key_w);
    draw_key(x0, y0 + key_size + gap, key_size, key_size, "A", None, st.key_a);
    draw_key(
        x0 + key_size + gap,
        y0 + key_size + gap,
        key_size,
        key_size,
        "S",
        None,
        st.key_s,
    );
    draw_key(
        x0 + (key_size + gap) * 2.0,
        y0 + key_size + gap,
        key_size,
        key_size,
        "D",
        None,
        st.key_d,
    );

    // LMB / RMB row
    let mw = (w - 8.0 - gap) * 0.5;
    let my = y0 + (key_size + gap) * 2.0;
    let l_sub = format!("{l_cps} CPS");
    let r_sub = format!("{r_cps} CPS");
    draw_key(x0, my, mw, 36.0, "LMB", Some(&l_sub), st.lmb_down);
    draw_key(x0 + mw + gap, my, mw, 36.0, "RMB", Some(&r_sub), st.rmb_down);

    // Spacebar
    let sy = my + 36.0 + gap;
    draw_key(x0, sy, w - 8.0, 22.0, "SPACE", None, st.key_space);

    r
}

fn draw_vitals_widget(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    _editing: bool,
) -> Rect {
    let w = 212.0;
    let eff_count = st.game.effects.len().min(3);
    let h = 88.0 + if eff_count > 0 { 20.0 } else { 0.0 };
    let r = widget_rect(&st.config.hud_vitals, w, h, sw, sh);
    let alpha = if st.config.hud_vitals.enabled { 1.0 } else { 0.45 };

    dl.rect_border(
        r,
        8.0,
        palette::HUD_BG.with_alpha(alpha),
        1.0,
        palette::BORDER.with_alpha(alpha),
    );

    if st.game.in_world {
        let hp_col = if st.game.health <= 7.0 {
            palette::DANGER
        } else {
            palette::SUCCESS
        };
        let hp_txt = if st.game.absorption > 0.0 {
            format!("Health {:.1} (+{:.0})", st.game.health, st.game.absorption)
        } else {
            format!("Health {:.1} / 20", st.game.health)
        };
        let food_txt = format!("Food {}/20 · Lv {}", st.game.food, st.game.xp_level);
        dl.text(
            atlas,
            FontKind::MonoBody,
            r.x + 10.0,
            r.y + 8.0,
            &hp_txt,
            hp_col.with_alpha(alpha),
        );
        dl.text(
            atlas,
            FontKind::MonoSmall,
            r.x + 10.0,
            r.y + 28.0,
            &food_txt,
            palette::TEXT_PRIMARY.with_alpha(alpha),
        );

        let inv_txt = format!(
            "Totems {} · Arrows {}",
            st.game.totem_count, st.game.arrow_count
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 10.0,
            r.y + 47.0,
            &inv_txt,
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );

        let target_line = if let Some(lbl) = &st.game.looking_at_label {
            if let Some(thp) = st.game.looking_at_hp {
                format!("{lbl} · {thp:.1} HP")
            } else {
                lbl.clone()
            }
        } else if let Some(held) = &st.game.held_item {
            held.clone()
        } else {
            String::from("Empty Hand")
        };
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 10.0,
            r.y + 65.0,
            &target_line,
            palette::ACCENT.with_alpha(alpha),
        );

        if let Some((eid, secs, amp)) = st.game.effects.first() {
            let clean = eid.strip_prefix("minecraft:").unwrap_or(eid).replace('_', " ");
            let eff_txt = format!("{clean} {} ({}s)", amp + 1, secs);
            dl.text(
                atlas,
                FontKind::MonoSmall,
                r.x + 10.0,
                r.y + 84.0,
                &eff_txt,
                palette::TEXT_SECONDARY.with_alpha(alpha),
            );
        }
    } else {
        dl.text(
            atlas,
            FontKind::MonoBody,
            r.x + 10.0,
            r.y + 8.0,
            "Health 20.0 / 20",
            palette::SUCCESS.with_alpha(alpha),
        );
        dl.text(
            atlas,
            FontKind::MonoSmall,
            r.x + 10.0,
            r.y + 28.0,
            "Food 20/20 · Lv 30",
            palette::TEXT_PRIMARY.with_alpha(alpha),
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 10.0,
            r.y + 47.0,
            "Totems 1 · Arrows 64",
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 10.0,
            r.y + 65.0,
            "Diamond Sword",
            palette::ACCENT.with_alpha(alpha),
        );
    }
    r
}

fn draw_compass_widget(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    editing: bool,
) -> Rect {
    let w = 340.0;
    let h = 42.0;
    let r = widget_rect(&st.config.hud_compass, w, h, sw, sh);
    let alpha = if st.config.hud_compass.enabled {
        1.0
    } else {
        0.45
    };

    dl.rect_border(
        r,
        8.0,
        palette::HUD_BG.with_alpha(alpha),
        1.0,
        palette::BORDER.with_alpha(alpha),
    );

    let px = st.game.pos[0];
    let pz = st.game.pos[2];
    let player_yaw = st.game.yaw;
    let cur_dim = if st.game.in_world {
        st.game.dimension.as_str()
    } else {
        "minecraft:overworld"
    };

    // Draw cardinal center indicator
    let cx = r.x + w * 0.5;
    dl.rect(
        Rect::new(cx - 1.0, r.y + 3.0, 2.0, 6.0),
        1.0,
        palette::ACCENT.with_alpha(alpha),
    );

    let (dir, _) = facing_label(player_yaw);
    dl.text(
        atlas,
        FontKind::UiSemi,
        r.x + 10.0,
        r.y + 11.0,
        dir,
        palette::ACCENT.with_alpha(alpha),
    );

    // Find active waypoints in current dimension (or preview in edit mode)
    let active: Vec<_> = st
        .config
        .waypoints
        .iter()
        .filter(|wp| wp.enabled && wp.dimension == cur_dim)
        .collect();

    if active.is_empty() {
        let msg = if editing || !st.game.in_world {
            "Waypoints"
        } else {
            "No waypoints"
        };
        let tw = atlas.measure(FontKind::UiSmall, msg);
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + (w - tw) * 0.5 + 8.0,
            r.y + 13.0,
            msg,
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );
        return r;
    }

    dl.push_scissor(Rect::new(r.x + 34.0, r.y + 2.0, w - 42.0, h - 4.0));
    let usable_half = (w - 60.0) * 0.5;
    for wp in active {
        let dx = wp.x as f64 + 0.5 - px;
        let dz = wp.z as f64 + 0.5 - pz;
        let dist = (dx * dx + dz * dz).sqrt();
        // Minecraft yaw: 0 = +Z, -90 = +X => atan2(-dx, dz) in degrees
        let target_yaw = (-dx).atan2(dz).to_degrees() as f32;
        let mut diff = (target_yaw - player_yaw) % 360.0;
        if diff > 180.0 {
            diff -= 360.0;
        }
        if diff < -180.0 {
            diff += 360.0;
        }
        let norm = (diff / 90.0).clamp(-1.05, 1.05);
        let wx = cx + norm * usable_half;
        let marker_col = waypoint_color(wp.color_idx).with_alpha(alpha);
        let label = format!("{} {:.0}m", wp.name, dist);
        let tw = atlas.measure(FontKind::MonoSmall, &label);
        dl.rect(Rect::new(wx - 3.0, r.y + 8.0, 6.0, 6.0), 3.0, marker_col);
        dl.text(
            atlas,
            FontKind::MonoSmall,
            wx - tw * 0.5,
            r.y + 20.0,
            &label,
            palette::TEXT_PRIMARY.with_alpha(alpha),
        );
    }
    dl.pop_scissor();
    r
}

fn draw_clock_widget(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    _editing: bool,
) -> Rect {
    let w = 212.0;
    let h = 44.0;
    let r = widget_rect(&st.config.hud_clock, w, h, sw, sh);
    let alpha = if st.config.hud_clock.enabled { 1.0 } else { 0.45 };

    dl.rect_border(
        r,
        8.0,
        palette::HUD_BG.with_alpha(alpha),
        1.0,
        palette::BORDER.with_alpha(alpha),
    );

    let secs = st.session_start.elapsed().as_secs();
    let sess = format!("Session {:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60);
    let world_t = if st.game.in_world {
        let weather = if st.game.thundering {
            "Thunder"
        } else if st.game.raining {
            "Rain"
        } else {
            "Clear"
        };
        format!(
            "Day {} · {} · {weather}",
            st.game.day,
            format_mc_time(st.game.time_of_day)
        )
    } else {
        String::from("Day 1 · 12:00 · Clear")
    };

    dl.text(
        atlas,
        FontKind::MonoSmall,
        r.x + 10.0,
        r.y + 7.0,
        &world_t,
        palette::TEXT_PRIMARY.with_alpha(alpha),
    );
    dl.text(
        atlas,
        FontKind::MonoSmall,
        r.x + 10.0,
        r.y + 24.0,
        &sess,
        palette::TEXT_SECONDARY.with_alpha(alpha),
    );
    r
}


fn draw_radar_widget(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    _editing: bool,
) -> Rect {
    let w = 172.0;
    let size = 148.0;
    let total_h = size + 38.0;
    let r = widget_rect(&st.config.hud_radar, w, total_h, sw, sh);
    let alpha = if st.config.hud_radar.enabled { 1.0 } else { 0.45 };

    dl.rect_border(
        r,
        8.0,
        palette::HUD_BG.with_alpha(alpha),
        1.0,
        palette::BORDER.with_alpha(alpha),
    );

    let map_r = Rect::new(r.x + 12.0, r.y + 22.0, size - 12.0, size - 12.0);
    let cx = map_r.x + map_r.w * 0.5;
    let cy = map_r.y + map_r.h * 0.5;
    let half = map_r.w * 0.5 - 4.0;

    // Crosshair lines inside radar
    dl.rect(
        Rect::new(cx - 0.5, map_r.y + 2.0, 1.0, map_r.h - 4.0),
        0.0,
        palette::BORDER.with_alpha(0.35 * alpha),
    );
    dl.rect(
        Rect::new(map_r.x + 2.0, cy - 0.5, map_r.w - 4.0, 1.0),
        0.0,
        palette::BORDER.with_alpha(0.35 * alpha),
    );

    if st.game.in_world {
        let title = format!("Radar ({})", st.game.nearby_entities.len());
        dl.text(
            atlas,
            FontKind::MonoSmall,
            r.x + 8.0,
            r.y + 5.0,
            &title,
            palette::TEXT_PRIMARY.with_alpha(alpha),
        );

        let yaw_rad = st.game.yaw.to_radians();
        let cos_y = yaw_rad.cos();
        let sin_y = yaw_rad.sin();
        let max_range = 48.0f32;

        dl.push_scissor(map_r);
        for e in &st.game.nearby_entities {
            let dx = (e.pos[0] - st.game.pos[0]) as f32;
            let dz = (e.pos[2] - st.game.pos[2]) as f32;
            let rx = (-dx * cos_y - dz * sin_y) / max_range * half;
            let ry = (dx * sin_y - dz * cos_y) / max_range * half;
            let px = (cx + rx).clamp(map_r.x + 3.0, map_r.x + map_r.w - 3.0);
            let py = (cy + ry).clamp(map_r.y + 3.0, map_r.y + map_r.h - 3.0);

            let dot_col = if e.label == "minecraft:player" {
                Color::hex(0xF5C542, 1.0) // Yellow: other players
            } else if e.kind == "item" {
                Color::hex(0x9FB1C8, 1.0) // Muted steel: dropped items
            } else if e.kind == "orb" || !is_hostile_mob_id(&e.label) {
                palette::SUCCESS // Green: non-hostile mobs & XP
            } else {
                palette::DANGER // Red: hostile mobs
            };
            let dot_sz = if e.label == "minecraft:player" { 6.0 } else { 5.0 };
            dl.rect(
                Rect::new(px - dot_sz * 0.5, py - dot_sz * 0.5, dot_sz, dot_sz),
                dot_sz * 0.5,
                dot_col.with_alpha(alpha),
            );
        }
        dl.pop_scissor();
    } else {
        let lbl = "Radar";
        dl.text(
            atlas,
            FontKind::MonoSmall,
            r.x + 8.0,
            r.y + 5.0,
            lbl,
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );
    }

    // White center marker = Me (drawn on top)
    dl.rect(
        Rect::new(cx - 3.0, cy - 3.0, 6.0, 6.0),
        3.0,
        Color::hex(0xFFFFFF, alpha),
    );

    // Compact color legend row at bottom
    let ly = r.y + total_h - 17.0;
    let legend = [
        ("You", Color::hex(0xFFFFFF, 1.0)),
        ("Ply", Color::hex(0xF5C542, 1.0)),
        ("Mob", palette::DANGER),
        ("Safe", palette::SUCCESS),
    ];
    let mut lx = r.x + 8.0;
    for (tag, col) in legend {
        dl.rect(Rect::new(lx, ly + 3.0, 6.0, 6.0), 3.0, col.with_alpha(alpha));
        dl.text(
            atlas,
            FontKind::MonoSmall,
            lx + 8.0,
            ly,
            tag,
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );
        lx += 39.0;
    }

    r
}

fn ore_color(kind: OreKind) -> Color {
    match kind {
        OreKind::Diamond => Color::hex(0x4DE3FF, 1.0),
        OreKind::Debris => Color::hex(0xFF8A47, 1.0),
        OreKind::Emerald => Color::hex(0x2EC27E, 1.0),
        OreKind::Gold => Color::hex(0xF5C542, 1.0),
        OreKind::Iron => Color::hex(0xD8C3A5, 1.0),
        OreKind::Lapis => Color::hex(0x4F8BFF, 1.0),
        OreKind::Redstone => Color::hex(0xE05A47, 1.0),
        OreKind::Coal => Color::hex(0xA8B3C4, 1.0),
        OreKind::Copper => Color::hex(0xE08A52, 1.0),
        OreKind::Quartz => Color::hex(0xF2F4F8, 1.0),
        OreKind::Chest => Color::hex(0xE5A93B, 1.0),
        OreKind::Spawner => Color::hex(0xD88CFF, 1.0),
    }
}

fn project_point(m: &[[f32; 4]; 4], rx: f32, ry: f32, rz: f32, sw: f32, sh: f32) -> Option<[f32; 2]> {
    let cx = m[0][0] * rx + m[1][0] * ry + m[2][0] * rz + m[3][0];
    let cy = m[0][1] * rx + m[1][1] * ry + m[2][1] * rz + m[3][1];
    let cw = m[0][3] * rx + m[1][3] * ry + m[2][3] * rz + m[3][3];
    if cw <= 0.05 {
        return None;
    }
    let nx = cx / cw;
    let ny = cy / cw;
    Some([(nx * 0.5 + 0.5) * sw, (0.5 - ny * 0.5) * sh])
}

fn fallback_view_proj(yaw: f32, pitch: f32, fov_deg: f32, sw: f32, sh: f32) -> [[f32; 4]; 4] {
    let deg = std::f32::consts::PI / 180.0;
    let angle_y = std::f32::consts::PI - yaw * deg;
    let angle_x = -pitch * deg;
    let (sy, cy) = (angle_y * 0.5).sin_cos();
    let (sx, cx) = (angle_x * 0.5).sin_cos();
    let qx = -(cy * sx);
    let qy = -(sy * cx);
    let qz = -(sy * sx);
    let qw = cy * cx;

    let x2 = qx + qx;
    let y2 = qy + qy;
    let z2 = qz + qz;
    let xx = qx * x2;
    let xy = qx * y2;
    let xz = qx * z2;
    let yy = qy * y2;
    let yz = qy * z2;
    let zz = qz * z2;
    let wx = qw * x2;
    let wy = qw * y2;
    let wz = qw * z2;

    let view = [
        [1.0 - (yy + zz), xy + wz, xz - wy, 0.0],
        [xy - wz, 1.0 - (xx + zz), yz + wx, 0.0],
        [xz + wy, yz - wx, 1.0 - (xx + yy), 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let h = (fov_deg.clamp(20.0, 140.0) * deg * 0.5).tan();
    let aspect = (sw / sh.max(1.0)).max(0.1);
    let near = 768.0f32;
    let far = 0.05f32;
    let p00 = 1.0 / (h * aspect);
    let p11 = 1.0 / h;
    let p22 = far / (near - far);
    let p32 = (far * near) / (near - far);

    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        out[c][0] = p00 * view[c][0];
        out[c][1] = p11 * view[c][1];
        out[c][2] = p22 * view[c][2] + p32 * view[c][3];
        out[c][3] = -view[c][2];
    }
    out
}

fn active_view_proj(st: &AppState, sw: f32, sh: f32) -> [[f32; 4]; 4] {
    if let Some(vp) = latest_view_proj() {
        return vp;
    }
    let fov = st.game.base_fov.unwrap_or(85) as f32;
    fallback_view_proj(st.game.yaw, st.game.pitch, fov, sw, sh)
}

fn draw_block_markers(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
    markers: &[crate::control_bridge::OreMarker],
    label_dist: f64,
    max_count: usize,
) {
    use std::fmt::Write;
    if markers.is_empty() {
        return;
    }
    let vp = active_view_proj(st, sw, sh);
    let eye = [
        st.game.eye[0] + f64::from(st.freecam_offset[0]),
        st.game.eye[1] + f64::from(st.freecam_offset[1]),
        st.game.eye[2] + f64::from(st.freecam_offset[2]),
    ];
    let mut tag = String::with_capacity(32);

    for marker in markers.iter().take(max_count) {
        let bx = marker.pos[0] as f64 - eye[0];
        let by = marker.pos[1] as f64 - eye[1];
        let bz = marker.pos[2] as f64 - eye[2];

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any_front = false;

        for dx in [0.05, 0.95] {
            for dy in [0.05, 0.95] {
                for dz in [0.05, 0.95] {
                    if let Some([sx, sy]) =
                        project_point(&vp, (bx + dx) as f32, (by + dy) as f32, (bz + dz) as f32, sw, sh)
                    {
                        any_front = true;
                        min_x = min_x.min(sx);
                        min_y = min_y.min(sy);
                        max_x = max_x.max(sx);
                        max_y = max_y.max(sy);
                    }
                }
            }
        }

        if !any_front || max_x < 0.0 || max_y < 0.0 || min_x > sw || min_y > sh {
            continue;
        }

        let x0 = min_x.max(-4.0);
        let y0 = min_y.max(-4.0);
        let w = (max_x.min(sw + 4.0) - x0).clamp(8.0, sw * 0.6);
        let h = (max_y.min(sh + 4.0) - y0).clamp(8.0, sh * 0.6);
        let col = ore_color(marker.kind);

        dl.rect_border(
            Rect::new(x0, y0, w, h),
            3.0,
            col.with_alpha(0.12),
            1.6,
            col.with_alpha(0.92),
        );

        if marker.distance <= label_dist {
            tag.clear();
            let _ = write!(tag, "{} {:.0}m", marker.kind.short_label(), marker.distance);
            let tw = atlas.measure(FontKind::MonoSmall, &tag) + 8.0;
            let tx = x0 + (w - tw) * 0.5;
            let ty = (y0 - 16.0).max(4.0);
            dl.rect(Rect::new(tx, ty, tw, 15.0), 3.0, Color::hex(0x0F1216, 0.78));
            dl.text(atlas, FontKind::MonoSmall, tx + 4.0, ty + 1.0, &tag, col);
        }
    }
}

fn draw_entity_esp_and_tracers(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    sw: f32,
    sh: f32,
) {
    use std::fmt::Write;
    let vp = active_view_proj(st, sw, sh);
    if st.game.nearby_entities.is_empty() {
        return;
    }
    let eye = [
        st.game.eye[0] + f64::from(st.freecam_offset[0]),
        st.game.eye[1] + f64::from(st.freecam_offset[1]),
        st.game.eye[2] + f64::from(st.freecam_offset[2]),
    ];
    let origin = [sw * 0.5, sh * 0.5];
    let mut tag = String::with_capacity(48);
    let (max_ent, ent_lbl_dist) = match st.config.perf_profile {
        crate::config::PerfProfile::Full => (48usize, 40.0f64),
        crate::config::PerfProfile::Balanced => (32usize, 34.0f64),
        crate::config::PerfProfile::Lite => (24usize, 30.0f64),
    };
    let mut drawn = 0usize;

    for e in &st.game.nearby_entities {
        if drawn >= max_ent {
            break;
        }
        let is_player = e.label == "minecraft:player";
        let is_item = e.kind == "item";
        let is_orb = e.kind == "orb";
        let is_hostile = !is_player && !is_item && !is_orb && is_hostile_mob_id(&e.label);
        let is_passive = !is_player && !is_item && !is_hostile;

        let show = (is_player && st.config.esp_players)
            || (is_hostile && st.config.esp_hostile)
            || (is_passive && st.config.esp_passive)
            || (is_item && st.config.esp_items);
        if !show {
            continue;
        }

        let col = if is_player {
            Color::hex(0xF5C542, 1.0)
        } else if is_hostile {
            palette::DANGER
        } else if is_item {
            Color::hex(0x9FB1C8, 1.0)
        } else {
            palette::SUCCESS
        };

        let (hw, h) = if is_player {
            (0.35, 1.85)
        } else if is_item || is_orb {
            (0.22, 0.35)
        } else {
            (0.42, 1.55)
        };

        let ex = e.pos[0] - eye[0];
        let ey = e.pos[1] - eye[1];
        let ez = e.pos[2] - eye[2];

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any_front = false;

        for dx in [-hw, hw] {
            for dy in [0.0, h] {
                for dz in [-hw, hw] {
                    if let Some([sx, sy]) =
                        project_point(&vp, (ex + dx) as f32, (ey + dy) as f32, (ez + dz) as f32, sw, sh)
                    {
                        any_front = true;
                        min_x = min_x.min(sx);
                        min_y = min_y.min(sy);
                        max_x = max_x.max(sx);
                        max_y = max_y.max(sy);
                    }
                }
            }
        }

        let center_pt = project_point(&vp, ex as f32, (ey + h * 0.5) as f32, ez as f32, sw, sh);

        if st.config.tracers_enabled && !is_item && !is_orb {
            let target_pt = if let Some([cx, cy]) = center_pt {
                [cx.clamp(6.0, sw - 6.0), cy.clamp(6.0, sh - 6.0)]
            } else {
                let rx = ex as f32;
                let ry = (ey + h * 0.5) as f32;
                let rz = ez as f32;
                let cx = vp[0][0] * rx + vp[1][0] * ry + vp[2][0] * rz + vp[3][0];
                let cy = vp[0][1] * rx + vp[1][1] * ry + vp[2][1] * rz + vp[3][1];
                let ang = (-cy).atan2(-cx);
                [
                    (origin[0] + ang.cos() * sw * 0.45).clamp(8.0, sw - 8.0),
                    (origin[1] - ang.sin() * sh * 0.45).clamp(8.0, sh - 8.0),
                ]
            };
            dl.line(origin, target_pt, 1.4, col.with_alpha(0.72));
        }

        if st.config.esp_enabled
            && any_front
            && max_x >= 0.0
            && max_y >= 0.0
            && min_x <= sw
            && min_y <= sh
        {
            let x0 = min_x.max(-4.0);
            let y0 = min_y.max(-4.0);
            let bw = (max_x.min(sw + 4.0) - x0).clamp(10.0, sw * 0.6);
            let bh = (max_y.min(sh + 4.0) - y0).clamp(10.0, sh * 0.7);
            dl.rect_border(
                Rect::new(x0, y0, bw, bh),
                3.0,
                col.with_alpha(0.08),
                1.6,
                col.with_alpha(0.95),
            );

            drawn += 1;
            if e.distance <= ent_lbl_dist {
                tag.clear();
                let clean = e.label.strip_prefix("minecraft:").unwrap_or(&e.label);
                if let Some(hp) = e.health {
                    let _ = write!(tag, "{clean} {:.0}hp {:.0}m", hp, e.distance);
                } else {
                    let _ = write!(tag, "{clean} {:.0}m", e.distance);
                }
                let tw = atlas.measure(FontKind::MonoSmall, &tag) + 8.0;
                let tx = x0 + (bw - tw) * 0.5;
                let ty = (y0 - 16.0).max(4.0);
                dl.rect(Rect::new(tx, ty, tw, 15.0), 3.0, Color::hex(0x0F1216, 0.80));
                dl.text(atlas, FontKind::MonoSmall, tx + 4.0, ty + 1.0, &tag, col);
            }
        }
    }
}
