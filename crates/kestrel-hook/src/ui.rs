use crate::config::{CrosshairStyle, FullbrightMode, KestrelConfig, OrePreset, Waypoint, XrayKey, ZoomKey};
use crate::control_bridge::{send_cmd, ControlCmd};
use crate::draw::{palette, Color, DrawList, Rect};
use crate::font::{FontAtlas, FontKind};
use crate::hud::waypoint_color;
use crate::state::{AppState, FocusId, NavTab, TextTarget, UiKey};
use std::time::Duration;

#[derive(Clone, Copy)]
enum ControlKind {
    Button,
    Toggle(bool),
    Slider(f32, f32, f32), // min, max, step
    Text(TextTarget),
}

struct Focusable {
    id: FocusId,
    rect: Rect,
    kind: ControlKind,
}

pub fn render_menu(st: &mut AppState, dl: &mut DrawList, atlas: &FontAtlas) {
    if !st.menu_open && st.menu_anim <= 0.01 {
        return;
    }

    let sw = st.screen_size[0].max(1) as f32;
    let sh = st.screen_size[1].max(1) as f32;

    // Backdrop dim (scales smoothly with menu_anim)
    let anim = st.menu_anim.clamp(0.0, 1.0);
    dl.rect(
        Rect::new(0.0, 0.0, sw, sh),
        0.0,
        Color::hex(0x05070A, 0.42 * anim),
    );

    // Panel geometry responsive to window size
    let panel_w = (sw - 48.0).clamp(640.0, 900.0);
    let panel_h = (sh - 48.0).clamp(420.0, 580.0);
    let px = ((sw - panel_w) * 0.5).round();
    let py = ((sh - panel_h) * 0.5 + (1.0 - anim) * 14.0).round();
    let panel = Rect::new(px, py, panel_w, panel_h);

    // Single elevation shadow (R-12 dose cap)
    dl.shadow(panel, 12.0, 22.0, palette::SHADOW.with_alpha(anim));
    dl.rect_border(
        panel,
        12.0,
        palette::CANVAS.with_alpha(anim),
        1.5,
        palette::BORDER_STRONG.with_alpha(0.75 * anim),
    );

    let mut focusables: Vec<Focusable> = Vec::with_capacity(32);

    // ---------------------------------------------------------
    // 1. Top Command Strip (Brand + Search + Quick Fullbright + Edit HUD + Close)
    // ---------------------------------------------------------
    let top_h = 56.0;
    let top_bar = Rect::new(px, py, panel_w, top_h);
    dl.rect(
        Rect::new(px + 1.0, py + top_h, panel_w - 2.0, 1.0),
        0.0,
        palette::BORDER.with_alpha(anim),
    );

    // Brand Crest & Wordmark
    dl.kestrel_crest(px + 18.0, py + 17.0, 22.0, palette::ACCENT, palette::TEXT_PRIMARY);
    dl.text(
        atlas,
        FontKind::UiTitle,
        px + 48.0,
        py + 15.0,
        "KESTREL",
        palette::TEXT_PRIMARY.with_alpha(anim),
    );
    dl.text(
        atlas,
        FontKind::MonoSmall,
        px + 152.0,
        py + 21.0,
        "SURVIVAL",
        palette::ACCENT.with_alpha(anim),
    );

    // Search Input (Live filter)
    let search_x = px + 236.0;
    let search_w = (panel_w - 644.0).max(120.0);
    let search_rect = Rect::new(search_x, py + 12.0, search_w, 32.0);
    draw_text_input(
        st,
        dl,
        atlas,
        search_rect,
        &st.search_query.clone(),
        "Search modules or waypoints...",
        st.active_text_target == Some(TextTarget::Search),
        st.focused == Some(FocusId::SearchBox),
        FocusId::SearchBox,
        TextTarget::Search,
        &mut focusables,
    );

    // Right action strip
    let close_rect = Rect::new(px + panel_w - 72.0, py + 12.0, 56.0, 32.0);
    let panic_rect = Rect::new(close_rect.x - 82.0, py + 12.0, 74.0, 32.0);
    let edit_hud_rect = Rect::new(panic_rect.x - 104.0, py + 12.0, 96.0, 32.0);
    let quick_fb_rect = Rect::new(edit_hud_rect.x - 130.0, py + 12.0, 122.0, 32.0);

    draw_quick_fb_pill(st, dl, atlas, quick_fb_rect, &mut focusables);
    draw_action_button(
        st,
        dl,
        atlas,
        panic_rect,
        "Panic",
        false,
        FocusId::PanicBtn,
        &mut focusables,
    );
    draw_action_button(
        st,
        dl,
        atlas,
        edit_hud_rect,
        "Edit HUD",
        true,
        FocusId::EditHudBtn,
        &mut focusables,
    );
    draw_action_button(
        st,
        dl,
        atlas,
        close_rect,
        "~ Esc",
        false,
        FocusId::CloseBtn,
        &mut focusables,
    );

    // ---------------------------------------------------------
    // 2. Left Navigation Rail (176px) + Main Workspace
    // ---------------------------------------------------------
    let rail_w = 176.0;
    let body_y = top_bar.y + top_h + 1.0;
    let body_h = panel_h - top_h - 1.0;
    let rail_rect = Rect::new(px, body_y, rail_w, body_h);
    dl.rect(
        Rect::new(px + rail_w, body_y, 1.0, body_h),
        0.0,
        palette::BORDER.with_alpha(anim),
    );

    let tabs = [
        (NavTab::Survival, FocusId::TabSurvival, "Survival", "Fullbright, X-Ray, Zoom"),
        (NavTab::Automation, FocusId::TabAutomation, "Automation", "Wurst7 Auto & Combat"),
        (NavTab::Hud, FocusId::TabHud, "HUD Overlay", "Coords, Radar, CPS"),
        (NavTab::Waypoints, FocusId::TabWaypoints, "Waypoints", "Markers & Portal Math"),
        (NavTab::System, FocusId::TabSystem, "System & Keys", "Hooks, Socket, Config"),
    ];

    for (i, (tab, fid, title, sub)) in tabs.iter().enumerate() {
        let tr = Rect::new(rail_rect.x + 12.0, rail_rect.y + 12.0 + i as f32 * 50.0, rail_w - 24.0, 46.0);
        let selected = st.active_tab == *tab;
        let hovered = tr.contains(st.cursor_pos[0], st.cursor_pos[1]);
        let focused = st.focused == Some(*fid);

        let bg = if selected {
            palette::SURFACE_HOVER
        } else if hovered {
            palette::SURFACE
        } else {
            Color::hex(0x000000, 0.0)
        };
        let border = if focused {
            palette::ACCENT
        } else if selected {
            palette::BORDER_STRONG
        } else {
            Color::hex(0x000000, 0.0)
        };
        dl.rect_border(tr, 8.0, bg, if focused { 2.0 } else { 1.0 }, border);
        if selected {
            dl.rect(
                Rect::new(tr.x + 3.0, tr.y + 10.0, 3.0, tr.h - 20.0),
                1.5,
                palette::ACCENT,
            );
        }
        dl.text(
            atlas,
            FontKind::UiSemi,
            tr.x + 14.0,
            tr.y + 7.0,
            title,
            if selected {
                palette::TEXT_PRIMARY
            } else {
                palette::TEXT_SECONDARY
            },
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            tr.x + 14.0,
            tr.y + 25.0,
            sub,
            palette::TEXT_SECONDARY.with_alpha(0.85),
        );
        focusables.push(Focusable {
            id: *fid,
            rect: tr,
            kind: ControlKind::Button,
        });
    }

    // Socket & Lightmap honest status summary at bottom of rail
    let status_rect = Rect::new(rail_rect.x + 12.0, rail_rect.y + body_h - 68.0, rail_w - 24.0, 56.0);
    dl.rect_border(status_rect, 8.0, palette::SURFACE, 1.0, palette::BORDER);
    let sock_label = if st.game.socket_connected {
        "Socket: Connected"
    } else {
        "Socket: Offline"
    };
    let sock_col = if st.game.socket_connected {
        palette::SUCCESS
    } else {
        palette::TEXT_SECONDARY
    };
    let lm_label = if st.lightmap_hooked {
        "Lightmap: Hooked"
    } else {
        "Lightmap: Waiting"
    };
    dl.text(
        atlas,
        FontKind::UiSmall,
        status_rect.x + 10.0,
        status_rect.y + 9.0,
        sock_label,
        sock_col,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        status_rect.x + 10.0,
        status_rect.y + 29.0,
        lm_label,
        if st.lightmap_hooked {
            palette::ACCENT
        } else {
            palette::TEXT_SECONDARY
        },
    );

    // ---------------------------------------------------------
    // 3. Scrollable Content Area
    // ---------------------------------------------------------
    let content = Rect::new(px + rail_w + 1.0, body_y, panel_w - rail_w - 1.0, body_h);
    dl.push_scissor(content);

    // Scroll input
    if content.contains(st.cursor_pos[0], st.cursor_pos[1]) && st.scroll_delta.abs() > 0.01 {
        st.scroll_y = (st.scroll_y - st.scroll_delta * 28.0).clamp(0.0, st.max_scroll_y);
    }

    let cx = content.x + 22.0;
    let cw = content.w - 44.0;
    let mut cy = content.y + 18.0 - st.scroll_y;

    let query = st.search_query.trim().to_lowercase();
    if !query.is_empty() {
        cy = render_search_results(st, dl, atlas, cx, cy, cw, &query, &mut focusables);
    } else {
        cy = match st.active_tab {
            NavTab::Survival => render_survival_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::Automation => render_automation_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::Hud => render_hud_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::Waypoints => render_waypoints_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
            NavTab::System => render_system_tab(st, dl, atlas, cx, cy, cw, &mut focusables),
        };
    }

    let total_content_h = (cy + st.scroll_y) - (content.y + 18.0);
    st.max_scroll_y = (total_content_h - (content.h - 32.0)).max(0.0);
    st.scroll_y = st.scroll_y.clamp(0.0, st.max_scroll_y);

    dl.pop_scissor();

    // Scrollbar indicator when needed
    if st.max_scroll_y > 1.0 {
        let track_h = content.h - 24.0;
        let thumb_h = (track_h * (content.h / (content.h + st.max_scroll_y))).max(32.0);
        let thumb_y = content.y + 12.0 + (st.scroll_y / st.max_scroll_y) * (track_h - thumb_h);
        dl.rect(
            Rect::new(content.x + content.w - 7.0, thumb_y, 3.5, thumb_h),
            1.75,
            palette::BORDER_STRONG,
        );
    }

    // Toast notification at bottom of panel
    if let Some((msg, at)) = st.toast.clone() {
        if at.elapsed() < Duration::from_secs(3) {
            let tw = atlas.measure(FontKind::UiSemi, &msg) + 28.0;
            let tr = Rect::new(px + (panel_w - tw) * 0.5, py + panel_h + 10.0, tw, 30.0);
            dl.rect_border(tr, 8.0, palette::SURFACE, 1.5, palette::ACCENT);
            dl.text(
                atlas,
                FontKind::UiSemi,
                tr.x + 14.0,
                tr.y + 7.0,
                &msg,
                palette::TEXT_PRIMARY,
            );
        } else {
            st.toast = None;
        }
    }

    // Process clicks, sliders, and keyboard navigation after drawing layout
    if st.menu_open {
        process_interactions(st, &focusables);
    }
}

