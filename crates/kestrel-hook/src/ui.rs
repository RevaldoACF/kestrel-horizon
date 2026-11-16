use crate::config::{
    AimTarget, CrosshairStyle, FullbrightMode, KestrelConfig, OrePreset, PerfProfile, Waypoint,
    XrayKey, ZoomKey,
};
use crate::control_bridge::{send_cmd, ControlCmd};
use crate::draw::{palette, Color, DrawList, Rect};
use crate::font::{FontAtlas, FontKind};
use crate::hud::waypoint_color;
use crate::state::{AppState, FocusId, NavTab, TextTarget, UiKey};
use std::time::Duration;

#[derive(Clone, Copy)]
enum ControlKind {
    Button,
    Toggle,
    Slider(f32, f32, f32),
    Text(TextTarget),
}

struct Focusable {
    id: FocusId,
    rect: Rect,
    kind: ControlKind,
}

fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

pub fn render_menu(st: &mut AppState, dl: &mut DrawList, atlas: &FontAtlas) {
    if !st.menu_open && st.menu_anim <= 0.01 {
        st.dragging_menu = None;
        return;
    }

    let sw = st.screen_size[0].max(1) as f32;
    let sh = st.screen_size[1].max(1) as f32;

    let raw_anim = st.menu_anim.clamp(0.0, 1.0);
    let anim = ease_out_cubic(raw_anim);

    // Compact, breathable floating panel (no dark fullscreen scrim)
    let base_w = (sw - 64.0).clamp(620.0, 800.0);
    let base_h = (sh - 64.0).clamp(400.0, 524.0);

    // Initialize or clamp draggable position
    let [cur_x, cur_y] = st.menu_pos.unwrap_or([
        ((sw - base_w) * 0.5).round(),
        ((sh - base_h) * 0.5).round(),
    ]);
    let base_x = cur_x.clamp(12.0, (sw - base_w - 12.0).max(12.0));
    let base_y = cur_y.clamp(12.0, (sh - base_h - 12.0).max(12.0));
    st.menu_pos = Some([base_x, base_y]);

    // Subtle scale + lift during open/close with ` or ~
    let scale = 0.965 + 0.035 * anim;
    let panel_w = (base_w * scale).round();
    let panel_h = (base_h * scale).round();
    let px = (base_x + (base_w - panel_w) * 0.5).round();
    let py = (base_y + (base_h - panel_h) * 0.5 + (1.0 - anim) * 10.0).round();
    let panel = Rect::new(px, py, panel_w, panel_h);

    dl.shadow(panel, 10.0, 20.0, palette::SHADOW.with_alpha(anim));
    dl.rect_border(
        panel,
        10.0,
        palette::CANVAS.with_alpha(anim),
        1.0,
        palette::BORDER.with_alpha(0.9 * anim),
    );

    // Fade cleanly while closing without overflowing scissor during scale
    if raw_anim < 0.35 && !st.menu_open {
        return;
    }

    let mut focusables: Vec<Focusable> = Vec::with_capacity(36);

    // ---------------------------------------------------------
    // 1. Top Bar (Draggable handle + Search + Compact Actions)
    // ---------------------------------------------------------
    let top_h = 46.0;
    let top_bar = Rect::new(px, py, panel_w, top_h);
    dl.rect(
        Rect::new(px + 1.0, py + top_h, panel_w - 2.0, 1.0),
        0.0,
        palette::BORDER.with_alpha(0.65 * anim),
    );

    // Subtle 2x3 grip dots + Kestrel mark + Wordmark
    for row in 0..3 {
        for col in 0..2 {
            dl.rect(
                Rect::new(px + 12.0 + col as f32 * 4.0, py + 17.0 + row as f32 * 5.0, 2.0, 2.0),
                1.0,
                palette::TEXT_SECONDARY.with_alpha(0.35 * anim),
            );
        }
    }
    dl.kestrel_crest(
        px + 26.0,
        py + 14.0,
        18.0,
        palette::ACCENT.with_alpha(anim),
        palette::TEXT_PRIMARY.with_alpha(anim),
    );
    dl.text(
        atlas,
        FontKind::UiHeading,
        px + 52.0,
        py + 13.0,
        "KESTREL",
        palette::TEXT_PRIMARY.with_alpha(anim),
    );

    // Right actions: Search | HUD | Panic | Close (x)
    let close_rect = Rect::new(px + panel_w - 40.0, py + 9.0, 28.0, 28.0);
    let panic_rect = Rect::new(close_rect.x - 66.0, py + 9.0, 58.0, 28.0);
    let hud_rect = Rect::new(panic_rect.x - 62.0, py + 9.0, 54.0, 28.0);
    let search_x = (px + 154.0).max(px + 140.0);
    let search_w = (hud_rect.x - search_x - 12.0).clamp(120.0, 230.0);
    let search_rect = Rect::new(hud_rect.x - search_w - 12.0, py + 9.0, search_w, 28.0);

    draw_text_input(
        st,
        dl,
        atlas,
        search_rect,
        &st.search_query.clone(),
        "Search...",
        st.active_text_target == Some(TextTarget::Search),
        st.focused == Some(FocusId::SearchBox),
        FocusId::SearchBox,
        TextTarget::Search,
        &mut focusables,
    );
    draw_top_pill(
        st,
        dl,
        atlas,
        hud_rect,
        "HUD",
        false,
        FocusId::EditHudBtn,
        &mut focusables,
    );
    draw_top_pill(
        st,
        dl,
        atlas,
        panic_rect,
        "Panic",
        true,
        FocusId::PanicBtn,
        &mut focusables,
    );
    draw_close_button(st, dl, atlas, close_rect, &mut focusables);

    // ---------------------------------------------------------
    // 2. Left Rail (148px, crisp 1-line items, active left bar)
    // ---------------------------------------------------------
    let rail_w = 148.0;
    let body_y = py + top_h + 1.0;
    let body_h = panel_h - top_h - 1.0;
    dl.rect(
        Rect::new(px + rail_w, body_y, 1.0, body_h),
        0.0,
        palette::BORDER.with_alpha(0.55 * anim),
    );

    let tabs = [
        (NavTab::Survival, FocusId::TabSurvival, "Visuals"),
        (NavTab::Automation, FocusId::TabAutomation, "Combat & Auto"),
        (NavTab::Hud, FocusId::TabHud, "HUD"),
        (NavTab::Waypoints, FocusId::TabWaypoints, "Waypoints"),
        (NavTab::System, FocusId::TabSystem, "Settings"),
    ];

    for (i, (tab, fid, title)) in tabs.iter().enumerate() {
        let tr = Rect::new(px + 10.0, body_y + 12.0 + i as f32 * 38.0, rail_w - 20.0, 34.0);
        let selected = st.active_tab == *tab && st.search_query.trim().is_empty();
        let hovered = tr.contains(st.cursor_pos[0], st.cursor_pos[1]);
        let focused = st.focused == Some(*fid);

        if selected {
            dl.rect(tr, 6.0, palette::SURFACE_HOVER.with_alpha(0.9 * anim));
            dl.rect(
                Rect::new(tr.x + 2.0, tr.y + 8.0, 2.5, tr.h - 16.0),
                1.25,
                palette::ACCENT.with_alpha(anim),
            );
        } else if hovered {
            dl.rect(tr, 6.0, palette::SURFACE.with_alpha(0.7 * anim));
        }
        if focused {
            dl.rect_border(tr, 6.0, Color::hex(0x000000, 0.0), 1.0, palette::ACCENT);
        }
        dl.text(
            atlas,
            if selected { FontKind::UiSemi } else { FontKind::UiBody },
            tr.x + 12.0,
            tr.y + 8.0,
            title,
            if selected {
                palette::TEXT_PRIMARY.with_alpha(anim)
            } else {
                palette::TEXT_SECONDARY.with_alpha(anim)
            },
        );
        focusables.push(Focusable {
            id: *fid,
            rect: tr,
            kind: ControlKind::Button,
        });
    }

    // Quiet single-line connection indicator at bottom of rail
    let dot_col = if st.game.socket_connected {
        palette::SUCCESS
    } else {
        palette::TEXT_SECONDARY
    };
    let status_txt = if st.game.socket_connected {
        "Connected"
    } else {
        "Standby"
    };
    let sy = body_y + body_h - 26.0;
    dl.rect(Rect::new(px + 18.0, sy + 5.0, 6.0, 6.0), 3.0, dot_col.with_alpha(anim));
    dl.text(
        atlas,
        FontKind::UiSmall,
        px + 30.0,
        sy,
        status_txt,
        palette::TEXT_SECONDARY.with_alpha(0.85 * anim),
    );

    // ---------------------------------------------------------
    // 3. Scrollable Flat Content Area
    // ---------------------------------------------------------
    let content = Rect::new(px + rail_w + 1.0, body_y, panel_w - rail_w - 1.0, body_h);
    dl.push_scissor(content);

    if content.contains(st.cursor_pos[0], st.cursor_pos[1]) && st.scroll_delta.abs() > 0.01 {
        st.scroll_y = (st.scroll_y - st.scroll_delta * 30.0).clamp(0.0, st.max_scroll_y);
    }

    let cx = content.x + 22.0;
    let cw = content.w - 44.0;
    let mut cy = content.y + 16.0 - st.scroll_y;

    let query = st.search_query.trim().to_lowercase();
    if !query.is_empty() {
        cy = render_search_results(st, dl, atlas, cx, cy, cw, &query, &mut focusables);
    } else {
        cy = match st.active_tab {
            NavTab::Survival => render_visuals_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::Automation => render_automation_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::Hud => render_hud_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::Waypoints => render_waypoints_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::System => render_settings_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
        };
    }

    let total_content_h = (cy + st.scroll_y) - (content.y + 16.0);
    st.max_scroll_y = (total_content_h - (content.h - 28.0)).max(0.0);
    st.scroll_y = st.scroll_y.clamp(0.0, st.max_scroll_y);

    dl.pop_scissor();

    // Minimal scrollbar
    if st.max_scroll_y > 1.0 {
        let track_h = content.h - 20.0;
        let thumb_h = (track_h * (content.h / (content.h + st.max_scroll_y))).max(28.0);
        let thumb_y = content.y + 10.0 + (st.scroll_y / st.max_scroll_y) * (track_h - thumb_h);
        dl.rect(
            Rect::new(content.x + content.w - 6.0, thumb_y, 3.0, thumb_h),
            1.5,
            palette::BORDER_STRONG.with_alpha(0.65 * anim),
        );
    }

    // Toast notification anchored just below floating window
    if let Some((msg, at)) = st.toast.clone() {
        if at.elapsed() < Duration::from_secs(3) {
            let tw = atlas.measure(FontKind::UiSmall, &msg) + 24.0;
            let tr = Rect::new(px + (panel_w - tw) * 0.5, py + panel_h + 10.0, tw, 28.0);
            dl.shadow(tr, 8.0, 10.0, palette::SHADOW);
            dl.rect_border(tr, 8.0, palette::CANVAS, 1.0, palette::ACCENT);
            dl.text(
                atlas,
                FontKind::UiSmall,
                tr.x + 12.0,
                tr.y + 6.0,
                &msg,
                palette::TEXT_PRIMARY,
            );
        } else {
            st.toast = None;
        }
    }

    // Handle drag & control interactions when open
    if st.menu_open {
        handle_menu_drag(st, top_bar, &focusables, sw, sh, base_w, base_h);
        process_interactions(st, &focusables);
    }
}

