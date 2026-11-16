use crate::config::KestrelConfig;
use crate::control_bridge::LiveGameState;
use std::sync::LazyLock;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavTab {
    Survival,
    Automation,
    Hud,
    Waypoints,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiKey {
    Tab,
    ShiftTab,
    Enter,
    Space,
    Escape,
    Backspace,
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragWidget {
    Coords,
    Fps,
    Keystrokes,
    Vitals,
    Compass,
    Clock,
    Radar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextTarget {
    Search,
    WaypointName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusId {
    SearchBox,
    QuickFullbright,
    EditHudBtn,
    CloseBtn,
    PanicBtn,
    TabAutomation,
    TabSurvival,
    TabHud,
    TabWaypoints,
    TabSystem,
    // Survival controls
    XrayTerrainToggle,
    XrayEntitiesToggle,
    XrayKeyCycle,
    XrayOresToggle,
    XrayOrePresetCycle,
    XrayRadiusSlider,
    EspToggle,
    TracersToggle,
    EspPlayersToggle,
    EspHostileToggle,
    EspPassiveToggle,
    EspItemsToggle,
    EspChestsToggle,
    FreecamToggle,
    FreecamSpeedSlider,
    AimAssistToggle,
    AimTargetCycle,
    AimFovSlider,
    AimRangeSlider,
    AimSmoothSlider,
    KillAuraToggle,
    KillAuraTargetCycle,
    KillAuraRangeSlider,
    KillAuraFovSlider,
    KillAuraDelaySlider,
    CriticalsToggle,
    FullbrightToggle,
    FullbrightModeMax,
    FullbrightModeSoft,
    FullbrightStrength,
    ZoomToggle,
    ZoomFovSlider,
    ZoomKeyCycle,
    CrosshairToggle,
    CrosshairStyleCycle,
    CrosshairSizeSlider,
    CrosshairGapSlider,
    GammaSlider,
    // HUD controls
    HudEnterEdit,
    HudResetLayout,
    HudToggleCoords,
    HudToggleNether,
    HudToggleFps,
    HudToggleKeystrokes,
    HudToggleVitals,
    HudToggleCompass,
    HudToggleClock,
    HudToggleRadar,
    AutoSprintToggle,
    AutoRespawnToggle,
    AutoEatToggle,
    AutoEatHungerSlider,
    AutoTotemToggle,
    AutoFishToggle,
    TriggerBotToggle,
    TriggerBotDelaySlider,
    AutoClickerToggle,
    AutoClickerCpsSlider,
    AntiAfkToggle,
    AutoMineToggle,
    SpeedMineToggle,
    SpeedMinePowerSlider,
    AutoToolToggle,
    // Waypoints controls
    WaypointInput,
    WaypointAddHere,
    WaypointItemToggle(usize),
    WaypointItemDelete(usize),
    // System controls
    SystemSaveConfig,
    SystemResetAll,
    PerfProfileCycle,
}

pub struct AppState {
    pub config: KestrelConfig,
    pub menu_open: bool,
    pub hud_edit_mode: bool,
    pub active_tab: NavTab,

    pub search_query: String,
    pub waypoint_draft_name: String,
    pub active_text_target: Option<TextTarget>,
    pub focused: Option<FocusId>,
    pub scroll_y: f32,
    pub max_scroll_y: f32,

    pub cursor_pos: [f32; 2],
    pub mouse_down: bool,
    pub mouse_just_pressed: bool,
    pub mouse_just_released: bool,
    pub scroll_delta: f32,
    pub pending_chars: Vec<char>,
    pub pending_keys: Vec<UiKey>,
    pub active_slider: Option<FocusId>,
    pub menu_pos: Option<[f32; 2]>,
    pub dragging_menu: Option<[f32; 2]>,
    pub dragging_widget: Option<(DragWidget, [f32; 2])>,

    // Telemetry from input hook (live even when menu is closed)
    pub key_w: bool,
    pub key_a: bool,
    pub key_s: bool,
    pub key_d: bool,
    pub key_space: bool,
    pub lmb_down: bool,
    pub xray_key_down: bool,
    pub rmb_down: bool,
    pub lmb_times: VecDeque<Instant>,
    pub rmb_times: VecDeque<Instant>,
    pub zoom_held: bool,

    // Frame timing telemetry
    pub last_frame: Instant,
    pub frame_times_ms: VecDeque<f32>,
    pub fps: f32,
    pub frame_ms: f32,
    pub low_1pct_fps: f32,
    pub screen_size: [u32; 2],
    pub gpu_name: String,

    // Lightmap tracking
    pub lightmap_hooked: bool,
    pub lightmap_images_seen: usize,
    pub lightmap_dirty: bool,

    pub freecam_offset: [f32; 3],
    pub last_freecam_step: Instant,
    // UI feedback / animation
    pub session_start: Instant,
    pub menu_anim: f32,
    pub toast: Option<(String, Instant)>,

    // Live Horizon state from control socket
    pub game: LiveGameState,
}

impl AppState {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            config: KestrelConfig::load(),
            menu_open: false,
            hud_edit_mode: false,
            active_tab: NavTab::Survival,

            search_query: String::new(),
            waypoint_draft_name: String::new(),
            active_text_target: None,
            focused: Some(FocusId::FullbrightToggle),
            scroll_y: 0.0,
            max_scroll_y: 0.0,

            cursor_pos: [640.0, 360.0],
            mouse_down: false,
            mouse_just_pressed: false,
            mouse_just_released: false,
            scroll_delta: 0.0,
            pending_chars: Vec::new(),
            pending_keys: Vec::new(),
            active_slider: None,
            dragging_widget: None,

            menu_pos: None,
            dragging_menu: None,
            key_w: false,
            key_a: false,
            key_s: false,
            key_d: false,
            key_space: false,
            lmb_down: false,
            rmb_down: false,
            lmb_times: VecDeque::with_capacity(64),
            rmb_times: VecDeque::with_capacity(64),
            zoom_held: false,

            last_frame: now,
            xray_key_down: false,
            frame_times_ms: VecDeque::with_capacity(240),
            fps: 0.0,
            frame_ms: 0.0,
            low_1pct_fps: 0.0,
            screen_size: [1280, 720],
            gpu_name: String::from("Vulkan GPU"),

            lightmap_hooked: false,
            lightmap_images_seen: 0,
            lightmap_dirty: false,

            session_start: now,
            freecam_offset: [0.0; 3],
            last_freecam_step: now,
            menu_anim: 0.0,
            toast: None,

            game: LiveGameState::default(),
        }
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn record_frame_time(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if dt > 0.0001 && dt < 2.0 {
            let ms = dt * 1000.0;
            self.frame_ms = if self.frame_ms == 0.0 {
                ms
            } else {
                self.frame_ms * 0.9 + ms * 0.1
            };
            self.frame_times_ms.push_back(ms);
            while self.frame_times_ms.len() > 180 {
                self.frame_times_ms.pop_front();
            }
            let avg_ms: f32 =
                self.frame_times_ms.iter().sum::<f32>() / self.frame_times_ms.len().max(1) as f32;
            self.fps = if avg_ms > 0.01 { 1000.0 / avg_ms } else { 0.0 };

            let mut sorted: Vec<f32> = self.frame_times_ms.iter().copied().collect();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            let top_count = (sorted.len() / 100).max(1);
            let worst_ms: f32 = sorted[..top_count].iter().sum::<f32>() / top_count as f32;
            self.low_1pct_fps = if worst_ms > 0.01 {
                1000.0 / worst_ms
            } else {
                self.fps
            };
        }

        // Smooth open/close progress (175ms open, 140ms close)
        let target = if self.menu_open { 1.0 } else { 0.0 };
        let dur = if self.menu_open { 0.175 } else { 0.14 };
        let step = (dt / dur).min(1.0);
        if self.menu_anim < target {
            self.menu_anim = (self.menu_anim + step).min(target);
        } else if self.menu_anim > target {
            self.menu_anim = (self.menu_anim - step).max(target);
        }
    }
    pub fn cps(&mut self) -> (usize, usize) {
        let now = Instant::now();
        while self
            .lmb_times
            .front()
            .is_some_and(|t| now.duration_since(*t).as_secs_f32() > 1.0)
        {
            self.lmb_times.pop_front();
        }
        while self
            .rmb_times
            .front()
            .is_some_and(|t| now.duration_since(*t).as_secs_f32() > 1.0)
        {
            self.rmb_times.pop_front();
        }
        (self.lmb_times.len(), self.rmb_times.len())
    }
}

pub static STATE: LazyLock<Mutex<AppState>> = LazyLock::new(|| Mutex::new(AppState::new()));