fn render_survival_tab(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    // Hero Featured Module: Fullbright (asymmetric larger treatment per R-14)
    let hero_h = 166.0;
    let hero = Rect::new(x, y, w, hero_h);
    dl.rect_border(hero, 10.0, palette::SURFACE, 1.0, palette::BORDER);

    dl.icon_sun(hero.x + 28.0, hero.y + 30.0, 11.0, palette::ACCENT);
    dl.text(
        atlas,
        FontKind::UiHeading,
        hero.x + 48.0,
        hero.y + 18.0,
        "Fullbright",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        hero.x + 48.0,
        hero.y + 42.0,
        "Overrides the GPU 16x16 lightmap texture directly in Vulkan so caves and night stay clear.",
        palette::TEXT_SECONDARY,
    );

    let fb_toggle = Rect::new(hero.x + hero.w - 86.0, hero.y + 18.0, 68.0, 30.0);
    draw_toggle_pill(
        st,
        dl,
        atlas,
        fb_toggle,
        st.config.fullbright_enabled,
        FocusId::FullbrightToggle,
        focusables,
    );

    // Mode segmented control (Max 100% vs Soft Blend)
    dl.text(
        atlas,
        FontKind::UiSmall,
        hero.x + 18.0,
        hero.y + 76.0,
        "Mode",
        palette::TEXT_SECONDARY,
    );
    let max_btn = Rect::new(hero.x + 18.0, hero.y + 96.0, 126.0, 30.0);
    let soft_btn = Rect::new(hero.x + 152.0, hero.y + 96.0, 126.0, 30.0);
    draw_segment_button(
        st,
        dl,
        atlas,
        max_btn,
        "Max 100%",
        st.config.fullbright_mode == FullbrightMode::Max,
        FocusId::FullbrightModeMax,
        focusables,
    );
    draw_segment_button(
        st,
        dl,
        atlas,
        soft_btn,
        "Soft Blend",
        st.config.fullbright_mode == FullbrightMode::Soft,
        FocusId::FullbrightModeSoft,
        focusables,
    );

    // Strength slider (active for Soft Blend, informative otherwise)
    let slider_x = hero.x + 304.0;
    let slider_w = (hero.w - 324.0).max(160.0);
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(slider_x, hero.y + 76.0, slider_w, 50.0),
        "Soft Boost Strength",
        &format!("{:.0}%", st.config.fullbright_strength * 100.0),
        st.config.fullbright_strength,
        0.35,
        1.0,
        0.05,
        FocusId::FullbrightStrength,
        focusables,
    );

    let hook_note = if st.lightmap_hooked {
        "GPU Lightmap active · toggles instantaneously even while standing still"
    } else {
        "Waiting for world renderer to initialize lightmap texture"
    };
    dl.text(
        atlas,
        FontKind::UiSmall,
        hero.x + 18.0,
        hero.y + 138.0,
        hook_note,
        palette::TEXT_SECONDARY,
    );

    y += hero_h + 16.0;

    // Two-column varied secondary survival controls: Zoom (left) + Crosshair+ (right)
    let col_gap = 14.0;
    let col_w = (w - col_gap) * 0.5;
    let card_h = 164.0;

    // Left: OptiZoom
    let z_rect = Rect::new(x, y, col_w, card_h);
    dl.rect_border(z_rect, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(
        atlas,
        FontKind::UiHeading,
        z_rect.x + 16.0,
        z_rect.y + 14.0,
        "OptiZoom",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        z_rect.x + 16.0,
        z_rect.y + 38.0,
        "Hold key to narrow FOV via Horizon control.",
        palette::TEXT_SECONDARY,
    );
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(z_rect.x + col_w - 82.0, z_rect.y + 14.0, 66.0, 28.0),
        st.config.zoom_enabled,
        FocusId::ZoomToggle,
        focusables,
    );
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(z_rect.x + 16.0, z_rect.y + 66.0, col_w - 32.0, 46.0),
        "Zoom FOV",
        &format!("{} deg", st.config.zoom_fov),
        st.config.zoom_fov as f32,
        15.0,
        60.0,
        5.0,
        FocusId::ZoomFovSlider,
        focusables,
    );
    draw_cycle_row(
        st,
        dl,
        atlas,
        Rect::new(z_rect.x + 16.0, z_rect.y + 120.0, col_w - 32.0, 30.0),
        "Hold Key",
        st.config.zoom_key.label(),
        FocusId::ZoomKeyCycle,
        focusables,
    );

    // Right: Crosshair+
    let c_rect = Rect::new(x + col_w + col_gap, y, col_w, card_h);
    dl.rect_border(c_rect, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(
        atlas,
        FontKind::UiHeading,
        c_rect.x + 16.0,
        c_rect.y + 14.0,
        "Crosshair+",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        c_rect.x + 16.0,
        c_rect.y + 38.0,
        "Sharp custom reticle overlaid in Vulkan.",
        palette::TEXT_SECONDARY,
    );
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(c_rect.x + col_w - 82.0, c_rect.y + 14.0, 66.0, 28.0),
        st.config.crosshair_enabled,
        FocusId::CrosshairToggle,
        focusables,
    );
    draw_cycle_row(
        st,
        dl,
        atlas,
        Rect::new(c_rect.x + 16.0, c_rect.y + 66.0, col_w - 32.0, 30.0),
        "Reticle Style",
        st.config.crosshair_style.label(),
        FocusId::CrosshairStyleCycle,
        focusables,
    );
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(c_rect.x + 16.0, c_rect.y + 106.0, (col_w - 40.0) * 0.5, 46.0),
        "Arm Size",
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
        Rect::new(
            c_rect.x + 24.0 + (col_w - 40.0) * 0.5,
            c_rect.y + 106.0,
            (col_w - 40.0) * 0.5,
            46.0,
        ),
        "Center Gap",
        &format!("{:.0}px", st.config.crosshair_gap),
        st.config.crosshair_gap,
        1.0,
        10.0,
        1.0,
        FocusId::CrosshairGapSlider,
        focusables,
    );

    y += card_h + 16.0;

    // X-Ray Vision Card (Ore 3D ESP + Cave Wireframe + Chest/Mobs ESP)
    let x_rect = Rect::new(x, y, w, 178.0);
    dl.rect_border(x_rect, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(
        atlas,
        FontKind::UiHeading,
        x_rect.x + 16.0,
        x_rect.y + 14.0,
        "X-Ray Vision",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        x_rect.x + 16.0,
        x_rect.y + 38.0,
        "3D Ore ESP highlights buried ores; Cave Wireframe & ESP reveal tunnels and chests.",
        palette::TEXT_SECONDARY,
    );

    let x_third = (w - 56.0) / 3.0;
    let xo_row = Rect::new(x_rect.x + 16.0, x_rect.y + 62.0, x_third, 30.0);
    dl.text(atlas, FontKind::UiSemi, xo_row.x, xo_row.y + 6.0, "Ore X-Ray", palette::TEXT_PRIMARY);
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(xo_row.x + xo_row.w - 60.0, xo_row.y, 60.0, 28.0),
        st.config.xray_ores,
        FocusId::XrayOresToggle,
        focusables,
    );

    let xt_row = Rect::new(x_rect.x + 28.0 + x_third, x_rect.y + 62.0, x_third, 30.0);
    dl.text(atlas, FontKind::UiSemi, xt_row.x, xt_row.y + 6.0, "Caves", palette::TEXT_PRIMARY);
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(xt_row.x + xt_row.w - 60.0, xt_row.y, 60.0, 28.0),
        st.config.xray_terrain,
        FocusId::XrayTerrainToggle,
        focusables,
    );

    let xe_row = Rect::new(x_rect.x + 40.0 + x_third * 2.0, x_rect.y + 62.0, x_third, 30.0);
    dl.text(atlas, FontKind::UiSemi, xe_row.x, xe_row.y + 6.0, "Chest/Mob", palette::TEXT_PRIMARY);
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(xe_row.x + xe_row.w - 60.0, xe_row.y, 60.0, 28.0),
        st.config.xray_entities,
        FocusId::XrayEntitiesToggle,
        focusables,
    );

    let x_half = (w - 46.0) * 0.5;
    draw_cycle_row(
        st,
        dl,
        atlas,
        Rect::new(x_rect.x + 16.0, x_rect.y + 100.0, x_half, 28.0),
        "Ore Filter",
        st.config.xray_ore_preset.label(),
        FocusId::XrayOrePresetCycle,
        focusables,
    );
    draw_cycle_row(
        st,
        dl,
        atlas,
        Rect::new(x_rect.x + 30.0 + x_half, x_rect.y + 100.0, x_half, 28.0),
        "Hotkey (Ore)",
        st.config.xray_key.label(),
        FocusId::XrayKeyCycle,
        focusables,
    );

    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(x_rect.x + 16.0, x_rect.y + 132.0, w - 32.0, 42.0),
        "Ore Scan Radius",
        &format!("{} blocks", st.config.xray_radius),
        st.config.xray_radius as f32,
        12.0,
        48.0,
        4.0,
        FocusId::XrayRadiusSlider,
        focusables,
    );

    y += 192.0;

    // Compact bottom strip: Horizon Vanilla Gamma Sync
    let g_rect = Rect::new(x, y, w, 72.0);
    dl.rect_border(g_rect, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    let gamma_val = st.game.horizon_gamma.unwrap_or(0.5);
    let gamma_lbl = if st.game.socket_connected {
        format!("{:.0}%", gamma_val * 100.0)
    } else {
        String::from("Socket offline")
    };
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(g_rect.x + 18.0, g_rect.y + 12.0, g_rect.w - 36.0, 48.0),
        "Horizon Brightness (options.txt gamma)",
        &gamma_lbl,
        gamma_val,
        0.0,
        1.0,
        0.05,
        FocusId::GammaSlider,
        focusables,
    );

    y + 72.0
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
        let banner = Rect::new(x, y, w, 58.0);
        dl.rect_border(banner, 8.0, palette::SURFACE, 1.0, palette::DANGER);
        dl.text(
            atlas,
            FontKind::UiSemi,
            banner.x + 14.0,
            banner.y + 10.0,
            "Horizon Control Socket Offline",
            palette::DANGER,
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            banner.x + 14.0,
            banner.y + 31.0,
            "Enable agentControl:true in horizon-extras.txt so automation modules can control gameplay.",
            palette::TEXT_SECONDARY,
        );
        y += 70.0;
    }

    let simple_rows = [
        (
            "AutoSprint",
            "Automatically holds sprint while moving forward with W.",
            st.config.auto_sprint,
            FocusId::AutoSprintToggle,
        ),
        (
            "AutoRespawn",
            "Instantly respawns when the death screen appears.",
            st.config.auto_respawn,
            FocusId::AutoRespawnToggle,
        ),
        (
            "AutoTotem",
            "Restocks Totem of Undying into offhand slot when missing.",
            st.config.auto_totem,
            FocusId::AutoTotemToggle,
        ),
        (
            "AutoFish",
            "Watches fishing bobber dip and reels/recasts automatically.",
            st.config.auto_fish,
            FocusId::AutoFishToggle,
        ),
        (
            "AntiAFK",
            "Taps sneak periodically when idle for 30s to prevent server kick.",
            st.config.anti_afk,
            FocusId::AntiAfkToggle,
        ),
    ];

    for (title, desc, on, fid) in simple_rows {
        let rr = Rect::new(x, y, w, 54.0);
        dl.rect_border(rr, 8.0, palette::SURFACE, 1.0, palette::BORDER);
        dl.text(atlas, FontKind::UiSemi, rr.x + 16.0, rr.y + 9.0, title, palette::TEXT_PRIMARY);
        dl.text(atlas, FontKind::UiSmall, rr.x + 16.0, rr.y + 29.0, desc, palette::TEXT_SECONDARY);
        draw_toggle_pill(
            st,
            dl,
            atlas,
            Rect::new(rr.x + w - 82.0, rr.y + 13.0, 66.0, 28.0),
            on,
            fid,
            focusables,
        );
        y += 62.0;
    }

    let eat_r = Rect::new(x, y, w, 106.0);
    dl.rect_border(eat_r, 8.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(atlas, FontKind::UiSemi, eat_r.x + 16.0, eat_r.y + 10.0, "AutoEat", palette::TEXT_PRIMARY);
    dl.text(
        atlas,
        FontKind::UiSmall,
        eat_r.x + 16.0,
        eat_r.y + 30.0,
        "Selects food in hotbar and eats automatically below hunger threshold.",
        palette::TEXT_SECONDARY,
    );
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(eat_r.x + w - 82.0, eat_r.y + 12.0, 66.0, 28.0),
        st.config.auto_eat,
        FocusId::AutoEatToggle,
        focusables,
    );
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(eat_r.x + 16.0, eat_r.y + 54.0, w - 32.0, 44.0),
        "Eat At Hunger",
        &format!("<= {}/20", st.config.auto_eat_hunger),
        st.config.auto_eat_hunger as f32,
        6.0,
        19.0,
        1.0,
        FocusId::AutoEatHungerSlider,
        focusables,
    );
    y += 116.0;

    let gap = 14.0;
    let cw = (w - gap) * 0.5;
    let tb_r = Rect::new(x, y, cw, 112.0);
    dl.rect_border(tb_r, 8.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(atlas, FontKind::UiSemi, tb_r.x + 14.0, tb_r.y + 12.0, "TriggerBot", palette::TEXT_PRIMARY);
    dl.text(
        atlas,
        FontKind::UiSmall,
        tb_r.x + 14.0,
        tb_r.y + 32.0,
        "Attacks aimed entity in reach.",
        palette::TEXT_SECONDARY,
    );
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(tb_r.x + cw - 78.0, tb_r.y + 12.0, 64.0, 28.0),
        st.config.trigger_bot,
        FocusId::TriggerBotToggle,
        focusables,
    );
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(tb_r.x + 14.0, tb_r.y + 56.0, cw - 28.0, 46.0),
        "Hit Cooldown",
        &format!("{} ms", st.config.trigger_bot_delay_ms),
        st.config.trigger_bot_delay_ms as f32,
        400.0,
        1200.0,
        50.0,
        FocusId::TriggerBotDelaySlider,
        focusables,
    );

    let ac_r = Rect::new(x + cw + gap, y, cw, 112.0);
    dl.rect_border(ac_r, 8.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(atlas, FontKind::UiSemi, ac_r.x + 14.0, ac_r.y + 12.0, "AutoClicker", palette::TEXT_PRIMARY);
    dl.text(
        atlas,
        FontKind::UiSmall,
        ac_r.x + 14.0,
        ac_r.y + 32.0,
        "Pulses LMB while held.",
        palette::TEXT_SECONDARY,
    );
    draw_toggle_pill(
        st,
        dl,
        atlas,
        Rect::new(ac_r.x + cw - 78.0, ac_r.y + 12.0, 64.0, 28.0),
        st.config.auto_clicker,
        FocusId::AutoClickerToggle,
        focusables,
    );
    draw_slider(
        st,
        dl,
        atlas,
        Rect::new(ac_r.x + 14.0, ac_r.y + 56.0, cw - 28.0, 46.0),
        "Target CPS",
        &format!("{} CPS", st.config.auto_clicker_cps),
        st.config.auto_clicker_cps as f32,
        4.0,
        20.0,
        1.0,
        FocusId::AutoClickerCpsSlider,
        focusables,
    );

    y + 112.0
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
    // Top action row
    let header_rect = Rect::new(x, y, w, 58.0);
    dl.rect_border(header_rect, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(
        atlas,
        FontKind::UiHeading,
        x + 16.0,
        y + 10.0,
        "On-Screen Survival HUD",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        x + 16.0,
        y + 32.0,
        "Toggle widgets below or enter HUD Editor to drag panels anywhere on screen.",
        palette::TEXT_SECONDARY,
    );

    let reset_btn = Rect::new(x + w - 122.0, y + 13.0, 106.0, 32.0);
    let edit_btn = Rect::new(reset_btn.x - 124.0, y + 13.0, 114.0, 32.0);
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
    y += 72.0;

    let rows = [
        (
            "Coordinates HUD",
            "Shows live XYZ, facing direction, and dimension.",
            st.config.hud_coords.enabled,
            FocusId::HudToggleCoords,
        ),
        (
            "Nether Portal Converter",
            "Displays auto-scaled Overworld/Nether coordinates underneath XYZ.",
            st.config.hud_nether_convert,
            FocusId::HudToggleNether,
        ),
        (
            "FPS & Fullbright Badge",
            "Live average FPS, frame time ms, 1% low, and Fullbright indicator.",
            st.config.hud_fps.enabled,
            FocusId::HudToggleFps,
        ),
        (
            "Keystrokes & CPS",
            "Real-time WASD + Space + LMB/RMB click rate counter.",
            st.config.hud_keystrokes.enabled,
            FocusId::HudToggleKeystrokes,
        ),
        (
            "Vitals, Totems & Target",
            "Health, hunger, XP level, totem/arrow counts, and crosshair target.",
            st.config.hud_vitals.enabled,
            FocusId::HudToggleVitals,
        ),
        (
            "Waypoint Compass",
            "Directional strip at top showing bearing and distance to waypoints.",
            st.config.hud_compass.enabled,
            FocusId::HudToggleCompass,
        ),
        (
            "World & Session Clock",
            "In-game day/clock, weather, and play session duration.",
            st.config.hud_clock.enabled,
            FocusId::HudToggleClock,
        ),
        (
            "Entity Radar (Wurst7)",
            "Top-down rotating minimap plotting mobs, players, items, and XP.",
            st.config.hud_radar.enabled,
            FocusId::HudToggleRadar,
        ),
    ];

    for (title, desc, enabled, fid) in rows {
        let rr = Rect::new(x, y, w, 54.0);
        dl.rect_border(rr, 8.0, palette::SURFACE, 1.0, palette::BORDER);
        dl.text(
            atlas,
            FontKind::UiSemi,
            rr.x + 16.0,
            rr.y + 9.0,
            title,
            palette::TEXT_PRIMARY,
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            rr.x + 16.0,
            rr.y + 29.0,
            desc,
            palette::TEXT_SECONDARY,
        );
        draw_toggle_pill(
            st,
            dl,
            atlas,
            Rect::new(rr.x + w - 82.0, rr.y + 13.0, 66.0, 28.0),
            enabled,
            fid,
            focusables,
        );
        y += 62.0;
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
    // Creation row
    let form_h = 98.0;
    let form = Rect::new(x, y, w, form_h);
    dl.rect_border(form, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(
        atlas,
        FontKind::UiHeading,
        form.x + 16.0,
        form.y + 12.0,
        "Survival Waypoints",
        palette::TEXT_PRIMARY,
    );

    let pos_hint = if st.game.in_world {
        format!(
            "Current: {:.0}, {:.0}, {:.0} ({})",
            st.game.pos[0].floor(),
            st.game.pos[1].floor(),
            st.game.pos[2].floor(),
            st.game
                .dimension
                .strip_prefix("minecraft:")
                .unwrap_or(&st.game.dimension)
        )
    } else {
        String::from("Enter a world with agentControl enabled to save your current XYZ.")
    };
    dl.text(
        atlas,
        FontKind::UiSmall,
        form.x + 16.0,
        form.y + 34.0,
        &pos_hint,
        palette::TEXT_SECONDARY,
    );

    let input_w = (w - 190.0).max(160.0);
    draw_text_input(
        st,
        dl,
        atlas,
        Rect::new(form.x + 16.0, form.y + 56.0, input_w, 30.0),
        &st.waypoint_draft_name.clone(),
        "Waypoint name (e.g. Base, Stronghold)...",
        st.active_text_target == Some(TextTarget::WaypointName),
        st.focused == Some(FocusId::WaypointInput),
        FocusId::WaypointInput,
        TextTarget::WaypointName,
        focusables,
    );

    let add_rect = Rect::new(form.x + 26.0 + input_w, form.y + 56.0, 146.0, 30.0);
    draw_action_button_enabled(
        st,
        dl,
        atlas,
        add_rect,
        if st.game.in_world {
            "Save Current XYZ"
        } else {
            "Not in World"
        },
        st.game.in_world,
        st.game.in_world,
        FocusId::WaypointAddHere,
        focusables,
    );

    y += form_h + 14.0;

    if st.config.waypoints.is_empty() {
        let empty = Rect::new(x, y, w, 86.0);
        dl.rect_border(empty, 10.0, palette::SURFACE, 1.0, palette::BORDER);
        dl.text(
            atlas,
            FontKind::UiSemi,
            empty.x + 18.0,
            empty.y + 18.0,
            "No waypoints saved yet",
            palette::TEXT_PRIMARY,
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            empty.x + 18.0,
            empty.y + 42.0,
            "Type a name above and click Save Current XYZ to pin it to your HUD compass.",
            palette::TEXT_SECONDARY,
        );
        return y + 86.0;
    }

    for (idx, wp) in st.config.waypoints.iter().enumerate() {
        let rr = Rect::new(x, y, w, 54.0);
        dl.rect_border(rr, 8.0, palette::SURFACE, 1.0, palette::BORDER);
        dl.rect(
            Rect::new(rr.x + 14.0, rr.y + 19.0, 10.0, 16.0),
            3.0,
            waypoint_color(wp.color_idx),
        );
        dl.text(
            atlas,
            FontKind::UiSemi,
            rr.x + 34.0,
            rr.y + 9.0,
            &wp.name,
            palette::TEXT_PRIMARY,
        );
        let dim = wp.dimension.strip_prefix("minecraft:").unwrap_or(&wp.dimension);
        let meta = if st.game.in_world && st.game.dimension == wp.dimension {
            let dx = wp.x as f64 - st.game.pos[0];
            let dz = wp.z as f64 - st.game.pos[2];
            let d = (dx * dx + dz * dz).sqrt();
            format!("{} / {} / {} · {} · {:.0}m away", wp.x, wp.y, wp.z, dim, d)
        } else {
            format!("{} / {} / {} · {}", wp.x, wp.y, wp.z, dim)
        };
        dl.text(
            atlas,
            FontKind::MonoSmall,
            rr.x + 34.0,
            rr.y + 30.0,
            &meta,
            palette::TEXT_SECONDARY,
        );

        let del_rect = Rect::new(rr.x + w - 78.0, rr.y + 13.0, 64.0, 28.0);
        let tog_rect = Rect::new(del_rect.x - 76.0, rr.y + 13.0, 66.0, 28.0);
        draw_toggle_pill(
            st,
            dl,
            atlas,
            tog_rect,
            wp.enabled,
            FocusId::WaypointItemToggle(idx),
            focusables,
        );
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
        y += 62.0;
    }

    y
}

fn render_system_tab(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    x: f32,
    mut y: f32,
    w: f32,
    focusables: &mut Vec<Focusable>,
) -> f32 {
    let diag = Rect::new(x, y, w, 178.0);
    dl.rect_border(diag, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(
        atlas,
        FontKind::UiHeading,
        diag.x + 16.0,
        diag.y + 14.0,
        "Injection & Diagnostics",
        palette::TEXT_PRIMARY,
    );

    let rows = [
        ("Vulkan Adapter", st.gpu_name.clone()),
        (
            "Swapchain Target",
            format!("{} x {} px", st.screen_size[0], st.screen_size[1]),
        ),
        (
            "Lightmap Intercept",
            if st.lightmap_hooked {
                format!("Active ({} 16x16 image tracked)", st.lightmap_images_seen)
            } else {
                String::from("Waiting for world lightmap upload")
            },
        ),
        (
            "Horizon Socket",
            if st.game.socket_connected {
                format!("Connected (screen: {})", st.game.screen)
            } else {
                String::from("Offline (set agentControl:true in horizon-extras.txt)")
            },
        ),
        ("Menu Toggle Keys", String::from("Grave [`] or Tilde [~] · Close [Esc]")),
        (
            "Config File",
            KestrelConfig::config_path().display().to_string(),
        ),
    ];

    for (i, (k, v)) in rows.iter().enumerate() {
        let ry = diag.y + 46.0 + i as f32 * 21.0;
        dl.text(atlas, FontKind::UiSmall, diag.x + 16.0, ry, k, palette::TEXT_SECONDARY);
        dl.text(atlas, FontKind::MonoSmall, diag.x + 178.0, ry, v, palette::TEXT_PRIMARY);
    }

    y += 194.0;

    let actions = Rect::new(x, y, w, 64.0);
    dl.rect_border(actions, 10.0, palette::SURFACE, 1.0, palette::BORDER);
    dl.text(
        atlas,
        FontKind::UiSemi,
        actions.x + 16.0,
        actions.y + 12.0,
        "Configuration Persistence",
        palette::TEXT_PRIMARY,
    );
    dl.text(
        atlas,
        FontKind::UiSmall,
        actions.x + 16.0,
        actions.y + 34.0,
        "Changes auto-save on toggle, or write manually now.",
        palette::TEXT_SECONDARY,
    );

    let reset_all = Rect::new(actions.x + w - 134.0, actions.y + 16.0, 118.0, 32.0);
    let save_now = Rect::new(reset_all.x - 128.0, actions.y + 16.0, 118.0, 32.0);
    draw_action_button(
        st,
        dl,
        atlas,
        save_now,
        "Save Config",
        true,
        FocusId::SystemSaveConfig,
        focusables,
    );
    draw_action_button(
        st,
        dl,
        atlas,
        reset_all,
        "Reset Defaults",
        false,
        FocusId::SystemResetAll,
        focusables,
    );

    y + 64.0
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
    dl.text(
        atlas,
        FontKind::UiSmall,
        x,
        y,
        &format!("Filtering by \"{q}\""),
        palette::ACCENT,
    );
    y += 24.0;

    let items: [(&str, &str, bool, FocusId); 19] = [
        ("Fullbright", "GPU 16x16 lightmap override", st.config.fullbright_enabled, FocusId::FullbrightToggle),
        ("Ore X-Ray (3D ESP)", "Highlight Diamond, Debris, Gold, Iron through stone", st.config.xray_ores, FocusId::XrayOresToggle),
        ("X-Ray Cave Wireframe", "See caves, lava, mineshafts through stone", st.config.xray_terrain, FocusId::XrayTerrainToggle),
        ("X-Ray Chests & Mobs", "Render chests, players, mobs through walls", st.config.xray_entities, FocusId::XrayEntitiesToggle),
        ("Entity Radar", "Rotating minimap of nearby mobs, players, items", st.config.hud_radar.enabled, FocusId::HudToggleRadar),
        ("AutoSprint", "Auto-hold sprint when moving forward", st.config.auto_sprint, FocusId::AutoSprintToggle),
        ("AutoRespawn", "Auto-respawn on death screen", st.config.auto_respawn, FocusId::AutoRespawnToggle),
        ("AutoEat", "Auto-eat hotbar food when hungry", st.config.auto_eat, FocusId::AutoEatToggle),
        ("AutoTotem", "Auto-equip Totem of Undying in offhand", st.config.auto_totem, FocusId::AutoTotemToggle),
        ("AutoFish", "Auto-reel and recast fishing rod", st.config.auto_fish, FocusId::AutoFishToggle),
        ("TriggerBot", "Auto-attack entity under crosshair", st.config.trigger_bot, FocusId::TriggerBotToggle),
        ("AutoClicker", "Auto-click LMB at configured CPS", st.config.auto_clicker, FocusId::AutoClickerToggle),
        ("AntiAFK", "Periodic sneak pulse while idle", st.config.anti_afk, FocusId::AntiAfkToggle),
        ("OptiZoom", "Hold key to narrow FOV", st.config.zoom_enabled, FocusId::ZoomToggle),
        ("Crosshair+", "Custom tactical reticle", st.config.crosshair_enabled, FocusId::CrosshairToggle),
        ("Coordinates HUD", "XYZ + Nether converter", st.config.hud_coords.enabled, FocusId::HudToggleCoords),
        ("FPS & Light HUD", "FPS, frametime, 1% low", st.config.hud_fps.enabled, FocusId::HudToggleFps),
        ("Keystrokes & CPS", "WASD + LMB/RMB CPS", st.config.hud_keystrokes.enabled, FocusId::HudToggleKeystrokes),
        ("Waypoint Compass", "Directional bearing strip", st.config.hud_compass.enabled, FocusId::HudToggleCompass),
    ];

    let mut matched = 0usize;
    for (name, desc, on, fid) in items {
        if name.to_lowercase().contains(q) || desc.to_lowercase().contains(q) {
            matched += 1;
            let rr = Rect::new(x, y, w, 54.0);
            dl.rect_border(rr, 8.0, palette::SURFACE, 1.0, palette::BORDER);
            dl.text(atlas, FontKind::UiSemi, rr.x + 16.0, rr.y + 9.0, name, palette::TEXT_PRIMARY);
            dl.text(atlas, FontKind::UiSmall, rr.x + 16.0, rr.y + 29.0, desc, palette::TEXT_SECONDARY);
            draw_toggle_pill(
                st,
                dl,
                atlas,
                Rect::new(rr.x + w - 82.0, rr.y + 13.0, 66.0, 28.0),
                on,
                fid,
                focusables,
            );
            y += 62.0;
        }
    }

    for (idx, wp) in st.config.waypoints.iter().enumerate() {
        if wp.name.to_lowercase().contains(q) {
            matched += 1;
            let rr = Rect::new(x, y, w, 54.0);
            dl.rect_border(rr, 8.0, palette::SURFACE, 1.0, palette::BORDER);
            dl.text(atlas, FontKind::UiSemi, rr.x + 16.0, rr.y + 9.0, &wp.name, palette::TEXT_PRIMARY);
            let coords = format!("Waypoint {} / {} / {}", wp.x, wp.y, wp.z);
            dl.text(atlas, FontKind::MonoSmall, rr.x + 16.0, rr.y + 29.0, &coords, palette::TEXT_SECONDARY);
            draw_toggle_pill(
                st,
                dl,
                atlas,
                Rect::new(rr.x + w - 82.0, rr.y + 13.0, 66.0, 28.0),
                wp.enabled,
                FocusId::WaypointItemToggle(idx),
                focusables,
            );
            y += 62.0;
        }
    }

    if matched == 0 {
        let empty = Rect::new(x, y, w, 72.0);
        dl.rect_border(empty, 8.0, palette::SURFACE, 1.0, palette::BORDER);
        dl.text(
            atlas,
            FontKind::UiSemi,
            empty.x + 16.0,
            empty.y + 14.0,
            "No matching modules or waypoints",
            palette::TEXT_PRIMARY,
        );
        dl.text(
            atlas,
            FontKind::UiSmall,
            empty.x + 16.0,
            empty.y + 36.0,
            "Clear the search box or try 'fullbright', 'zoom', 'coords', or 'fps'.",
            palette::TEXT_SECONDARY,
        );
        y += 72.0;
    }

    y
}

// ---------------------------------------------------------
// Reusable UI Widgets (all keyboard + mouse accessible)
// ---------------------------------------------------------

fn draw_quick_fb_pill(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    focusables: &mut Vec<Focusable>,
) {
    let on = st.config.fullbright_enabled;
    let hovered = r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(FocusId::QuickFullbright);
    let bg = if on {
        palette::SUCCESS_SOFT
    } else if hovered {
        palette::SURFACE_HOVER
    } else {
        palette::SURFACE
    };
    let border = if focused {
        palette::ACCENT
    } else if on {
        palette::SUCCESS
    } else {
        palette::BORDER_STRONG
    };
    dl.rect_border(r, 7.0, bg, if focused { 2.0 } else { 1.0 }, border);
    let label = if on { "Fullbright: ON" } else { "Fullbright: OFF" };
    let tw = atlas.measure(FontKind::UiSemi, label);
    dl.text(
        atlas,
        FontKind::UiSemi,
        r.x + (r.w - tw) * 0.5,
        r.y + 8.0,
        label,
        if on {
            palette::SUCCESS
        } else {
            palette::TEXT_PRIMARY
        },
    );
    focusables.push(Focusable {
        id: FocusId::QuickFullbright,
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
        (palette::SURFACE, palette::TEXT_SECONDARY.with_alpha(0.55), palette::BORDER)
    } else if primary {
        let b = if hovered {
            Color::hex(0xF2B94F, 1.0)
        } else {
            palette::ACCENT
        };
        (b, palette::CANVAS_SOLID, if focused { palette::TEXT_PRIMARY } else { b })
    } else {
        let b = if hovered {
            palette::SURFACE_HOVER
        } else {
            palette::SURFACE
        };
        (
            b,
            palette::TEXT_PRIMARY,
            if focused {
                palette::ACCENT
            } else {
                palette::BORDER_STRONG
            },
        )
    };
    dl.rect_border(r, 7.0, bg, if focused { 2.0 } else { 1.0 }, border);
    let tw = atlas.measure(FontKind::UiSemi, label);
    dl.text(
        atlas,
        FontKind::UiSemi,
        r.x + (r.w - tw) * 0.5,
        r.y + (r.h - 16.0) * 0.5,
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

fn draw_toggle_pill(
    st: &AppState,
    dl: &mut DrawList,
    atlas: &FontAtlas,
    r: Rect,
    on: bool,
    fid: FocusId,
    focusables: &mut Vec<Focusable>,
) {
    let hovered = r.contains(st.cursor_pos[0], st.cursor_pos[1]);
    let focused = st.focused == Some(fid);
    let bg = if on {
        palette::SUCCESS
    } else if hovered {
        palette::SURFACE_HOVER
    } else {
        palette::CANVAS_SOLID
    };
    let border = if focused {
        palette::ACCENT
    } else if on {
        palette::SUCCESS
    } else {
        palette::BORDER_STRONG
    };
    dl.rect_border(r, 7.0, bg, if focused { 2.0 } else { 1.0 }, border);
    let txt = if on { "ON" } else { "OFF" };
    let tw = atlas.measure(FontKind::UiSemi, txt);
    dl.text(
        atlas,
        FontKind::UiSemi,
        r.x + (r.w - tw) * 0.5,
        r.y + (r.h - 16.0) * 0.5,
        txt,
        if on {
            palette::CANVAS_SOLID
        } else {
            palette::TEXT_PRIMARY
        },
    );
    focusables.push(Focusable {
        id: fid,
        rect: r,
        kind: ControlKind::Toggle(on),
    });
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
        palette::CANVAS_SOLID
    };
    let border = if focused {
        palette::TEXT_PRIMARY
    } else if active {
        palette::ACCENT
    } else {
        palette::BORDER_STRONG
    };
    dl.rect_border(r, 6.0, bg, if focused { 2.0 } else { 1.0 }, border);
    let tw = atlas.measure(FontKind::UiSemi, label);
    dl.text(
        atlas,
        FontKind::UiSemi,
        r.x + (r.w - tw) * 0.5,
        r.y + 7.0,
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

fn draw_cycle_row(
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
        palette::CANVAS_SOLID
    };
    let border = if focused {
        palette::ACCENT
    } else {
        palette::BORDER_STRONG
    };
    dl.rect_border(r, 6.0, bg, if focused { 2.0 } else { 1.0 }, border);
    dl.text(atlas, FontKind::UiSmall, r.x + 10.0, r.y + 8.0, label, palette::TEXT_SECONDARY);
    let vw = atlas.measure(FontKind::UiSemi, value);
    dl.text(
        atlas,
        FontKind::UiSemi,
        r.x + r.w - vw - 10.0,
        r.y + 7.0,
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
    dl.text(atlas, FontKind::UiSmall, r.x, r.y, label, palette::TEXT_SECONDARY);
    let vw = atlas.measure(FontKind::MonoSmall, val_text);
    dl.text(
        atlas,
        FontKind::MonoSmall,
        r.x + r.w - vw,
        r.y,
        val_text,
        palette::TEXT_PRIMARY,
    );

    let track = Rect::new(r.x, r.y + 24.0, r.w, 10.0);
    let focused = st.focused == Some(fid);
    let t = ((val - min) / (max - min).max(0.0001)).clamp(0.0, 1.0);
    dl.rect_border(
        track,
        5.0,
        palette::CANVAS_SOLID,
        if focused { 1.5 } else { 1.0 },
        if focused {
            palette::ACCENT
        } else {
            palette::BORDER_STRONG
        },
    );
    let fill_w = (track.w * t).max(8.0);
    dl.rect(Rect::new(track.x, track.y, fill_w, track.h), 5.0, palette::ACCENT);
    let knob_x = (track.x + track.w * t - 7.0).clamp(track.x, track.x + track.w - 14.0);
    dl.rect_border(
        Rect::new(knob_x, track.y - 3.0, 14.0, 16.0),
        7.0,
        palette::TEXT_PRIMARY,
        1.5,
        palette::CANVAS_SOLID,
    );

    focusables.push(Focusable {
        id: fid,
        rect: Rect::new(r.x, r.y + 14.0, r.w, 28.0),
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
        palette::BORDER_STRONG
    };
    dl.rect_border(
        r,
        7.0,
        palette::CANVAS_SOLID,
        if editing || focused { 1.5 } else { 1.0 },
        border,
    );
    dl.push_scissor(Rect::new(r.x + 8.0, r.y + 2.0, r.w - 16.0, r.h - 4.0));
    if text.is_empty() {
        dl.text(
            atlas,
            FontKind::UiSmall,
            r.x + 10.0,
            r.y + 8.0,
            placeholder,
            palette::TEXT_SECONDARY,
        );
    } else {
        dl.text(
            atlas,
            FontKind::UiBody,
            r.x + 10.0,
            r.y + 7.0,
            text,
            palette::TEXT_PRIMARY,
        );
        if editing {
            let cw = atlas.measure(FontKind::UiBody, text);
            dl.rect(
                Rect::new(r.x + 11.0 + cw, r.y + 6.0, 1.5, r.h - 12.0),
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

    // 1. Mouse clicks
    if st.mouse_just_pressed {
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
                    ControlKind::Button | ControlKind::Toggle(_) => {
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

    // 2. Typed text characters
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

    // 3. Keyboard navigation & activation (R-32)
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
        FocusId::AutoClickerCpsSlider => {
            st.config.auto_clicker_cps = snapped.round() as u32;
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
        FocusId::AutoClickerCpsSlider => {
            let next = (st.config.auto_clicker_cps as f32 + delta).clamp(min, max);
            st.config.auto_clicker_cps = next.round() as u32;
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
            st.config.auto_sprint = false;
            st.config.auto_respawn = false;
            st.config.auto_eat = false;
            st.config.auto_totem = false;
            st.config.auto_fish = false;
            st.config.trigger_bot = false;
            st.config.auto_clicker = false;
            st.config.anti_afk = false;
            st.lightmap_dirty = true;
            st.set_toast("Panic: all survival/combat modules disabled");
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
        FocusId::QuickFullbright | FocusId::FullbrightToggle => {
            st.config.fullbright_enabled = !st.config.fullbright_enabled;
            st.lightmap_dirty = true;
        }
        FocusId::FullbrightModeMax => {
            st.config.fullbright_mode = FullbrightMode::Max;
            st.lightmap_dirty = true;
        }
        FocusId::FullbrightModeSoft => {
            st.config.fullbright_mode = FullbrightMode::Soft;
            st.lightmap_dirty = true;
        }
        FocusId::XrayTerrainToggle => {
            st.config.xray_terrain = !st.config.xray_terrain;
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
        FocusId::XrayEntitiesToggle => {
            st.config.xray_entities = !st.config.xray_entities;
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