fn handle_menu_drag(
    st: &mut AppState,
    top_bar: Rect,
    focusables: &[Focusable],
    sw: f32,
    sh: f32,
    panel_w: f32,
    panel_h: f32,
) {
    let [mx, my] = st.cursor_pos;
    if st.mouse_just_pressed && top_bar.contains(mx, my) {
        let over_control = focusables
            .iter()
            .any(|f| top_bar.contains(f.rect.x, f.rect.y) && f.rect.contains(mx, my));
        if !over_control {
            let cur = st.menu_pos.unwrap_or([top_bar.x, top_bar.y]);
            st.dragging_menu = Some([mx - cur[0], my - cur[1]]);
        }
    }
    if st.mouse_down
        && let Some([ox, oy]) = st.dragging_menu
    {
        let nx = (mx - ox).clamp(12.0, (sw - panel_w - 12.0).max(12.0));
        let ny = (my - oy).clamp(12.0, (sh - panel_h - 12.0).max(12.0));
        st.menu_pos = Some([nx.round(), ny.round()]);
    }
    if !st.mouse_down {
        st.dragging_menu = None;
    }
}

// ---------------------------------------------------------
// Tabs (Clean flat rows + inset option strips)
// ---------------------------------------------------------

fn render_visuals_tab(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    y = draw_section_label(dl, atlas, x, y, w, "LIGHTING & CAMERA");

    // Fullbright
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Fullbright",
        "Brightens caves and night areas without torches",
        st.config.fullbright_enabled,
        FocusId::FullbrightToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 58.0);
    {
        let half = (iw - 20.0) * 0.44;
        draw_segment_button(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 14.0, (half - 6.0) * 0.5, 28.0),
            "Max",
            st.config.fullbright_mode == FullbrightMode::Max,
            FocusId::FullbrightModeMax,
            focusables,
        );
        draw_segment_button(
            st,
            dl,
            atlas,
            Rect::new(ix + (half - 6.0) * 0.5 + 6.0, iy + 14.0, (half - 6.0) * 0.5, 28.0),
            "Soft",
            st.config.fullbright_mode == FullbrightMode::Soft,
            FocusId::FullbrightModeSoft,
            focusables,
        );
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix + half + 18.0, iy + 6.0, iw - half - 18.0, 42.0),
            "Soft Strength",
            &format!("{:.0}%", st.config.fullbright_strength * 100.0),
            st.config.fullbright_strength,
            0.35,
            1.0,
            0.05,
            FocusId::FullbrightStrength,
            focusables,
        );
    }
    y = next_y;

    // Horizon Brightness
    let gamma_val = st.game.horizon_gamma.unwrap_or(0.5);
    let gamma_lbl = if st.game.socket_connected {
        format!("{:.0}%", gamma_val * 100.0)
    } else {
        String::from("Offline")
    };
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(x + 4.0, y + 6.0, w - 8.0, 40.0),
        "Game Brightness",
        &gamma_lbl,
        gamma_val,
        0.0,
        1.0,
        0.05,
        FocusId::GammaSlider,
        focusables,
    );
    y += 50.0;
    draw_hairline(dl, x, y, w);

    // OptiZoom
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "OptiZoom",
        "Hold key to smoothly zoom in on distant targets",
        st.config.zoom_enabled,
        FocusId::ZoomToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 50.0);
    {
        let sw_w = iw * 0.60;
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 4.0, sw_w, 40.0),
            "Zoom FOV",
            &format!("{} deg", st.config.zoom_fov),
            st.config.zoom_fov as f32,
            15.0,
            60.0,
            5.0,
            FocusId::ZoomFovSlider,
            focusables,
        );
        draw_cycle_chip(
            st,
            dl,
            atlas,
            Rect::new(ix + sw_w + 14.0, iy + 11.0, iw - sw_w - 14.0, 28.0),
            "Key",
            st.config.zoom_key.label(),
            FocusId::ZoomKeyCycle,
            focusables,
        );
    }
    y = next_y;

    // Crosshair+
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Crosshair+",
        "Sharp custom reticle overlaid cleanly",
        st.config.crosshair_enabled,
        FocusId::CrosshairToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 50.0);
    {
        let cyc_w = 136.0;
        draw_cycle_chip(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 11.0, cyc_w, 28.0),
            "Style",
            st.config.crosshair_style.label(),
            FocusId::CrosshairStyleCycle,
            focusables,
        );
        let rem = (iw - cyc_w - 24.0) * 0.5;
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix + cyc_w + 12.0, iy + 4.0, rem, 40.0),
            "Size",
            &format!("{:.0}px", st.config.crosshair_size),
            st.config.crosshair_size,
            3.0,
            14.0,
            1.0,
            FocusId::CrosshairSizeSlider,
            focusables,
        );
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix + cyc_w + 24.0 + rem, iy + 4.0, rem, 40.0),
            "Gap",
            &format!("{:.0}px", st.config.crosshair_gap),
            st.config.crosshair_gap,
            1.0,
            10.0,
            1.0,
            FocusId::CrosshairGapSlider,
            focusables,
        );
    }
    y = next_y;

    // Freecam
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Freecam",
        "Fly camera freely while your character stays safely in place",
        st.config.freecam_enabled,
        FocusId::FreecamToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 46.0);
    {
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 3.0, iw, 40.0),
            "Flight Speed",
            &format!("{:.0} blk/s", st.config.freecam_speed),
            st.config.freecam_speed,
            4.0,
            36.0,
            2.0,
            FocusId::FreecamSpeedSlider,
            focusables,
        );
    }
    y = next_y;

    y += 10.0;
    y = draw_section_label(dl, atlas, x, y, w, "X-RAY & ESP");

    // Ore X-Ray
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Ore X-Ray",
        "3D boxes for buried Diamond, Debris, Gold, and Iron",
        st.config.xray_ores,
        FocusId::XrayOresToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 84.0);
    {
        let half = (iw - 12.0) * 0.5;
        draw_cycle_chip(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 6.0, half, 26.0),
            "Filter",
            st.config.xray_ore_preset.label(),
            FocusId::XrayOrePresetCycle,
            focusables,
        );
        draw_cycle_chip(
            st,
            dl,
            atlas,
            Rect::new(ix + half + 12.0, iy + 6.0, half, 26.0),
            "Key",
            st.config.xray_key.label(),
            FocusId::XrayKeyCycle,
            focusables,
        );
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 38.0, iw, 40.0),
            "Scan Radius",
            &format!("{} blocks", st.config.xray_radius),
            st.config.xray_radius as f32,
            12.0,
            48.0,
            4.0,
            FocusId::XrayRadiusSlider,
            focusables,
        );
    }
    y = next_y;

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Cave Wireframe",
        "See underground caves and lava pockets through stone",
        st.config.xray_terrain,
        FocusId::XrayTerrainToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Chest & Mob Through-Walls",
        "Render chests, players, and mobs through walls",
        st.config.xray_entities,
        FocusId::XrayEntitiesToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Entity ESP Boxes",
        "3D boxes with name, HP, and distance tags",
        st.config.esp_enabled,
        FocusId::EspToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Tracers",
        "Guide lines from screen center to nearby entities",
        st.config.tracers_enabled,
        FocusId::TracersToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 42.0);
    {
        let q_w = (iw - 32.0) / 5.0;
        let filters = [
            ("Players", st.config.esp_players, FocusId::EspPlayersToggle),
            ("Hostile", st.config.esp_hostile, FocusId::EspHostileToggle),
            ("Passive", st.config.esp_passive, FocusId::EspPassiveToggle),
            ("Items", st.config.esp_items, FocusId::EspItemsToggle),
            ("Chests", st.config.esp_chests, FocusId::EspChestsToggle),
        ];
        for (idx, (lbl, on, fid)) in filters.into_iter().enumerate() {
            let fx = ix + idx as f32 * (q_w + 8.0);
            draw_segment_button(
                st,
                dl,
                atlas,
                Rect::new(fx, iy + 8.0, q_w, 26.0),
                lbl,
                on,
                fid,
                focusables,
            );
        }
    }
    y = next_y;

    y
}

