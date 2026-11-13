use crate::config::{CrosshairStyle, HudWidget};
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
        ("S", "+Z")
    } else if norm < 67.5 {
        ("SW", "-X/+Z")
    } else if norm < 112.5 {
        ("W", "-X")
    } else if norm < 157.5 {
        ("NW", "-X/-Z")
    } else if norm < 202.5 {
        ("N", "-Z")
    } else if norm < 247.5 {
        ("NE", "+X/-Z")
    } else if norm < 292.5 {
        ("E", "+X")
    } else {
        ("SE", "+X/+Z")
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

    // Show HUD only inside an active world (or during HUD layout editing), just like Lunar/Wurst
    if !st.game.in_world && !editing {
        return;
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

    // Top instruction banner
    let banner_text = "HUD Layout Editor · Drag panels to move · Press Esc or ` to finish";
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
    editing: bool,
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
        let (dir, axis) = facing_label(st.game.yaw);
        let is_nether = st.game.dimension.contains("nether");
        let is_end = st.game.dimension.contains("end");
        let dim_tag = if is_nether {
            "NETHER"
        } else if is_end {
            "THE END"
        } else {
            "OVERWORLD"
        };
        let l1 = format!("XYZ  {:.0} / {:.0} / {:.0}", x.floor(), y.floor(), z.floor());
        let l2 = format!("{dir} ({axis}) · {dim_tag}");
        let sub = if is_nether {
            format!("OW   {:.0} / {:.0}", x * 8.0, z * 8.0)
        } else if !is_end {
            format!("NTH  {:.0} / {:.0}", x / 8.0, z / 8.0)
        } else {
            String::from("END  No portal ratio")
        };
        (l1, l2, sub)
    } else if !st.game.socket_connected && !editing {
        (
            String::from("XYZ  Socket offline"),
            String::from("Enable agentControl in Extras"),
            String::from("NTH  -- / --"),
        )
    } else {
        (
            String::from("XYZ  128 / 64 / -240"),
            String::from("N (-Z) · OVERWORLD"),
            String::from("NTH  16 / -30"),
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
    if show_nether {
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
    let w = 204.0;
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

    let fps_str = format!(
        "{:.0} FPS  {:.1}ms  (1%: {:.0})",
        st.fps, st.frame_ms, st.low_1pct_fps
    );
    dl.text(
        atlas,
        FontKind::MonoSmall,
        r.x + 10.0,
        r.y + 8.0,
        &fps_str,
        palette::TEXT_PRIMARY.with_alpha(alpha),
    );

    let xray_on = st.config.xray_terrain || st.config.xray_entities;
    let fb_status = match (st.config.fullbright_enabled, xray_on) {
        (true, true) => "FB ON · XRAY ON",
        (true, false) => "FULLBRIGHT ON",
        (false, true) => "FB OFF · XRAY ON",
        (false, false) => "FULLBRIGHT OFF",
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
    editing: bool,
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
            format!("HP {:.1} (+{:.0})", st.game.health, st.game.absorption)
        } else {
            format!("HP {:.1}/20", st.game.health)
        };
        let food_txt = format!("FOOD {}/20  XP {}", st.game.food, st.game.xp_level);
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
                format!("Target: {lbl} ({thp:.1} HP)")
            } else {
                format!("Look: {lbl}")
            }
        } else if let Some(held) = &st.game.held_item {
            if st.game.held_damage > 0 {
                format!("Held: {held} (-{} dmg)", st.game.held_damage)
            } else {
                format!("Held: {held}")
            }
        } else {
            String::from("Held: Empty hand")
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
            let clean = eid.strip_prefix("minecraft:").unwrap_or(eid);
            let eff_txt = format!("Effect: {clean} {} ({}s)", amp + 1, secs);
            dl.text(
                atlas,
                FontKind::MonoSmall,
                r.x + 10.0,
                r.y + 84.0,
                &eff_txt,
                palette::TEXT_SECONDARY.with_alpha(alpha),
            );
        }
    } else if !st.game.socket_connected && !editing {
        dl.text(
            atlas,
            FontKind::UiSemi,
            r.x + 10.0,
            r.y + 10.0,
            "Vitals & Inventory",
            palette::TEXT_PRIMARY.with_alpha(alpha),
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 10.0,
            r.y + 32.0,
            "Waiting for horizon-control.sock",
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );
    } else {
        dl.text(
            atlas,
            FontKind::MonoBody,
            r.x + 10.0,
            r.y + 8.0,
            "HP 20.0/20",
            palette::SUCCESS.with_alpha(alpha),
        );
        dl.text(
            atlas,
            FontKind::MonoSmall,
            r.x + 10.0,
            r.y + 28.0,
            "FOOD 20/20  XP 30",
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
            "Target: Preview",
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
            "Waypoint Compass · Add waypoints via [`]"
        } else {
            "No active waypoints in this dimension"
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
        String::from("World Time: Not in world")
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
    editing: bool,
) -> Rect {
    let size = 132.0;
    let r = widget_rect(&st.config.hud_radar, size, size + 22.0, sw, sh);
    let alpha = if st.config.hud_radar.enabled { 1.0 } else { 0.45 };

    dl.rect_border(
        r,
        8.0,
        palette::HUD_BG.with_alpha(alpha),
        1.0,
        palette::BORDER.with_alpha(alpha),
    );

    let map_r = Rect::new(r.x + 4.0, r.y + 22.0, size - 8.0, size - 8.0);
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
    // Player dot in center
    dl.rect(
        Rect::new(cx - 3.0, cy - 3.0, 6.0, 6.0),
        3.0,
        palette::TEXT_PRIMARY.with_alpha(alpha),
    );

    if st.game.in_world {
        let title = format!("Radar · {} near", st.game.nearby_entities.len());
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
            // Rotate so forward (player look direction) is UP (-Y on screen)
            // Minecraft yaw: 0=+Z, -90=+X => forward = (-sin(yaw), cos(yaw))
            let rx = (-dx * cos_y - dz * sin_y) / max_range * half;
            let ry = (dx * sin_y - dz * cos_y) / max_range * half;
            let px = (cx + rx).clamp(map_r.x + 3.0, map_r.x + map_r.w - 3.0);
            let py = (cy + ry).clamp(map_r.y + 3.0, map_r.y + map_r.h - 3.0);
            let dot_col = if e.kind == "item" {
                palette::ACCENT
            } else if e.kind == "orb" {
                palette::SUCCESS
            } else if e.label.contains("player") {
                Color::hex(0x52A8FF, 1.0)
            } else {
                palette::DANGER
            };
            dl.rect(
                Rect::new(px - 2.5, py - 2.5, 5.0, 5.0),
                2.5,
                dot_col.with_alpha(alpha),
            );
        }
        dl.pop_scissor();
    } else {
        let lbl = if editing { "Radar · Preview" } else { "Radar · Idle" };
        dl.text(
            atlas,
            FontKind::MonoSmall,
            r.x + 8.0,
            r.y + 5.0,
            lbl,
            palette::TEXT_SECONDARY.with_alpha(alpha),
        );
    }

    r
}