fn render_automation_tab(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    if !st.game.socket_connected {
        dl.rect(Rect::new(x, y, 3.0, 34.0), 1.5, palette::DANGER);
        dl.text(
            atlas,
            FontKind::UiSmall,
            x + 12.0,
            y + 9.0,
            "Game link standby · join a world to run combat and mining helpers.",
            palette::TEXT_SECONDARY,
        );
        y += 42.0;
    }

    y = draw_section_label(dl, atlas, x, y, w, "COMBAT");

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AimAssist",
        "Smoothly guides crosshair toward nearby targets",
        st.config.aim_assist,
        FocusId::AimAssistToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 82.0);
    {
        draw_cycle_chip(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 6.0, iw, 26.0),
            "Targets",
            st.config.aim_target.label(),
            FocusId::AimTargetCycle,
            focusables,
        );
        let third = (iw - 24.0) / 3.0;
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 38.0, third, 40.0),
            "FOV",
            &format!("{:.0} deg", st.config.aim_fov),
            st.config.aim_fov,
            20.0,
            150.0,
            5.0,
            FocusId::AimFovSlider,
            focusables,
        );
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix + third + 12.0, iy + 38.0, third, 40.0),
            "Range",
            &format!("{:.1}m", st.config.aim_range),
            st.config.aim_range,
            2.5,
            6.0,
            0.5,
            FocusId::AimRangeSlider,
            focusables,
        );
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix + (third + 12.0) * 2.0, iy + 38.0, third, 40.0),
            "Smooth",
            &format!("{:.0}%", st.config.aim_smooth * 100.0),
            st.config.aim_smooth,
            0.15,
            0.90,
            0.05,
            FocusId::AimSmoothSlider,
            focusables,
        );
    }
    y = next_y;

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "KillAura",
        "Automatically turns and attacks nearby targets",
        st.config.killaura_enabled,
        FocusId::KillAuraToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 82.0);
    {
        draw_cycle_chip(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 6.0, iw, 26.0),
            "Targets",
            st.config.killaura_target.label(),
            FocusId::KillAuraTargetCycle,
            focusables,
        );
        let third = (iw - 24.0) / 3.0;
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 38.0, third, 40.0),
            "Range",
            &format!("{:.1}m", st.config.killaura_range),
            st.config.killaura_range,
            2.5,
            6.0,
            0.5,
            FocusId::KillAuraRangeSlider,
            focusables,
        );
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix + third + 12.0, iy + 38.0, third, 40.0),
            "FOV",
            &format!("{:.0} deg", st.config.killaura_fov),
            st.config.killaura_fov,
            30.0,
            360.0,
            15.0,
            FocusId::KillAuraFovSlider,
            focusables,
        );
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix + (third + 12.0) * 2.0, iy + 38.0, third, 40.0),
            "Cooldown",
            &format!("{} ms", st.config.killaura_delay_ms),
            st.config.killaura_delay_ms as f32,
            250.0,
            1200.0,
            25.0,
            FocusId::KillAuraDelaySlider,
            focusables,
        );
    }
    y = next_y;

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "Criticals",
        "Micro-jumps on attack so hits land while falling for 1.5x damage",
        st.config.criticals_enabled,
        FocusId::CriticalsToggle,
        focusables,
    );

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "TriggerBot",
        "Automatically strikes when crosshair aims at an entity",
        st.config.trigger_bot,
        FocusId::TriggerBotToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 46.0);
    {
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 3.0, iw, 40.0),
            "Hit Cooldown",
            &format!("{} ms", st.config.trigger_bot_delay_ms),
            st.config.trigger_bot_delay_ms as f32,
            400.0,
            1200.0,
            50.0,
            FocusId::TriggerBotDelaySlider,
            focusables,
        );
    }
    y = next_y;

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoClicker",
        "Pulses left click continuously while holding attack",
        st.config.auto_clicker,
        FocusId::AutoClickerToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 46.0);
    {
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 3.0, iw, 40.0),
            "Target CPS",
            &format!("{} CPS", st.config.auto_clicker_cps),
            st.config.auto_clicker_cps as f32,
            4.0,
            20.0,
            1.0,
            FocusId::AutoClickerCpsSlider,
            focusables,
        );
    }
    y = next_y;

    y += 10.0;
    y = draw_section_label(dl, atlas, x, y, w, "MINING");

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoMine",
        "Holds mine automatically on aimed blocks and releases when looking away",
        st.config.auto_mine,
        FocusId::AutoMineToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "SpeedMine",
        "Breaks blocks faster and clears break delay immediately",
        st.config.speed_mine,
        FocusId::SpeedMineToggle,
        focusables,
    );
    let speed_pct = 100 + st.config.speed_mine_power * 20;
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 46.0);
    {
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 3.0, iw, 40.0),
            "Mine Speed",
            &format!("{:.1}x", speed_pct as f32 / 100.0),
            st.config.speed_mine_power as f32,
            1.0,
            5.0,
            1.0,
            FocusId::SpeedMinePowerSlider,
            focusables,
        );
    }
    y = next_y;
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoTool",
        "Switches hotbar slot to the best tool when mining a block",
        st.config.auto_tool,
        FocusId::AutoToolToggle,
        focusables,
    );

    y += 10.0;
    y = draw_section_label(dl, atlas, x, y, w, "SURVIVAL");

    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoSprint",
        "Keeps sprint active whenever you move forward",
        st.config.auto_sprint,
        FocusId::AutoSprintToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoRespawn",
        "Automatically respawns right away when you die",
        st.config.auto_respawn,
        FocusId::AutoRespawnToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoEat",
        "Selects food in hotbar and eats below hunger threshold",
        st.config.auto_eat,
        FocusId::AutoEatToggle,
        focusables,
    );
    let (ix, iy, iw, next_y) = draw_inset_bg(dl, x, y, w, 46.0);
    {
        draw_slider(
            st,
            dl,
            atlas,
            Rect::new(ix, iy + 3.0, iw, 40.0),
            "Eat At Hunger",
            &format!("<= {}/20", st.config.auto_eat_hunger),
            st.config.auto_eat_hunger as f32,
            6.0,
            19.0,
            1.0,
            FocusId::AutoEatHungerSlider,
            focusables,
        );
    }
    y = next_y;
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoTotem",
        "Moves a Totem of Undying into your offhand when missing",
        st.config.auto_totem,
        FocusId::AutoTotemToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AutoFish",
        "Reels in and recasts your fishing rod on a bite",
        st.config.auto_fish,
        FocusId::AutoFishToggle,
        focusables,
    );
    y = draw_module_row(
        st,
        dl,
        atlas,
        x,
        y,
        w,
        "AntiAFK",
        "Occasionally taps sneak when idle to avoid server kicks",
        st.config.anti_afk,
        FocusId::AntiAfkToggle,
        focusables,
    );

    y
}

fn render_hud_tab(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    dl.text(
        atlas,
        FontKind::UiHeading,
        x,
        y + 2.0,
        "On-Screen HUD",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        x,
        y + 24.0,
        "Toggle panels below or drag them freely on screen.",
        palette::TEXT_SECONDARY,
    );

    let reset_btn = Rect::new(x + w - 98.0, y + 6.0, 98.0, 30.0);
    let edit_btn = Rect::new(reset_btn.x - 114.0, y + 6.0, 104.0, 30.0);
    draw_action_button(
        st,
        dl,
        atlas,
        edit_btn,
        "Drag Layout",
        true,
        FocusId::HudEnterEdit,
        focusables,
    );
    draw_action_button(
        st,
        dl,
        atlas,
        reset_btn,
        "Reset Pos",
        false,
        FocusId::HudResetLayout,
        focusables,
    );
    y += 52.0;

    y = draw_section_label(dl, atlas, x, y, w, "WIDGETS");

    let rows = [
        (
            "Coordinates",
            "Live position, facing direction, and dimension",
            st.config.hud_coords.enabled,
            FocusId::HudToggleCoords,
        ),
        (
            "Nether Portal Converter",
            "Auto-scaled Overworld and Nether coordinates",
            st.config.hud_nether_convert,
            FocusId::HudToggleNether,
        ),
        (
            "FPS & Visual Status",
            "Clean FPS readout and active mode badge",
            st.config.hud_fps.enabled,
            FocusId::HudToggleFps,
        ),
        (
            "Keystrokes & CPS",
            "WASD, Space, and live click speed",
            st.config.hud_keystrokes.enabled,
            FocusId::HudToggleKeystrokes,
        ),
        (
            "Vitals & Gear",
            "Health, food, totems, arrows, and target",
            st.config.hud_vitals.enabled,
            FocusId::HudToggleVitals,
        ),
        (
            "Waypoint Compass",
            "Top directional bar for saved waypoints",
            st.config.hud_compass.enabled,
            FocusId::HudToggleCompass,
        ),
        (
            "World & Session Clock",
            "In-game day, time, weather, and play duration",
            st.config.hud_clock.enabled,
            FocusId::HudToggleClock,
        ),
        (
            "Entity Radar",
            "Top-down minimap of nearby players and mobs",
            st.config.hud_radar.enabled,
            FocusId::HudToggleRadar,
        ),
    ];

    for (title, desc, enabled, fid) in rows {
        y = draw_module_row(st, dl, atlas, x, y, w, title, desc, enabled, fid, focusables);
    }

    y
}

fn render_waypoints_tab(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    dl.text(
        atlas,
        FontKind::UiHeading,
        x,
        y,
        "Waypoints",
        palette::TEXT_PRIMARY,
    );
    let pos_hint = if st.game.in_world {
        format!(
            "Current position: {:.0}, {:.0}, {:.0} ({})",
            st.game.pos[0].floor(),
            st.game.pos[1].floor(),
            st.game.pos[2].floor(),
            st.game
                .dimension
                .strip_prefix("minecraft:")
                .unwrap_or(&st.game.dimension)
        )
    } else {
        String::from("Enter a world to save your current coordinates.")
    };
    dl.text(
        atlas,
        FontKind::UiSmall,
        x,
        y + 22.0,
        &pos_hint,
        palette::TEXT_SECONDARY,
    );

    let input_w = (w - 144.0).max(160.0);
    draw_text_input(
        st,
        dl,
        atlas,
        Rect::new(x, y + 46.0, input_w, 30.0),
        &st.waypoint_draft_name.clone(),
        "Waypoint name (Base, Portal, Stronghold)...",
        st.active_text_target == Some(TextTarget::WaypointName),
        st.focused == Some(FocusId::WaypointInput),
        FocusId::WaypointInput,
        TextTarget::WaypointName,
        focusables,
    );

    let add_rect = Rect::new(x + input_w + 10.0, y + 46.0, 134.0, 30.0);
    draw_action_button_enabled(
        st,
        dl,
        atlas,
        add_rect,
        if st.game.in_world { "Save Current" } else { "Not in World" },
        st.game.in_world,
        st.game.in_world,
        FocusId::WaypointAddHere,
        focusables,
    );
    y += 92.0;

    y = draw_section_label(dl, atlas, x, y, w, "SAVED MARKERS");

    if st.config.waypoints.is_empty() {
        dl.text(
            atlas,
            FontKind::UiSmall,
            x,
            y + 6.0,
            "No waypoints saved yet. Name one above to pin it to your compass.",
            palette::TEXT_SECONDARY,
        );
        return y + 34.0;
    }

    for (idx, wp) in st.config.waypoints.iter().enumerate() {
        let rr = Rect::new(x, y, w, 46.0);
        if rr.contains(st.cursor_pos[0], st.cursor_pos[1]) {
            dl.rect(rr, 6.0, palette::SURFACE.with_alpha(0.7));
        }
        dl.rect(
            Rect::new(rr.x + 6.0, rr.y + 14.0, 8.0, 18.0),
            3.0,
            waypoint_color(wp.color_idx),
        );
        dl.text(
            atlas,
            FontKind::UiSemi,
            rr.x + 22.0,
            rr.y + 6.0,
            &wp.name,
            palette::TEXT_PRIMARY,
        );
        let dim = wp.dimension.strip_prefix("minecraft:").unwrap_or(&wp.dimension);
        let meta = if st.game.in_world && st.game.dimension == wp.dimension {
            let dx = wp.x as f64 - st.game.pos[0];
            let dz = wp.z as f64 - st.game.pos[2];
            let d = (dx * dx + dz * dz).sqrt();
            format!("{}, {}, {} · {} · {:.0}m", wp.x, wp.y, wp.z, dim, d)
        } else {
            format!("{}, {}, {} · {}", wp.x, wp.y, wp.z, dim)
        };
        dl.text(
            atlas,
            FontKind::MonoSmall,
            rr.x + 22.0,
            rr.y + 25.0,
            &meta,
            palette::TEXT_SECONDARY,
        );

        let del_rect = Rect::new(rr.x + w - 64.0, rr.y + 10.0, 58.0, 26.0);
        let sw_rect = Rect::new(del_rect.x - 48.0, rr.y + 13.0, 38.0, 20.0);
        draw_switch(st, dl, sw_rect, wp.enabled, FocusId::WaypointItemToggle(idx), focusables);
        draw_action_button(
            st,
            dl,
            atlas,
            del_rect,
            "Delete",
            false,
            FocusId::WaypointItemDelete(idx),
            focusables,
        );
        y += 48.0;
        draw_hairline(dl, x, y - 1.0, w);
    }

    y
}

fn render_settings_tab(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    y = draw_section_label(dl, atlas, x, y, w, "PERFORMANCE");

    dl.text(
        atlas,
        FontKind::UiSemi,
        x + 4.0,
        y + 4.0,
        "X-Ray & ESP Scan Mode",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        x + 4.0,
        y + 23.0,
        "Full uses original scan density & range; Balanced or Lite reduces CPU load.",
        palette::TEXT_SECONDARY,
    );
    draw_cycle_chip(
        st,
        dl,
        atlas,
        Rect::new(x + w - 168.0, y + 6.0, 164.0, 28.0),
        "Mode",
        st.config.perf_profile.label(),
        FocusId::PerfProfileCycle,
        focusables,
    );
    y += 50.0;
    draw_hairline(dl, x, y, w);

    y += 8.0;
    y = draw_section_label(dl, atlas, x, y, w, "CONFIGURATION");

    dl.text(
        atlas,
        FontKind::UiSemi,
        x + 4.0,
        y + 4.0,
        "Save or Reset Settings",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        x + 4.0,
        y + 23.0,
        "Settings auto-save on change. Reset restores default configuration.",
        palette::TEXT_SECONDARY,
    );

    let reset_all = Rect::new(x + w - 112.0, y + 7.0, 108.0, 28.0);
    let save_now = Rect::new(reset_all.x - 106.0, y + 7.0, 98.0, 28.0);
    draw_action_button(
        st,
        dl,
        atlas,
        save_now,
        "Save Now",
        true,
        FocusId::SystemSaveConfig,
        focusables,
    );
    draw_action_button(
        st,
        dl,
        atlas,
        reset_all,
        "Reset All",
        false,
        FocusId::SystemResetAll,
        focusables,
    );
    y += 52.0;
    draw_hairline(dl, x, y, w);

    y += 8.0;
    y = draw_section_label(dl, atlas, x, y, w, "ENVIRONMENT");

    let rows = [
        ("GPU", st.gpu_name.clone()),
        ("Resolution", format!("{} x {}", st.screen_size[0], st.screen_size[1])),
        (
            "Fullbright Hook",
            if st.lightmap_hooked {
                String::from("Active")
            } else {
                String::from("Waiting for world")
            },
        ),
        (
            "Game Link",
            if st.game.socket_connected {
                String::from("Connected")
            } else {
                String::from("Standby")
            },
        ),
        ("Menu Keys", String::from("~ or ` to toggle · Esc to close")),
        ("Config Path", KestrelConfig::config_path().display().to_string()),
    ];

    for (k, v) in rows {
        dl.text(atlas, FontKind::UiSmall, x + 4.0, y + 4.0, k, palette::TEXT_SECONDARY);
        dl.text(atlas, FontKind::MonoSmall, x + 144.0, y + 4.0, &v, palette::TEXT_PRIMARY);
        y += 22.0;
    }

    y + 8.0
}

fn render_search_results(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    q: &str,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    y = draw_section_label(dl, atlas, x, y, w, "SEARCH RESULTS");

    let items: [(&str, &str, bool, FocusId); 28] = [
        ("Fullbright", "Brightens caves and night areas", st.config.fullbright_enabled, FocusId::FullbrightToggle),
        ("Ore X-Ray", "3D boxes for buried Diamond, Debris, Gold, Iron", st.config.xray_ores, FocusId::XrayOresToggle),
        ("Cave Wireframe", "See underground tunnels and lava pockets", st.config.xray_terrain, FocusId::XrayTerrainToggle),
        ("Chest & Mob Through-Walls", "Show chests and mobs behind walls", st.config.xray_entities, FocusId::XrayEntitiesToggle),
        ("Entity ESP Boxes", "Draw boxes around players, mobs, and loot", st.config.esp_enabled, FocusId::EspToggle),
        ("Tracers", "Guide lines to nearby players and mobs", st.config.tracers_enabled, FocusId::TracersToggle),
        ("Freecam", "Scout freely while your character stays still", st.config.freecam_enabled, FocusId::FreecamToggle),
        ("AimAssist", "Gently aims toward nearby targets", st.config.aim_assist, FocusId::AimAssistToggle),
        ("KillAura", "Automatically turns and attacks nearby targets", st.config.killaura_enabled, FocusId::KillAuraToggle),
        ("Criticals", "Micro-jump on attack for 1.5x critical hits", st.config.criticals_enabled, FocusId::CriticalsToggle),
        ("Entity Radar", "Top-down minimap of players and mobs", st.config.hud_radar.enabled, FocusId::HudToggleRadar),
        ("AutoSprint", "Keeps sprint active while moving forward", st.config.auto_sprint, FocusId::AutoSprintToggle),
        ("AutoRespawn", "Respawns automatically when you die", st.config.auto_respawn, FocusId::AutoRespawnToggle),
        ("AutoEat", "Eats hotbar food when hunger gets low", st.config.auto_eat, FocusId::AutoEatToggle),
        ("AutoTotem", "Equips Totem of Undying into offhand", st.config.auto_totem, FocusId::AutoTotemToggle),
        ("AutoFish", "Auto-reels and recasts fishing rod", st.config.auto_fish, FocusId::AutoFishToggle),
        ("TriggerBot", "Strikes the target under your crosshair", st.config.trigger_bot, FocusId::TriggerBotToggle),
        ("AutoClicker", "Clicks continuously while holding attack", st.config.auto_clicker, FocusId::AutoClickerToggle),
        ("AutoMine", "Automatically holds mine on aimed blocks", st.config.auto_mine, FocusId::AutoMineToggle),
        ("SpeedMine", "Breaks blocks faster with minimal delay", st.config.speed_mine, FocusId::SpeedMineToggle),
        ("AutoTool", "Switches hotbar slot to the best mining tool", st.config.auto_tool, FocusId::AutoToolToggle),
        ("AntiAFK", "Taps sneak when idle to avoid AFK kick", st.config.anti_afk, FocusId::AntiAfkToggle),
        ("OptiZoom", "Hold key to zoom in", st.config.zoom_enabled, FocusId::ZoomToggle),
        ("Crosshair+", "Custom reticle overlay", st.config.crosshair_enabled, FocusId::CrosshairToggle),
        ("Coordinates HUD", "Coordinates and Nether portal converter", st.config.hud_coords.enabled, FocusId::HudToggleCoords),
        ("FPS & Visual Status", "FPS and active visual modes", st.config.hud_fps.enabled, FocusId::HudToggleFps),
        ("Keystrokes & CPS", "Live movement keys and click speed", st.config.hud_keystrokes.enabled, FocusId::HudToggleKeystrokes),
        ("Waypoint Compass", "Directional compass for saved markers", st.config.hud_compass.enabled, FocusId::HudToggleCompass),
    ];

    let mut matched = 0usize;
    for (name, desc, on, fid) in items {
        if name.to_lowercase().contains(q) || desc.to_lowercase().contains(q) {
            matched += 1;
            y = draw_module_row(st, dl, atlas, x, y, w, name, desc, on, fid, focusables);
        }
    }

    for (idx, wp) in st.config.waypoints.iter().enumerate() {
        if wp.name.to_lowercase().contains(q) {
            matched += 1;
            let coords = format!("Waypoint · {}, {}, {}", wp.x, wp.y, wp.z);
            y = draw_module_row(
                st,
                dl,
                atlas,
                x,
                y,
                w,
                &wp.name,
                &coords,
                wp.enabled,
                FocusId::WaypointItemToggle(idx),
                focusables,
            );
        }
    }

    if matched == 0 {
        dl.text(
            atlas,
            FontKind::UiSmall,
            x + 4.0,
            y + 8.0,
            "No matching modules or waypoints. Clear search to return.",
            palette::TEXT_SECONDARY,
        );
        y += 32.0;
    }

    y
}

// ---------------------------------------------------------
// Crafted Minimal Components (Switches, Hairlines, Strips)
// ---------------------------------------------------------

fn draw_section_label(
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    y: f32,
    w: f32,
    title: &str,
) -> f32 {
    dl.text(
        atlas,
        FontKind::MonoSmall,
        x + 2.0,
        y,
        title,
        palette::ACCENT.with_alpha(0.9),
    );
    let tw = atlas.measure(FontKind::MonoSmall, title);
    dl.rect(
        Rect::new(x + tw + 12.0, y + 6.0, (w - tw - 12.0).max(0.0), 1.0),
        0.0,
        palette::BORDER.with_alpha(0.45),
    );
    y + 22.0
}

fn draw_hairline(dl: &mut DrawList, x: f32, y: f32, w: f32) {
    dl.rect(Rect::new(x + 4.0, y, w - 8.0, 1.0), 0.0, palette::BORDER.with_alpha(0.32));
}

#[allow(clippy::too_many_arguments)]
fn draw_module_row(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    y: f32,
    w: f32,
    title: &str,
    desc: &str,
    on: bool,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    let row = Rect::new(x, y, w, 46.0);
    let hovered = row.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(fid);

    if hovered {
        dl.rect(row, 6.0, palette::SURFACE.with_alpha(0.72));
    }
    if focused {
        dl.rect_border(row, 6.0, Color::hex(0x000000, 0.0), 1.0, palette::ACCENT);
    }

    dl.text(
        atlas,
        FontKind::UiSemi,
        row.x + 8.0,
        row.y + 6.0,
        title,
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        row.x + 8.0,
        row.y + 25.0,
        desc,
        palette::TEXT_SECONDARY,
    );

    let sw_r = Rect::new(row.x + row.w - 44.0, row.y + 13.0, 36.0, 20.0);
    draw_switch_visual(dl, sw_r, on, hovered || focused);
    focusables.push(Focusable {
        id: fid,
        rect: row,
        kind: ControlKind::Toggle,
    });

    let next_y = y + 46.0;
    draw_hairline(dl, x, next_y - 0.5, w);
    next_y
}

fn draw_inset_bg(
    dl: &mut DrawList,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> (f32, f32, f32, f32) {
    let strip = Rect::new(x + 8.0, y + 4.0, w - 16.0, h);
    dl.rect(strip, 6.0, palette::SURFACE.with_alpha(0.68));
    dl.rect(
        Rect::new(strip.x, strip.y + 6.0, 2.0, strip.h - 12.0),
        1.0,
        palette::ACCENT.with_alpha(0.75),
    );
    (strip.x + 12.0, strip.y, strip.w - 22.0, y + h + 8.0)
}

fn draw_switch_visual(dl: &mut DrawList, r: Rect, on: bool, highlight: bool) {
    let track_col = if on {
        palette::ACCENT
    } else if highlight {
        palette::SURFACE_HOVER
    } else {
        Color::hex(0x232934, 1.0)
    };
    let border_col = if on {
        palette::ACCENT
    } else {
        palette::BORDER_STRONG.with_alpha(0.65)
    };
    dl.rect_border(r, r.h * 0.5, track_col, 1.0, border_col);

    let knob_sz = r.h - 6.0;
    let kx = if on {
        r.x + r.w - knob_sz - 3.0
    } else {
        r.x + 3.0
    };
    let knob_col = if on {
        palette::CANVAS_SOLID
    } else {
        palette::TEXT_PRIMARY.with_alpha(0.9)
    };
    dl.rect(
        Rect::new(kx, r.y + 3.0, knob_sz, knob_sz),
        knob_sz * 0.5,
        knob_col,
    );
}

fn draw_switch(
    st: &AppState,
    dl: &mut DrawList,
    r: Rect,
    on: bool,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    let hovered = r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(fid);
    draw_switch_visual(dl, r, on, hovered || focused);
    focusables.push(Focusable {
        id: fid,
        rect: r,
        kind: ControlKind::Toggle,
    });
}

fn draw_top_pill(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    label: &str,
    danger: bool,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    let hovered = r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(fid);
    let bg = if danger && hovered {
        palette::DANGER.with_alpha(0.22)
    } else if hovered {
        palette::SURFACE_HOVER
    } else {
        palette::SURFACE.with_alpha(0.8)
    };
    let fg = if danger && hovered {
        palette::DANGER
    } else {
        palette::TEXT_PRIMARY
    };
    let border = if focused {
        palette::ACCENT
    } else if danger && hovered {
        palette::DANGER.with_alpha(0.75)
    } else {
        palette::BORDER.with_alpha(0.8)
    };
    dl.rect_border(r, 6.0, bg, 1.0, border);
    let tw = atlas.measure(FontKind::UiSmall, label);
    dl.text(
        atlas,
        FontKind::UiSmall,
        r.x + (r.w - tw) * 0.5,
        r.y + 7.0,
        label,
        fg,
    );
    focusables.push(Focusable {
        id: fid,
        rect: r,
        kind: ControlKind::Button,
    });
}

fn draw_close_button(
    st: &AppState,
    dl: &mut DrawList,
    _atlas: &FontAtlas,
    r: Rect,
    focusables: &mut Vec<Focusable>,
) {
    let hovered = r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(FocusId::CloseBtn);
    let bg = if hovered {
        palette::SURFACE_HOVER
    } else {
        Color::hex(0x000000, 0.0)
    };
    if hovered || focused {
        dl.rect_border(
            r,
            6.0,
            bg,
            1.0,
            if focused { palette::ACCENT } else { palette::BORDER },
        );
    }
    let col = if hovered {
        palette::TEXT_PRIMARY
    } else {
        palette::TEXT_SECONDARY
    };
    let cx = r.x + r.w * 0.5;
    let cy = r.y + r.h * 0.5;
    dl.line([cx - 4.5, cy - 4.5], [cx + 4.5, cy + 4.5], 1.6, col);
    dl.line([cx + 4.5, cy - 4.5], [cx - 4.5, cy + 4.5], 1.6, col);
    focusables.push(Focusable {
        id: FocusId::CloseBtn,
        rect: r,
        kind: ControlKind::Button,
    });
}

fn draw_action_button(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    label: &str,
    primary: bool,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    draw_action_button_enabled(st, dl, atlas, r, label, primary, true, fid, focusables);
}

fn draw_action_button_enabled(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    label: &str,
    primary: bool,
    enabled: bool,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    let hovered = enabled && r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(fid);
    let (bg, fg, border) = if !enabled {
        (palette::SURFACE, palette::TEXT_SECONDARY.with_alpha(0.5), palette::BORDER)
    } else if primary {
        let b = if hovered { Color::hex(0xF2B94F, 1.0) } else { palette::ACCENT };
        (b, palette::CANVAS_SOLID, if focused { palette::TEXT_PRIMARY } else { b })
    } else {
        let b = if hovered { palette::SURFACE_HOVER } else { palette::SURFACE };
        (
            b,
            palette::TEXT_PRIMARY,
            if focused { palette::ACCENT } else { palette::BORDER_STRONG.with_alpha(0.7) },
        )
    };
    dl.rect_border(r, 6.0, bg, 1.0, border);
    let tw = atlas.measure(FontKind::UiSmall, label);
    dl.text(
        atlas,
        FontKind::UiSmall,
        r.x + (r.w - tw) * 0.5,
        r.y + (r.h - 14.0) * 0.5,
        label,
        fg,
    );
    if enabled {
        focusables.push(Focusable {
            id: fid,
            rect: r,
            kind: ControlKind::Button,
        });
    }
}

fn draw_segment_button(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    label: &str,
    active: bool,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    let hovered = r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(fid);
    let bg = if active {
        palette::ACCENT
    } else if hovered {
        palette::SURFACE_HOVER
    } else {
        palette::CANVAS_SOLID.with_alpha(0.7)
    };
    let border = if focused {
        palette::TEXT_PRIMARY
    } else if active {
        palette::ACCENT
    } else {
        palette::BORDER.with_alpha(0.8)
    };
    dl.rect_border(r, 5.0, bg, 1.0, border);
    let tw = atlas.measure(FontKind::UiSmall, label);
    dl.text(
        atlas,
        FontKind::UiSmall,
        r.x + (r.w - tw) * 0.5,
        r.y + (r.h - 14.0) * 0.5,
        label,
        if active {
            palette::CANVAS_SOLID
        } else {
            palette::TEXT_PRIMARY
        },
    );
    focusables.push(Focusable {
        id: fid,
        rect: r,
        kind: ControlKind::Button,
    });
}

fn draw_cycle_chip(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    label: &str,
    value: &str,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    let hovered = r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(fid);
    let bg = if hovered {
        palette::SURFACE_HOVER
    } else {
        palette::CANVAS_SOLID.with_alpha(0.75)
    };
    let border = if focused {
        palette::ACCENT
    } else {
        palette::BORDER.with_alpha(0.85)
    };
    dl.rect_border(r, 5.0, bg, 1.0, border);
    dl.text(atlas, FontKind::UiSmall, r.x + 8.0, r.y + 6.0, label, palette::TEXT_SECONDARY);
    let vw = atlas.measure(FontKind::UiSmall, value);
    dl.text(
        atlas,
        FontKind::UiSmall,
        r.x + r.w - vw - 8.0,
        r.y + 6.0,
        value,
        palette::ACCENT,
    );
    focusables.push(Focusable {
        id: fid,
        rect: r,
        kind: ControlKind::Button,
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_slider(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    label: &str,
    val_text: &str,
    val: f32,
    min: f32,
    max: f32,
    step: f32,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    dl.text(atlas, FontKind::UiSmall, r.x, r.y + 1.0, label, palette::TEXT_SECONDARY);
    let vw = atlas.measure(FontKind::MonoSmall, val_text);
    dl.text(
        atlas,
        FontKind::MonoSmall,
        r.x + r.w - vw,
        r.y + 1.0,
        val_text,
        palette::TEXT_PRIMARY,
    );

    let track = Rect::new(r.x, r.y + 23.0, r.w, 4.0);
    let focused = st.focused == Some(fid) || st.active_slider == Some(fid);
    let t = ((val - min) / (max - min).max(0.0001)).clamp(0.0, 1.0);
    dl.rect(track, 2.0, Color::hex(0x232A36, 1.0));
    let fill_w = (track.w * t).max(4.0);
    dl.rect(Rect::new(track.x, track.y, fill_w, track.h), 2.0, palette::ACCENT);

    let knob_x = (track.x + track.w * t - 6.0).clamp(track.x, track.x + track.w - 12.0);
    dl.rect_border(
        Rect::new(knob_x, track.y - 4.0, 12.0, 12.0),
        6.0,
        palette::TEXT_PRIMARY,
        if focused { 1.8 } else { 1.0 },
        if focused { palette::ACCENT } else { palette::CANVAS_SOLID },
    );

    focusables.push(Focusable {
        id: fid,
        rect: Rect::new(r.x, r.y + 12.0, r.w, 24.0),
        kind: ControlKind::Slider(min, max, step),
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_text_input(
    _st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    text: &str,
    placeholder: &str,
    editing: bool,
    focused: bool,
    fid: FocusId,
    target: TextTarget,
    focusables: &mut Vec<Focusable>,
) {
    let border = if editing || focused {
        palette::ACCENT
    } else {
        palette::BORDER.with_alpha(0.8)
    };
    dl.rect_border(
        r,
        6.0,
        palette::CANVAS_SOLID.with_alpha(0.75),
        1.0,
        border,
    );
    dl.push_scissor(Rect::new(r.x + 8.0, r.y + 2.0, r.w - 16.0, r.h - 4.0));
    if text.is_empty() {
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 9.0,
            r.y + 7.0,
            placeholder,
            palette::TEXT_SECONDARY.with_alpha(0.8),
        );
    } else {
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 9.0,
            r.y + 7.0,
            text,
            palette::TEXT_PRIMARY,
        );
        if editing {
            let cw = atlas.measure(FontKind::UiSmall, text);
            dl.rect(
                Rect::new(r.x + 10.0 + cw, r.y + 6.0, 1.5, r.h - 12.0),
                0.5,
                palette::ACCENT,
            );
        }
    }
    dl.pop_scissor();
    focusables.push(Focusable {
        id: fid,
        rect: r,
        kind: ControlKind::Text(target),
    });
}

// ---------------------------------------------------------
// Input & Activation Handling
// ---------------------------------------------------------

fn process_interactions(st: &mut AppState, focusables: &[Focusable]) {
    let [mx, my] = st.cursor_pos;

    if st.mouse_just_pressed && st.dragging_menu.is_none() {
        let mut hit_text = None;
        for f in focusables {
            if f.rect.contains(mx, my) {
                st.focused = Some(f.id);
                match f.kind {
                    ControlKind::Slider(min, max, step) => {
                        st.active_slider = Some(f.id);
                        apply_slider_pos(st, f.id, f.rect, mx, min, max, step);
                    }
                    ControlKind::Text(target) => {
                        hit_text = Some(target);
                    }
                    ControlKind::Button | ControlKind::Toggle => {
                        activate_control(st, f.id);
                    }
                }
                break;
            }
        }
        st.active_text_target = hit_text;
    }

    if st.mouse_down
        && let Some(sid) = st.active_slider
        && let Some(f) = focusables.iter().find(|x| x.id == sid)
        && let ControlKind::Slider(min, max, step) = f.kind
    {
        apply_slider_pos(st, sid, f.rect, mx, min, max, step);
    }

    if st.mouse_just_released {
        if st.active_slider.is_some() {
            let _ = st.config.save();
        }
        st.active_slider = None;
    }

    let chars: Vec<char> = st.pending_chars.drain(..).collect();
    if let Some(target) = st.active_text_target {
        for ch in chars {
            if !ch.is_control() {
                match target {
                    TextTarget::Search => st.search_query.push(ch),
                    TextTarget::WaypointName if st.waypoint_draft_name.len() < 32 => {
                        st.waypoint_draft_name.push(ch);
                    }
                    _ => {}
                }
            }
        }
    }

    let keys: Vec<UiKey> = st.pending_keys.drain(..).collect();
    for key in keys {
        match key {
            UiKey::Backspace => {
                if let Some(target) = st.active_text_target {
                    match target {
                        TextTarget::Search => {
                            st.search_query.pop();
                        }
                        TextTarget::WaypointName => {
                            st.waypoint_draft_name.pop();
                        }
                    }
                }
            }
            UiKey::Tab if !focusables.is_empty() => {
                let cur = st
                    .focused
                    .and_then(|id| focusables.iter().position(|f| f.id == id));
                let next = match cur {
                    Some(i) => (i + 1) % focusables.len(),
                    None => 0,
                };
                st.focused = Some(focusables[next].id);
                st.active_text_target = match focusables[next].kind {
                    ControlKind::Text(t) => Some(t),
                    _ => None,
                };
            }
            UiKey::ShiftTab if !focusables.is_empty() => {
                let cur = st
                    .focused
                    .and_then(|id| focusables.iter().position(|f| f.id == id));
                let prev = match cur {
                    Some(0) | None => focusables.len() - 1,
                    Some(i) => i - 1,
                };
                st.focused = Some(focusables[prev].id);
                st.active_text_target = match focusables[prev].kind {
                    ControlKind::Text(t) => Some(t),
                    _ => None,
                };
            }
            UiKey::Tab | UiKey::ShiftTab => {}
            UiKey::Enter | UiKey::Space => {
                if key == UiKey::Space && st.active_text_target.is_some() {
                    if let Some(TextTarget::Search) = st.active_text_target {
                        st.search_query.push(' ');
                    } else if let Some(TextTarget::WaypointName) = st.active_text_target
                        && st.waypoint_draft_name.len() < 32
                    {
                        st.waypoint_draft_name.push(' ');
                    }
                    continue;
                }
                if key == UiKey::Enter && st.active_text_target == Some(TextTarget::WaypointName) {
                    activate_control(st, FocusId::WaypointAddHere);
                    continue;
                }
                if let Some(fid) = st.focused {
                    activate_control(st, fid);
                }
            }
            UiKey::Left | UiKey::Right => {
                if let Some(fid) = st.focused
                    && let Some(f) = focusables.iter().find(|x| x.id == fid)
                    && let ControlKind::Slider(min, max, step) = f.kind
                {
                    let dir = if key == UiKey::Right { 1.0 } else { -1.0 };
                    nudge_slider(st, fid, dir * step, min, max);
                    let _ = st.config.save();
                }
            }
            UiKey::Escape => {
                if st.active_text_target.is_some() {
                    st.active_text_target = None;
                } else {
                    st.menu_open = false;
                }
            }
            UiKey::Up => {
                st.scroll_y = (st.scroll_y - 36.0).clamp(0.0, st.max_scroll_y);
            }
            UiKey::Down => {
                st.scroll_y = (st.scroll_y + 36.0).clamp(0.0, st.max_scroll_y);
            }
        }
    }
}

fn apply_slider_pos(
    st: &mut AppState,
    fid: FocusId,
    rect: Rect,
    mx: f32,
    min: f32,
    max: f32,
    step: f32,
) {
    let t = ((mx - rect.x) / rect.w.max(1.0)).clamp(0.0, 1.0);
    let raw = min + t * (max - min);
    let snapped = ((raw / step).round() * step).clamp(min, max);
    match fid {
        FocusId::FullbrightStrength => {
            st.config.fullbright_strength = snapped;
            st.lightmap_dirty = true;
        }
        FocusId::ZoomFovSlider => {
            st.config.zoom_fov = snapped.round() as u32;
        }
        FocusId::CrosshairSizeSlider => {
            st.config.crosshair_size = snapped;
        }
        FocusId::CrosshairGapSlider => {
            st.config.crosshair_gap = snapped;
        }
        FocusId::GammaSlider => {
            st.game.horizon_gamma = Some(snapped);
            send_cmd(ControlCmd::SetGamma(snapped));
        }
        FocusId::AutoEatHungerSlider => {
            st.config.auto_eat_hunger = snapped.round() as i32;
        }
        FocusId::TriggerBotDelaySlider => {
            st.config.trigger_bot_delay_ms = snapped.round() as u32;
        }
        FocusId::XrayRadiusSlider => {
            st.config.xray_radius = snapped.round() as i32;
        }
        FocusId::FreecamSpeedSlider => {
            st.config.freecam_speed = snapped;
        }
        FocusId::AimFovSlider => {
            st.config.aim_fov = snapped;
        }
        FocusId::AimRangeSlider => {
            st.config.aim_range = snapped;
        }
        FocusId::AimSmoothSlider => {
            st.config.aim_smooth = snapped;
        }
        FocusId::KillAuraRangeSlider => {
            st.config.killaura_range = snapped;
        }
        FocusId::KillAuraFovSlider => {
            st.config.killaura_fov = snapped;
        }
        FocusId::KillAuraDelaySlider => {
            st.config.killaura_delay_ms = snapped.round() as u32;
        }
        FocusId::AutoClickerCpsSlider => {
            st.config.auto_clicker_cps = snapped.round() as u32;
        }
        FocusId::SpeedMinePowerSlider => {
            st.config.speed_mine_power = snapped.round() as u32;
        }
        _ => {}
    }
}

fn nudge_slider(st: &mut AppState, fid: FocusId, delta: f32, min: f32, max: f32) {
    match fid {
        FocusId::FullbrightStrength => {
            st.config.fullbright_strength = (st.config.fullbright_strength + delta).clamp(min, max);
            st.lightmap_dirty = true;
        }
        FocusId::ZoomFovSlider => {
            let cur = st.config.zoom_fov as f32 + delta;
            st.config.zoom_fov = cur.clamp(min, max).round() as u32;
        }
        FocusId::CrosshairSizeSlider => {
            st.config.crosshair_size = (st.config.crosshair_size + delta).clamp(min, max);
        }
        FocusId::CrosshairGapSlider => {
            st.config.crosshair_gap = (st.config.crosshair_gap + delta).clamp(min, max);
        }
        FocusId::GammaSlider => {
            let cur = st.game.horizon_gamma.unwrap_or(0.5);
            let next = (cur + delta).clamp(min, max);
            st.game.horizon_gamma = Some(next);
            send_cmd(ControlCmd::SetGamma(next));
        }
        FocusId::AutoEatHungerSlider => {
            let next = (st.config.auto_eat_hunger as f32 + delta).clamp(min, max);
            st.config.auto_eat_hunger = next.round() as i32;
        }
        FocusId::TriggerBotDelaySlider => {
            let next = (st.config.trigger_bot_delay_ms as f32 + delta).clamp(min, max);
            st.config.trigger_bot_delay_ms = next.round() as u32;
        }
        FocusId::XrayRadiusSlider => {
            let next = (st.config.xray_radius as f32 + delta).clamp(min, max);
            st.config.xray_radius = next.round() as i32;
        }
        FocusId::FreecamSpeedSlider => {
            st.config.freecam_speed = (st.config.freecam_speed + delta).clamp(min, max);
        }
        FocusId::AimFovSlider => {
            st.config.aim_fov = (st.config.aim_fov + delta).clamp(min, max);
        }
        FocusId::AimRangeSlider => {
            st.config.aim_range = (st.config.aim_range + delta).clamp(min, max);
        }
        FocusId::AimSmoothSlider => {
            st.config.aim_smooth = (st.config.aim_smooth + delta).clamp(min, max);
        }
        FocusId::KillAuraRangeSlider => {
            st.config.killaura_range = (st.config.killaura_range + delta).clamp(min, max);
        }
        FocusId::KillAuraFovSlider => {
            st.config.killaura_fov = (st.config.killaura_fov + delta).clamp(min, max);
        }
        FocusId::KillAuraDelaySlider => {
            let next = (st.config.killaura_delay_ms as f32 + delta).clamp(min, max);
            st.config.killaura_delay_ms = next.round() as u32;
        }
        FocusId::AutoClickerCpsSlider => {
            let next = (st.config.auto_clicker_cps as f32 + delta).clamp(min, max);
            st.config.auto_clicker_cps = next.round() as u32;
        }
        FocusId::SpeedMinePowerSlider => {
            let next = (st.config.speed_mine_power as f32 + delta).clamp(min, max);
            st.config.speed_mine_power = next.round() as u32;
        }
        _ => {}
    }
}

fn activate_control(st: &mut AppState, fid: FocusId) {
    let mut save = true;
    match fid {
        FocusId::CloseBtn => {
            st.menu_open = false;
            save = false;
        }
        FocusId::EditHudBtn | FocusId::HudEnterEdit => {
            st.menu_open = false;
            st.hud_edit_mode = true;
            save = false;
        }
        FocusId::PanicBtn => {
            st.config.fullbright_enabled = false;
            st.config.xray_terrain = false;
            st.config.xray_entities = false;
            st.config.xray_ores = false;
            st.config.esp_enabled = false;
            st.config.esp_chests = false;
            st.config.tracers_enabled = false;
            st.config.criticals_enabled = false;
            st.config.freecam_enabled = false;
            st.config.aim_assist = false;
            st.config.killaura_enabled = false;
            st.freecam_offset = [0.0; 3];
            st.config.auto_sprint = false;
            st.config.auto_respawn = false;
            st.config.auto_eat = false;
            st.config.auto_totem = false;
            st.config.auto_fish = false;
            st.config.trigger_bot = false;
            st.config.auto_clicker = false;
            st.config.anti_afk = false;
            st.config.auto_mine = false;
            st.config.speed_mine = false;
            st.config.auto_tool = false;
            st.lightmap_dirty = true;
            st.set_toast("Panic: all modules disabled");
        }
        FocusId::TabAutomation => {
            st.active_tab = NavTab::Automation;
            st.scroll_y = 0.0;
            st.search_query.clear();
            save = false;
        }
        FocusId::TabSurvival => {
            st.active_tab = NavTab::Survival;
            st.scroll_y = 0.0;
            st.search_query.clear();
            save = false;
        }
        FocusId::TabHud => {
            st.active_tab = NavTab::Hud;
            st.scroll_y = 0.0;
            st.search_query.clear();
            save = false;
        }
        FocusId::TabWaypoints => {
            st.active_tab = NavTab::Waypoints;
            st.scroll_y = 0.0;
            st.search_query.clear();
            save = false;
        }
        FocusId::TabSystem => {
            st.active_tab = NavTab::System;
            st.scroll_y = 0.0;
            st.search_query.clear();
            save = false;
        }
        FocusId::FullbrightToggle | FocusId::QuickFullbright => {
            st.config.fullbright_enabled = !st.config.fullbright_enabled;
            st.lightmap_dirty = true;
        }
        FocusId::FullbrightModeMax => {
            st.config.fullbright_mode = FullbrightMode::Max;
            st.config.fullbright_enabled = true;
            st.lightmap_dirty = true;
        }
        FocusId::FullbrightModeSoft => {
            st.config.fullbright_mode = FullbrightMode::Soft;
            st.config.fullbright_enabled = true;
            st.lightmap_dirty = true;
        }
        FocusId::XrayTerrainToggle => {
            st.config.xray_terrain = !st.config.xray_terrain;
        }
        FocusId::XrayEntitiesToggle => {
            st.config.xray_entities = !st.config.xray_entities;
        }
        FocusId::XrayOresToggle => {
            st.config.xray_ores = !st.config.xray_ores;
        }
        FocusId::XrayOrePresetCycle => {
            st.config.xray_ore_preset = match st.config.xray_ore_preset {
                OrePreset::Valuable => OrePreset::All,
                OrePreset::All => OrePreset::Valuable,
            };
        }
        FocusId::EspToggle => {
            st.config.esp_enabled = !st.config.esp_enabled;
        }
        FocusId::TracersToggle => {
            st.config.tracers_enabled = !st.config.tracers_enabled;
        }
        FocusId::EspPlayersToggle => {
            st.config.esp_players = !st.config.esp_players;
        }
        FocusId::EspHostileToggle => {
            st.config.esp_hostile = !st.config.esp_hostile;
        }
        FocusId::EspPassiveToggle => {
            st.config.esp_passive = !st.config.esp_passive;
        }
        FocusId::EspItemsToggle => {
            st.config.esp_items = !st.config.esp_items;
        }
        FocusId::EspChestsToggle => {
            st.config.esp_chests = !st.config.esp_chests;
        }
        FocusId::FreecamToggle => {
            st.config.freecam_enabled = !st.config.freecam_enabled;
            if !st.config.freecam_enabled {
                st.freecam_offset = [0.0; 3];
            }
        }
        FocusId::CriticalsToggle => {
            st.config.criticals_enabled = !st.config.criticals_enabled;
        }
        FocusId::AimAssistToggle => {
            st.config.aim_assist = !st.config.aim_assist;
        }
        FocusId::KillAuraToggle => {
            st.config.killaura_enabled = !st.config.killaura_enabled;
        }
        FocusId::AutoMineToggle => {
            st.config.auto_mine = !st.config.auto_mine;
        }
        FocusId::SpeedMineToggle => {
            st.config.speed_mine = !st.config.speed_mine;
        }
        FocusId::AutoToolToggle => {
            st.config.auto_tool = !st.config.auto_tool;
        }
        FocusId::SpeedMinePowerSlider => {}
        FocusId::AimTargetCycle => {
            st.config.aim_target = cycle_aim_target(st.config.aim_target);
        }
        FocusId::KillAuraTargetCycle => {
            st.config.killaura_target = cycle_aim_target(st.config.killaura_target);
        }
        FocusId::XrayKeyCycle => {
            st.config.xray_key = match st.config.xray_key {
                XrayKey::X => XrayKey::B,
                XrayKey::B => XrayKey::None,
                XrayKey::None => XrayKey::X,
            };
        }
        FocusId::ZoomToggle => {
            st.config.zoom_enabled = !st.config.zoom_enabled;
        }
        FocusId::ZoomKeyCycle => {
            st.config.zoom_key = match st.config.zoom_key {
                ZoomKey::C => ZoomKey::V,
                ZoomKey::V => ZoomKey::Alt,
                ZoomKey::Alt => ZoomKey::C,
            };
        }
        FocusId::CrosshairToggle => {
            st.config.crosshair_enabled = !st.config.crosshair_enabled;
        }
        FocusId::CrosshairStyleCycle => {
            st.config.crosshair_style = match st.config.crosshair_style {
                CrosshairStyle::Cross => CrosshairStyle::Dot,
                CrosshairStyle::Dot => CrosshairStyle::TBar,
                CrosshairStyle::TBar => CrosshairStyle::Cross,
            };
        }
        FocusId::HudResetLayout => {
            st.config.reset_hud_positions();
            st.set_toast("HUD positions reset");
        }
        FocusId::HudToggleCoords => st.config.hud_coords.enabled = !st.config.hud_coords.enabled,
        FocusId::HudToggleNether => {
            st.config.hud_nether_convert = !st.config.hud_nether_convert
        }
        FocusId::HudToggleFps => st.config.hud_fps.enabled = !st.config.hud_fps.enabled,
        FocusId::HudToggleKeystrokes => {
            st.config.hud_keystrokes.enabled = !st.config.hud_keystrokes.enabled
        }
        FocusId::HudToggleVitals => st.config.hud_vitals.enabled = !st.config.hud_vitals.enabled,
        FocusId::HudToggleCompass => st.config.hud_compass.enabled = !st.config.hud_compass.enabled,
        FocusId::HudToggleClock => st.config.hud_clock.enabled = !st.config.hud_clock.enabled,
        FocusId::HudToggleRadar => st.config.hud_radar.enabled = !st.config.hud_radar.enabled,
        FocusId::AutoSprintToggle => st.config.auto_sprint = !st.config.auto_sprint,
        FocusId::AutoRespawnToggle => st.config.auto_respawn = !st.config.auto_respawn,
        FocusId::AutoEatToggle => st.config.auto_eat = !st.config.auto_eat,
        FocusId::AutoTotemToggle => st.config.auto_totem = !st.config.auto_totem,
        FocusId::AutoFishToggle => st.config.auto_fish = !st.config.auto_fish,
        FocusId::TriggerBotToggle => st.config.trigger_bot = !st.config.trigger_bot,
        FocusId::AutoClickerToggle => st.config.auto_clicker = !st.config.auto_clicker,
        FocusId::AntiAfkToggle => st.config.anti_afk = !st.config.anti_afk,
        FocusId::WaypointAddHere => {
            if st.game.in_world {
                let raw = st.waypoint_draft_name.trim();
                let name = if raw.is_empty() {
                    format!("Point {}", st.config.waypoints.len() + 1)
                } else {
                    raw.to_string()
                };
                let color_idx = (st.config.waypoints.len() % 5) as u8;
                st.config.waypoints.push(Waypoint {
                    name: name.clone(),
                    x: st.game.pos[0].floor() as i32,
                    y: st.game.pos[1].floor() as i32,
                    z: st.game.pos[2].floor() as i32,
                    dimension: st.game.dimension.clone(),
                    color_idx,
                    enabled: true,
                });
                st.waypoint_draft_name.clear();
                st.set_toast(format!("Waypoint \"{name}\" saved"));
            } else {
                st.set_toast("Enter a world first to save coordinates");
                save = false;
            }
        }
        FocusId::WaypointItemToggle(idx) => {
            if let Some(wp) = st.config.waypoints.get_mut(idx) {
                wp.enabled = !wp.enabled;
            }
        }
        FocusId::WaypointItemDelete(idx) => {
            if idx < st.config.waypoints.len() {
                st.config.waypoints.remove(idx);
                st.set_toast("Waypoint removed");
            }
        }
        FocusId::SystemSaveConfig => {
            match st.config.save() {
                Ok(()) => st.set_toast("Configuration saved"),
                Err(e) => st.set_toast(format!("Save error: {e}")),
            }
            save = false;
        }
        FocusId::PerfProfileCycle => {
            st.config.perf_profile = match st.config.perf_profile {
                PerfProfile::Full => PerfProfile::Balanced,
                PerfProfile::Balanced => PerfProfile::Lite,
                PerfProfile::Lite => PerfProfile::Full,
            };
        }
        FocusId::SystemResetAll => {
            st.config = KestrelConfig::default();
            st.lightmap_dirty = true;
            st.set_toast("Defaults restored");
        }
        FocusId::SearchBox
        | FocusId::WaypointInput
        | FocusId::FullbrightStrength
        | FocusId::ZoomFovSlider
        | FocusId::CrosshairSizeSlider
        | FocusId::CrosshairGapSlider
        | FocusId::GammaSlider
        | FocusId::XrayRadiusSlider
        | FocusId::FreecamSpeedSlider
        | FocusId::AimFovSlider
        | FocusId::AimRangeSlider
        | FocusId::AimSmoothSlider
        | FocusId::KillAuraRangeSlider
        | FocusId::KillAuraFovSlider
        | FocusId::KillAuraDelaySlider
        | FocusId::AutoEatHungerSlider
        | FocusId::TriggerBotDelaySlider
        | FocusId::AutoClickerCpsSlider => {
            save = false;
        }
    }
    if save {
        let _ = st.config.save();
    }
}

fn cycle_aim_target(current: AimTarget) -> AimTarget {
    match current {
        AimTarget::Hostile => AimTarget::Players,
        AimTarget::Players => AimTarget::Both,
        AimTarget::Both => AimTarget::Hostile,
    }
}
