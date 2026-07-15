// crates/qbzd/src/tui/app.rs — the setup-TUI state machine.
//
// Owns the six screens (D7 hard cap: `const SCREENS: [Screen; 6]`), the route,
// the dirty-save model (§4), the App-level overlays (help / result panel /
// dirty-leave modal), and the worker plumbing (§5.5: NO I/O on keystrokes — disk
// and HTTP happen only at screen entry, `r`, save, and the immediate actions, on
// a worker with a spinner). Persistence is reused wholesale: saves go through
// T11's `write_one`, import/export through the T12 bundle engine, auth through
// the T5 login engine. The TUI adds no persistence of its own (03 §6).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use serde_json::{json, Value};
use tokio::runtime::Handle;

use qbz_app::settings::bundle::{self, Bundle, ExportOptions, ExportSource, ImportOptions, LiveSystem, ProfilePaths};
use qbz_app::settings::daemon_prefs;
use qbz_app::settings::playback::PlaybackPreferencesStore;
use qbz_audio::settings::{AudioSettings, AudioSettingsStore};
use qbz_audio::{AudioBackendType, AudioDevice, BackendManager};

use crate::cli::client::{ApiClient, CliError};
use crate::config::QbzdConfig;
use crate::login;
use crate::paths::ProfileRoots;
use crate::qconnect::transport as qconnect_kv;

use super::screens::account::{AccountState, AuthSnapshot};
use super::screens::audio::AudioState;
use super::screens::bundle::{BundleState, PendingImport};
use super::screens::network::{self as network_screen, NetworkState};
use super::screens::playback::PlaybackState;
use super::screens::qconnect::QConnectState;
use super::strings as s;
use super::theme;
use super::widgets;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

// ============================ shared vocabulary ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Account,
    Audio,
    Playback,
    QConnect,
    Network,
    Bundle,
}

/// D7 hard cap — a seventh screen needs an owner decision, not a PR.
pub const SCREENS: [Screen; 6] = [
    Screen::Account,
    Screen::Audio,
    Screen::Playback,
    Screen::QConnect,
    Screen::Network,
    Screen::Bundle,
];

/// Construct the path to the OAuth token file in the config root.
fn cred_file_path(config_root: &PathBuf) -> PathBuf {
    config_root.join(".qbz-oauth-token")
}

/// Determine the initial landing screen at startup (03 §2.2).
///
/// Landing rules:
/// - Credential file present → None (land on Menu)
/// - Credential file absent → Some(Screen::Account) (land on Account setup)
///
/// The decision is based on credential-file presence, not live daemon auth state.
/// The `logged_in` parameter is passed for test clarity (three cases) but does not
/// affect the decision.
fn initial_screen(cred_file_present: bool, _logged_in: bool) -> Option<Screen> {
    if cred_file_present {
        None  // land on Menu
    } else {
        Some(Screen::Account)  // land on Account
    }
}

/// The intent a screen's key handler returns to the App.
pub enum ScreenAction {
    Consumed,
    Save,
    Back,
    RefreshDevices,
    LoginBrowser,
    LoginToken(String),
    Logout,
    ImportPlan(String),
    ImportApply,
    Export { dest: String, include_auth: bool },
}

/// Read-only context passed to every screen's `draw` (the live status body for
/// the screens that render a daemon-state line).
pub struct DrawCtx<'a> {
    pub status: Option<&'a Value>,
}

/// What the event loop must do after handling a key (terminal-control cases).
pub enum LoopCmd {
    None,
    /// Suspend the alt-screen and run the T5 browser-login engine on the plain
    /// terminal, then resume (see the task report for this deliberate divergence).
    BrowserLogin,
}

// ============================ worker messages ============================

pub enum Msg {
    Devices(Result<Vec<AudioDevice>, String>),
    Saved { lines: Vec<String>, status: Option<Value>, reachable: bool, success: bool },
    TokenLogin(Result<(String, Option<String>), String>),
    ImportPlanned(Result<Box<PendingImport>, String>),
    ImportApplied { lines: Vec<String>, status: Option<Value>, reachable: bool },
    Exported(Result<Vec<String>, String>),
}

// ============================ active screen ============================

enum Active {
    Menu,
    Account(AccountState),
    Audio(AudioState),
    Playback(PlaybackState),
    QConnect(QConnectState),
    Network(NetworkState),
    Bundle(BundleState),
}

enum Overlay {
    None,
    Help,
    Result { title: String, lines: Vec<String> },
    DirtyLeave { target: LeaveTarget },
}

#[derive(Clone, Copy)]
enum LeaveTarget {
    Menu,
    Quit,
}

// ============================ App ============================

pub struct App {
    roots: ProfileRoots,
    handle: Handle,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,

    active: Active,
    menu_focus: usize,
    menu_summaries: Vec<String>,

    status: Option<Value>,
    reachable: bool,
    auth: AuthSnapshot,

    overlay: Overlay,
    busy: Option<String>,
    pub busy_tick: u64,
    should_quit: bool,
    /// Set when a save was requested from the dirty-leave modal — the leave
    /// happens once the save succeeds (§4.1 Save/Discard/Stay → Save then leave).
    leave_after_save: Option<LeaveTarget>,
}

impl App {
    pub fn new(roots: ProfileRoots, handle: Handle) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut app = App {
            roots: roots.clone(),
            handle,
            tx,
            rx,
            active: Active::Menu,
            menu_focus: 0,
            menu_summaries: vec![String::new(); 6],
            status: None,
            reachable: false,
            auth: AuthSnapshot::default(),
            overlay: Overlay::None,
            busy: None,
            busy_tick: 0,
            should_quit: false,
            leave_after_save: None,
        };
        app.refresh_status();
        // Determine landing screen per spec (03 §2.2):
        // - Credential file exists (any state) → land on Menu (user has auth credentials)
        // - No credential file → land on Account (needs auth setup)
        let cred_file_exists = cred_file_path(&roots.config).exists();
        if let Some(screen) = initial_screen(cred_file_exists, app.auth.logged_in) {
            app.enter_screen(screen);
        } else {
            app.refresh_menu();
        }
        app
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }
    pub fn busy(&self) -> bool {
        self.busy.is_some()
    }

    // -------------------------- status / auth --------------------------

    fn refresh_status(&mut self) {
        let roots = self.roots.clone();
        let body = self.handle.block_on(fetch_status(roots));
        self.reachable = body.is_some();
        self.status = body;
        self.auth = self.derive_auth();
    }

    /// Resolve auth from live status (daemon up) or credential-file presence
    /// (daemon down) — NEVER fabricating a name offline (§3.1).
    fn derive_auth(&self) -> AuthSnapshot {
        if self.reachable {
            if let Some(st) = &self.status {
                let state = st.pointer("/auth/state").and_then(Value::as_str).unwrap_or("");
                if state == "logged_in" {
                    let id = st.pointer("/auth/user_id").and_then(Value::as_u64);
                    let plan = st
                        .pointer("/auth/subscription")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    return AuthSnapshot {
                        logged_in: true,
                        email: id.map(|i| format!("user {i}")),
                        plan,
                        cred_file_present: true,
                    };
                }
                return AuthSnapshot::default();
            }
        }
        // Offline: only report credential-file presence.
        let cred = self.roots.config.join(".qbz-oauth-token").exists();
        AuthSnapshot {
            logged_in: false,
            email: None,
            plan: None,
            cred_file_present: cred,
        }
    }

    // -------------------------- navigation --------------------------

    fn enter_screen(&mut self, screen: Screen) {
        self.active = match screen {
            Screen::Account => Active::Account(AccountState::new(self.auth.clone())),
            Screen::Audio => {
                let audio = load_audio(&self.roots);
                let mut st = AudioState::new(&audio);
                st.start_scan();
                let backend = st.backend();
                self.spawn_devices(backend);
                Active::Audio(st)
            }
            Screen::Playback => {
                let audio = load_audio(&self.roots);
                let playback = PlaybackPreferencesStore::new_at(&self.roots.data)
                    .and_then(|s| s.get_preferences())
                    .unwrap_or_default();
                let quality = daemon_prefs::load_at(&self.roots.data).streaming_quality;
                Active::Playback(PlaybackState::new(&quality, &audio, &playback))
            }
            Screen::QConnect => {
                let db = self.roots.data.join("qconnect_settings.db");
                let on = matches!(
                    qconnect_kv::load_startup_mode_at(&db),
                    qconnect_app::QconnectStartupMode::On
                );
                let name = qconnect_kv::load_device_name_at(&db);
                let vol = qconnect_kv::load_volume_mode_at(&db);
                Active::QConnect(QConnectState::new(on, name, vol))
            }
            Screen::Network => {
                let (cfg, warns) = QbzdConfig::load(&self.roots.config.join("qbzd.toml"))
                    .unwrap_or_else(|_| (QbzdConfig::default(), Vec::new()));
                Active::Network(NetworkState::new(&cfg, warns))
            }
            Screen::Bundle => Active::Bundle(BundleState::new(desktop_profile_present())),
        };
    }

    fn refresh_menu(&mut self) {
        self.refresh_status();
        self.menu_summaries = self.compute_summaries();
    }

    fn compute_summaries(&self) -> Vec<String> {
        let audio = load_audio(&self.roots);
        let playback = PlaybackPreferencesStore::new_at(&self.roots.data)
            .and_then(|s| s.get_preferences())
            .unwrap_or_default();
        let quality = daemon_prefs::load_at(&self.roots.data).streaming_quality;
        let db = self.roots.data.join("qconnect_settings.db");
        let qc_on = matches!(
            qconnect_kv::load_startup_mode_at(&db),
            qconnect_app::QconnectStartupMode::On
        );
        let qc_name = qconnect_kv::load_device_name_at(&db);
        let qc_vol = qconnect_kv::load_volume_mode_at(&db);
        let (cfg, _) = QbzdConfig::load(&self.roots.config.join("qbzd.toml"))
            .unwrap_or_else(|_| (QbzdConfig::default(), Vec::new()));

        vec![
            AccountState::new(self.auth.clone()).summary(),
            AudioState::new(&audio).summary(),
            PlaybackState::new(&quality, &audio, &playback).summary(),
            QConnectState::new(qc_on, qc_name, qc_vol).summary(),
            NetworkState::new(&cfg, Vec::new()).summary(),
            "bundle tools…".to_string(),
        ]
    }

    fn leave_screen(&mut self, target: LeaveTarget) {
        if self.active_is_dirty() {
            self.overlay = Overlay::DirtyLeave { target };
            return;
        }
        self.apply_leave(target);
    }

    fn apply_leave(&mut self, target: LeaveTarget) {
        match target {
            LeaveTarget::Menu => {
                self.active = Active::Menu;
                self.refresh_menu();
            }
            LeaveTarget::Quit => self.should_quit = true,
        }
    }

    fn active_is_dirty(&self) -> bool {
        match &self.active {
            Active::Audio(s) => s.is_dirty(),
            Active::Playback(s) => s.is_dirty(),
            Active::QConnect(s) => s.is_dirty(),
            Active::Network(s) => s.is_dirty(),
            _ => false,
        }
    }

    fn active_is_editing(&self) -> bool {
        match &self.active {
            Active::Account(s) => s.is_editing(),
            Active::Audio(s) => s.is_editing(),
            Active::Playback(s) => s.is_editing(),
            Active::QConnect(s) => s.is_editing(),
            Active::Network(s) => s.is_editing(),
            Active::Bundle(s) => s.is_editing(),
            Active::Menu => false,
        }
    }

    // -------------------------- key handling --------------------------

    pub fn on_key(&mut self, key: KeyEvent) -> LoopCmd {
        if self.busy.is_some() {
            return LoopCmd::None; // §5.5: input parked while a worker runs
        }
        // Overlays capture keys first.
        match &self.overlay {
            Overlay::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
                    self.overlay = Overlay::None;
                }
                return LoopCmd::None;
            }
            Overlay::Result { .. } => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    self.overlay = Overlay::None;
                }
                return LoopCmd::None;
            }
            Overlay::DirtyLeave { target } => {
                let target = *target;
                match key.code {
                    KeyCode::Char('s') => {
                        self.overlay = Overlay::None;
                        self.save_active(Some(target));
                    }
                    KeyCode::Char('d') => {
                        self.overlay = Overlay::None;
                        self.apply_leave(target);
                    }
                    KeyCode::Esc => self.overlay = Overlay::None,
                    _ => {}
                }
                return LoopCmd::None;
            }
            Overlay::None => {}
        }

        if let Active::Menu = self.active {
            return self.on_menu_key(key);
        }

        // Global keys when the screen is at top level (not editing a field).
        if !self.active_is_editing() {
            match key.code {
                KeyCode::Char('?') => {
                    self.overlay = Overlay::Help;
                    return LoopCmd::None;
                }
                KeyCode::Char('q') => {
                    self.leave_screen(LeaveTarget::Quit);
                    return LoopCmd::None;
                }
                _ => {}
            }
        }

        let action = self.dispatch_screen_key(key);
        self.handle_screen_action(action)
    }

    fn on_menu_key(&mut self, key: KeyEvent) -> LoopCmd {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.menu_focus = if self.menu_focus == 0 { SCREENS.len() - 1 } else { self.menu_focus - 1 };
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.menu_focus = (self.menu_focus + 1) % SCREENS.len();
            }
            KeyCode::Enter => self.enter_screen(SCREENS[self.menu_focus]),
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
        LoopCmd::None
    }

    fn dispatch_screen_key(&mut self, key: KeyEvent) -> ScreenAction {
        match &mut self.active {
            Active::Account(s) => s.handle_key(key),
            Active::Audio(s) => s.handle_key(key),
            Active::Playback(s) => s.handle_key(key),
            Active::QConnect(s) => s.handle_key(key),
            Active::Network(s) => s.handle_key(key),
            Active::Bundle(s) => s.handle_key(key),
            Active::Menu => ScreenAction::Consumed,
        }
    }

    fn handle_screen_action(&mut self, action: ScreenAction) -> LoopCmd {
        match action {
            ScreenAction::Consumed => LoopCmd::None,
            ScreenAction::Save => {
                self.save_active(None);
                LoopCmd::None
            }
            ScreenAction::Back => {
                self.leave_screen(LeaveTarget::Menu);
                LoopCmd::None
            }
            ScreenAction::RefreshDevices => {
                if let Active::Audio(s) = &self.active {
                    let backend = s.backend();
                    self.spawn_devices(backend);
                }
                LoopCmd::None
            }
            ScreenAction::LoginBrowser => LoopCmd::BrowserLogin,
            ScreenAction::LoginToken(token) => {
                self.spawn_token_login(token);
                LoopCmd::None
            }
            ScreenAction::Logout => {
                self.do_logout();
                LoopCmd::None
            }
            ScreenAction::ImportPlan(path) => {
                self.spawn_import_plan(path);
                LoopCmd::None
            }
            ScreenAction::ImportApply => {
                self.spawn_import_apply();
                LoopCmd::None
            }
            ScreenAction::Export { dest, include_auth } => {
                self.spawn_export(dest, include_auth);
                LoopCmd::None
            }
        }
    }

    // -------------------------- saves --------------------------

    fn save_active(&mut self, then_leave: Option<LeaveTarget>) {
        let (keys, network) = match &self.active {
            Active::Audio(s) => (s.save_keys(), None),
            Active::Playback(s) => (s.save_keys(), None),
            Active::QConnect(s) => (s.save_keys(), None),
            Active::Network(s) => match s.validated() {
                Ok(v) => (Vec::new(), Some(v)),
                Err(e) => {
                    self.overlay = Overlay::Result {
                        title: s::SAVE_TITLE.to_string(),
                        lines: vec![format!("cannot save: {e}")],
                    };
                    return;
                }
            },
            _ => return, // Account / Bundle / Menu never save
        };

        if keys.is_empty() && network.is_none() {
            // Nothing changed — just leave if that was the intent.
            if let Some(t) = then_leave {
                self.apply_leave(t);
            }
            return;
        }

        // The baseline is updated only on a SUCCESSFUL write (§4.2: a failed
        // store write leaves the screen dirty). Input is parked while busy, so
        // the staged form cannot change under the async save.
        self.leave_after_save = then_leave;
        self.busy = Some("saving…".to_string());
        let roots = self.roots.clone();
        let tx = self.tx.clone();
        let is_network = network.is_some();
        self.handle.spawn(async move {
            let (write_err, reinit) = if let Some((bind, port, token)) = network {
                (save_network(&roots, &bind, port, token.as_deref()), false)
            } else {
                write_keys(&roots, &keys)
            };
            let success = write_err.is_none();
            if let Some(err) = write_err {
                // Store write failed — do not touch the daemon; report the fault.
                let _ = tx.send(Msg::Saved {
                    lines: vec![err],
                    status: None,
                    reachable: true,
                    success: false,
                });
                return;
            }
            let (lines, status, reachable) = do_reload(&roots, is_network, reinit).await;
            let _ = tx.send(Msg::Saved { lines, status, reachable, success });
        });
    }

    // -------------------------- immediate actions --------------------------

    fn spawn_devices(&mut self, backend: AudioBackendType) {
        let tx = self.tx.clone();
        self.handle.spawn_blocking(move || {
            let _ = tx.send(Msg::Devices(enumerate_devices(backend)));
        });
    }

    fn spawn_token_login(&mut self, token: String) {
        self.busy = Some(s::ACCOUNT_VALIDATING.to_string());
        let roots = self.roots.clone();
        let tx = self.tx.clone();
        self.handle.spawn(async move {
            let res = login::login_with_token_arg(&roots, &token)
                .await
                .map(|session| (session.email, Some(session.subscription_label)))
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::TokenLogin(res));
        });
    }

    fn do_logout(&mut self) {
        match login::logout(&self.roots) {
            Ok(_) => {
                self.auth = AuthSnapshot {
                    logged_in: false,
                    email: None,
                    plan: None,
                    cred_file_present: false,
                };
                if let Active::Account(s) = &mut self.active {
                    s.set_auth(self.auth.clone());
                }
                self.overlay = Overlay::Result {
                    title: s::ACCOUNT_TITLE.to_string(),
                    lines: vec!["logged out".to_string()],
                };
            }
            Err(e) => {
                self.overlay = Overlay::Result {
                    title: s::ACCOUNT_TITLE.to_string(),
                    lines: vec![e.to_string()],
                };
            }
        }
    }

    fn spawn_import_plan(&mut self, path: String) {
        self.busy = Some("reading bundle…".to_string());
        let roots = self.roots.clone();
        let tx = self.tx.clone();
        self.handle.spawn_blocking(move || {
            let _ = tx.send(Msg::ImportPlanned(plan_import(&roots, &path).map(Box::new)));
        });
    }

    fn spawn_import_apply(&mut self) {
        let ctx = match &self.active {
            Active::Bundle(s) => s.apply_context(),
            _ => None,
        };
        let Some((bundle, target, live, mut opts, choice, with_auth)) = ctx else {
            return;
        };
        self.busy = Some("applying import…".to_string());
        let roots = self.roots.clone();
        let tx = self.tx.clone();
        self.handle.spawn(async move {
            opts.include_auth = with_auth;
            let msg = apply_import(&roots, bundle, target, live, opts, choice).await;
            let _ = tx.send(msg);
        });
    }

    fn spawn_export(&mut self, dest: String, include_auth: bool) {
        self.busy = Some("exporting…".to_string());
        let roots = self.roots.clone();
        let tx = self.tx.clone();
        self.handle.spawn_blocking(move || {
            let _ = tx.send(Msg::Exported(export_bundle(&roots, &dest, include_auth)));
        });
    }

    // -------------------------- worker results --------------------------

    pub fn drain_worker(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.on_msg(msg);
        }
    }

    fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Devices(result) => {
                if let Active::Audio(s) = &mut self.active {
                    s.set_devices(result);
                }
            }
            Msg::Saved { lines, status, reachable, success } => {
                self.busy = None;
                self.reachable = reachable;
                if status.is_some() {
                    self.status = status;
                }
                self.overlay = Overlay::Result {
                    title: s::SAVE_TITLE.to_string(),
                    lines,
                };
                if success {
                    // §4.1: the staged form becomes the baseline (dirty clears).
                    match &mut self.active {
                        Active::Audio(sc) => sc.mark_saved(),
                        Active::Playback(sc) => sc.mark_saved(),
                        Active::QConnect(sc) => sc.mark_saved(),
                        Active::Network(sc) => sc.mark_saved(),
                        _ => {}
                    }
                    // Dirty-leave "Save" → leave once the save landed (§4.1).
                    if let Some(target) = self.leave_after_save.take() {
                        self.apply_leave(target);
                    }
                } else {
                    // §4.2: a failed write leaves the screen dirty; do not leave.
                    self.leave_after_save = None;
                }
            }
            Msg::TokenLogin(result) => {
                self.busy = None;
                match result {
                    Ok((email, plan)) => {
                        self.auth = AuthSnapshot {
                            logged_in: true,
                            email: Some(email.clone()),
                            plan: plan.clone(),
                            cred_file_present: true,
                        };
                        if let Active::Account(st) = &mut self.active {
                            st.set_auth(self.auth.clone());
                        }
                        self.overlay = Overlay::Result {
                            title: s::ACCOUNT_TITLE.to_string(),
                            lines: vec![s::account_logged_in(&email)],
                        };
                    }
                    Err(e) => {
                        self.overlay = Overlay::Result {
                            title: s::ACCOUNT_TITLE.to_string(),
                            lines: e.lines().map(str::to_string).collect(),
                        };
                    }
                }
            }
            Msg::ImportPlanned(result) => {
                self.busy = None;
                match result {
                    Ok(pending) => {
                        if let Active::Bundle(s) = &mut self.active {
                            s.set_plan(*pending);
                        }
                    }
                    Err(e) => {
                        self.overlay = Overlay::Result {
                            title: s::BUNDLE_TITLE.to_string(),
                            lines: e.lines().map(str::to_string).collect(),
                        };
                    }
                }
            }
            Msg::ImportApplied { lines, status, reachable } => {
                self.busy = None;
                self.reachable = reachable;
                if status.is_some() {
                    self.status = status;
                }
                if let Active::Bundle(s) = &mut self.active {
                    s.clear_pending();
                }
                // A bundle may have logged us in — refresh auth.
                self.auth = self.derive_auth();
                self.overlay = Overlay::Result {
                    title: s::BUNDLE_TITLE.to_string(),
                    lines,
                };
            }
            Msg::Exported(result) => {
                self.busy = None;
                let lines = match result {
                    Ok(lines) => lines,
                    Err(e) => e.lines().map(str::to_string).collect(),
                };
                self.overlay = Overlay::Result {
                    title: s::BUNDLE_TITLE.to_string(),
                    lines,
                };
            }
        }
    }

    /// Called by the loop after it runs the suspended browser-login engine.
    pub fn after_browser_login(&mut self, result: Result<(String, Option<String>), String>) {
        match result {
            Ok((email, plan)) => {
                self.auth = AuthSnapshot {
                    logged_in: true,
                    email: Some(email.clone()),
                    plan: plan.clone(),
                    cred_file_present: true,
                };
                if let Active::Account(st) = &mut self.active {
                    st.set_auth(self.auth.clone());
                }
                self.overlay = Overlay::Result {
                    title: s::ACCOUNT_TITLE.to_string(),
                    lines: vec![s::account_logged_in(&email)],
                };
            }
            Err(e) => {
                self.overlay = Overlay::Result {
                    title: s::ACCOUNT_TITLE.to_string(),
                    lines: e.lines().map(str::to_string).collect(),
                };
            }
        }
    }

    pub fn roots(&self) -> &ProfileRoots {
        &self.roots
    }

    // -------------------------- render --------------------------

    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        if area.width < 80 || area.height < 24 {
            let msg = s::too_small(area.width, area.height);
            f.render_widget(Paragraph::new(msg), area);
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1), Constraint::Length(1)])
            .split(area);
        let content_area = rows[0];
        let footer_area = rows[1];
        let help_area = rows[2];

        // The outer frame stays neutral (dim) so the accent-bordered ACTIVE inner
        // section is what draws the eye; the accent-bold title carries identity.
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(theme::dim())
            .title(self.screen_title_line());
        let inner = block.inner(content_area);
        f.render_widget(block, content_area);

        let ctx = DrawCtx {
            status: self.status.as_ref(),
        };
        match &self.active {
            Active::Menu => self.draw_menu(f, inner),
            Active::Account(sc) => sc.draw(f, inner, &ctx),
            Active::Audio(sc) => sc.draw(f, inner, &ctx),
            Active::Playback(sc) => sc.draw(f, inner, &ctx),
            Active::QConnect(sc) => sc.draw(f, inner, &ctx),
            Active::Network(sc) => sc.draw(f, inner, &ctx),
            Active::Bundle(sc) => sc.draw(f, inner, &ctx),
        }

        self.draw_footer(f, footer_area);
        widgets::help_bar(f, help_area, self.help_text());

        match &self.overlay {
            Overlay::Help => widgets::panel(
                f,
                area,
                s::HELP_TITLE,
                s::HELP_OVERLAY.lines().map(|l| Line::from(l.to_string())).collect(),
                0,
            ),
            Overlay::Result { title, lines } => {
                let body = lines.join("\n");
                widgets::modal(f, area, title, &body, s::RESULT_HINT);
            }
            Overlay::DirtyLeave { .. } => {
                widgets::modal(f, area, s::DIRTY_TITLE, s::DIRTY_BODY, s::DIRTY_HINT);
            }
            Overlay::None => {}
        }

        if let Some(label) = &self.busy {
            widgets::busy_overlay(f, area, label, self.busy_tick);
        }
    }

    fn screen_name_dirty(&self) -> (&'static str, bool) {
        match &self.active {
            Active::Menu => (s::MENU_TITLE, false),
            Active::Account(_) => (s::ACCOUNT_TITLE, false),
            Active::Audio(sc) => (s::AUDIO_TITLE, sc.is_dirty()),
            Active::Playback(sc) => (s::PLAYBACK_TITLE, sc.is_dirty()),
            Active::QConnect(sc) => (s::QCONNECT_TITLE, sc.is_dirty()),
            Active::Network(sc) => (s::NETWORK_TITLE, sc.is_dirty()),
            Active::Bundle(_) => (s::BUNDLE_TITLE, false),
        }
    }

    /// The frame title: accent-bold screen name, plus a warn `*` when the screen
    /// has unsaved edits (the `*` glyph is the meaning; the color reinforces it).
    fn screen_title_line(&self) -> Line<'static> {
        let (name, dirty) = self.screen_name_dirty();
        let mut spans = vec![Span::styled(format!(" {name} "), theme::accent_bold())];
        if dirty {
            spans.push(Span::styled("* ", theme::warn()));
        }
        Line::from(spans)
    }

    fn draw_menu(&self, f: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        for (i, label) in s::MENU_ROWS.iter().enumerate() {
            let focused = i == self.menu_focus;
            let summary = self.menu_summaries.get(i).map(String::as_str).unwrap_or("");
            let (marker, marker_style, label_style) = if focused {
                ("▸ ", theme::accent(), theme::accent_bold())
            } else {
                ("  ", Style::default(), Style::default())
            };
            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(format!("{label:<20}"), label_style),
                Span::styled(format!("  {summary}"), theme::dim()),
            ]));
        }
        f.render_widget(Paragraph::new(lines), area);
    }

    /// The daemon-state footer, color-coded via `footer_state`. Never color
    /// alone — every state spells itself out.
    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let playing = self.status.as_ref().and_then(playing_extra);
        let (text, style) = footer_state(self.reachable, self.auth.logged_in, playing);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text, style))),
            area,
        );
    }

    fn help_text(&self) -> &'static str {
        match &self.active {
            Active::Menu => s::HELP_MENU,
            Active::Audio(sc) => {
                if sc.is_dirty() {
                    s::HELP_AUDIO_DIRTY
                } else {
                    s::HELP_AUDIO_CLEAN
                }
            }
            _ => {
                if self.active_is_dirty() {
                    s::HELP_SCREEN_DIRTY
                } else {
                    s::HELP_SCREEN_CLEAN
                }
            }
        }
    }
}

// ============================ worker functions ============================

async fn fetch_status(roots: ProfileRoots) -> Option<Value> {
    let client = ApiClient::new(None, &roots);
    client.get("/api/status").await.ok()
}

fn enumerate_devices(backend: AudioBackendType) -> Result<Vec<AudioDevice>, String> {
    BackendManager::create_backend(backend)
        .and_then(|b| b.enumerate_devices())
        .map_err(|e| e.to_string())
}

fn load_audio(roots: &ProfileRoots) -> AudioSettings {
    AudioSettingsStore::new_at(&roots.data)
        .and_then(|s| s.get_settings())
        .unwrap_or_default()
}

/// Persist changed keys through T11's `write_one`. Returns `(Some(error_line),
/// reinit)` — a mid-set failure names the key; `reinit` is true when any written
/// key was Reinit-class (§4.3 client-side classification).
fn write_keys(roots: &ProfileRoots, keys: &[(String, String)]) -> (Option<String>, bool) {
    let mut reinit = false;
    for (k, v) in keys {
        match crate::cli::settings::write_one(roots, k, v) {
            Ok(class) => {
                if class == crate::cli::settings::ApplyClass::Reinit {
                    reinit = true;
                }
            }
            Err(e) => {
                // The TUI only displays the message — it doesn't need the
                // Usage/Io exit-code split `settings set` maps to (see
                // `cli::settings::SetError`).
                return (Some(format!("failed to save {k}: {}", e.to_string().trim())), reinit);
            }
        }
    }
    (None, reinit)
}

fn save_network(roots: &ProfileRoots, bind: &str, port: u16, token: Option<&str>) -> Option<String> {
    let path = roots.config.join("qbzd.toml");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    match network_screen::rewrite_toml(&existing, bind, port, token) {
        Ok(text) => match std::fs::write(&path, text) {
            Ok(()) => None,
            Err(e) => Some(format!("failed to write qbzd.toml: {e}")),
        },
        Err(e) => Some(format!("failed to rewrite qbzd.toml: {e}")),
    }
}

/// POST /api/settings/reload and compose the §4.3 result. Returns
/// `(lines, status_body, reachable)`.
async fn do_reload(
    roots: &ProfileRoots,
    is_network: bool,
    reinit: bool,
) -> (Vec<String>, Option<Value>, bool) {
    let client = ApiClient::new(None, roots);
    match client.post("/api/settings/reload", json!({})).await {
        Ok(body) => {
            let lines = if is_network {
                vec!["saved.".to_string(), s::NETWORK_RESTART.to_string()]
            } else {
                let mut line = "saved · daemon reloaded".to_string();
                if reinit {
                    line.push_str(" (output device reinitialized");
                    if let Some(extra) = playing_extra(&body) {
                        line.push_str(&format!(" · {extra}"));
                    }
                    line.push(')');
                }
                vec![line]
            };
            (lines, Some(body), true)
        }
        Err(CliError::Unreachable(_)) => {
            let lines = if is_network {
                vec!["saved.".to_string(), s::APPLIES_ON_START.to_string()]
            } else {
                vec![s::SAVED_DISK_ONLY.to_string()]
            };
            (lines, None, false)
        }
        Err(_) => (vec![s::RELOAD_REFUSED.to_string()], None, true),
    }
}

fn plan_import(roots: &ProfileRoots, path: &str) -> Result<PendingImport, String> {
    let text = std::fs::read_to_string(expand_tilde(path))
        .map_err(|e| format!("cannot read bundle: {e}"))?;
    let bundle = Bundle::parse(&text).map_err(|e| e.to_string())?;
    let (live, backend, devices) = build_live(&bundle);
    let target = ProfilePaths {
        config_root: roots.config.clone(),
        data_root: roots.data.clone(),
    };
    let opts = ImportOptions {
        include_auth: false,
        trust_dsd: false,
        remap: Vec::new(),
        non_tty: false,
    };
    let plan = bundle::plan(&bundle, &target, &opts, &live).map_err(|e| e.to_string())?;
    let has_auth = bundle
        .domains
        .get("auth")
        .and_then(Value::as_object)
        .and_then(|a| a.get("user_auth_token"))
        .and_then(Value::as_str)
        .map(|t| !t.is_empty())
        .unwrap_or(false);
    Ok(PendingImport {
        bundle,
        plan,
        live,
        opts,
        target,
        backend,
        devices,
        device_choice: None,
        has_auth,
        apply_with_auth: false,
    })
}

async fn apply_import(
    roots: &ProfileRoots,
    bundle: Bundle,
    target: ProfilePaths,
    live: LiveSystem,
    opts: ImportOptions,
    choice: Option<bundle::DeviceChoice>,
) -> Msg {
    let plan = match &choice {
        Some(c) => bundle::replan_with_device(&bundle, &target, &opts, &live, c.clone()),
        None => bundle::plan(&bundle, &target, &opts, &live),
    };
    let plan = match plan {
        Ok(p) => p,
        Err(e) => {
            return Msg::ImportApplied {
                lines: vec![e.to_string()],
                status: None,
                reachable: false,
            }
        }
    };

    // Validate the auth token BEFORE any write (§3.6 step 5).
    let mut uid = None;
    if let Some(token) = plan.auth_token.clone() {
        match login::validate_token(&token).await {
            Ok(session) => uid = Some(session.user_id),
            Err(_) => {
                return Msg::ImportApplied {
                    lines: vec!["the Qobuz token in this bundle was rejected".to_string()],
                    status: None,
                    reachable: false,
                }
            }
        }
    }

    if let Err(e) = bundle::apply(&plan, &target, uid) {
        return Msg::ImportApplied {
            lines: vec![format!("import only partially applied: {e}")],
            status: None,
            reachable: false,
        };
    }

    let (mut lines, status, reachable) = do_reload(roots, false, plan.routing_critical_changed).await;
    let mut out = vec![s::b_import_done(
        plan.applied.len(),
        plan.adapted.len(),
        plan.skipped.len(),
    )];
    out.append(&mut lines);
    if uid.is_some() {
        out.push("logged in with the bundled account".to_string());
    }
    Msg::ImportApplied { lines: out, status, reachable }
}

fn export_bundle(roots: &ProfileRoots, dest: &str, include_auth: bool) -> Result<Vec<String>, String> {
    let source = ExportSource::Daemon(ProfilePaths {
        config_root: roots.config.clone(),
        data_root: roots.data.clone(),
    });
    let b = bundle::export(source, &ExportOptions { include_auth }).map_err(|e| e.to_string())?;
    let path = expand_tilde(dest);
    bundle::write_bundle_file(&path, &b).map_err(|e| e.to_string())?;

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(dest)
        .to_string();
    let mut lines = vec![s::b_export_success(&name)];
    if b.contains_secrets() {
        lines.push("this file contains your Qobuz token — 0600, move it privately, delete after import".to_string());
    }
    if desktop_profile_present() {
        for l in s::B_DESKTOP_HINT.lines() {
            lines.push(l.to_string());
        }
    }
    Ok(lines)
}

// ============================ helpers ============================

/// The bundle's target backend + a live enumeration for the re-pick picker
/// (mirrors cli/settings.rs `build_live_system`).
fn build_live(bundle: &Bundle) -> (LiveSystem, AudioBackendType, Vec<AudioDevice>) {
    let backends: Vec<String> = BackendManager::available_backends()
        .into_iter()
        .filter_map(|b| serde_json::to_value(b).ok().and_then(|v| v.as_str().map(str::to_string)))
        .collect();
    let wanted: Option<AudioBackendType> = bundle
        .domains
        .get("audio")
        .and_then(|a| a.get("backend_type"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let backend = wanted.unwrap_or(AudioBackendType::SystemDefault);
    let devices = enumerate_devices(backend).unwrap_or_default();
    let live_devices: Vec<(String, String)> =
        devices.iter().map(|d| (d.id.clone(), d.name.clone())).collect();
    (
        LiveSystem { backends, devices: live_devices },
        backend,
        devices,
    )
}

/// Pure footer mapping (tested below). Three states, each spelled out in text —
/// the tone only reinforces it:
/// - unreachable → dim `daemon: not reachable`;
/// - reachable but not signed in → warn `daemon: running · not signed in`
///   (a deliberate FB2 addition over the base footer: an operator-visible
///   needs-auth cue, owner veto at the smoke);
/// - running + signed in → ok, with the optional `playing …` tail.
fn footer_state(
    reachable: bool,
    logged_in: bool,
    playing: Option<String>,
) -> (String, ratatui::style::Style) {
    if !reachable {
        (format!(" {}", s::FOOTER_UNREACHABLE), theme::dim())
    } else if !logged_in {
        (
            format!(" {} · {}", s::FOOTER_RUNNING, s::FOOTER_NEEDS_AUTH),
            theme::warn(),
        )
    } else {
        let text = match playing {
            Some(e) => format!(" {} · {e}", s::FOOTER_RUNNING),
            None => format!(" {}", s::FOOTER_RUNNING),
        };
        (text, theme::ok())
    }
}

/// A "playing 192000 Hz / 24 bit" tail from a status body (§4.3), if playing.
fn playing_extra(status: &Value) -> Option<String> {
    let state = status.pointer("/playback/state").and_then(Value::as_str).unwrap_or("");
    if state != "playing" {
        return None;
    }
    let sr = status.pointer("/audio/sample_rate").and_then(Value::as_u64)?;
    let bd = status.pointer("/audio/bit_depth").and_then(Value::as_u64)?;
    Some(format!("playing {sr} Hz / {bd} bit"))
}

fn desktop_profile_present() -> bool {
    dirs::data_dir().map(|d| d.join("qbz").exists()).unwrap_or(false)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_screen_no_cred_file() {
        // No credential file → land on Account
        assert_eq!(initial_screen(false, false), Some(Screen::Account));
        assert_eq!(initial_screen(false, true), Some(Screen::Account));
    }

    #[test]
    fn test_initial_screen_cred_file_present() {
        // Credential file present (daemon down) → land on Menu
        assert_eq!(initial_screen(true, false), None);
    }

    #[test]
    fn test_initial_screen_cred_file_and_logged_in() {
        // Credential file present and logged in (daemon up) → land on Menu
        assert_eq!(initial_screen(true, true), None);
    }

    #[test]
    fn footer_state_maps_the_three_daemon_states() {
        // Unreachable → dim, regardless of auth.
        let (text, style) = footer_state(false, true, None);
        assert_eq!(text, format!(" {}", s::FOOTER_UNREACHABLE));
        assert_eq!(style, theme::dim());

        // Reachable but not signed in → warn, names the missing auth.
        let (text, style) = footer_state(true, false, None);
        assert_eq!(text, format!(" {} · {}", s::FOOTER_RUNNING, s::FOOTER_NEEDS_AUTH));
        assert_eq!(style, theme::warn());

        // Running + signed in → ok, with and without the playing tail.
        let (text, style) = footer_state(true, true, None);
        assert_eq!(text, format!(" {}", s::FOOTER_RUNNING));
        assert_eq!(style, theme::ok());
        let (text, _) = footer_state(true, true, Some("playing 96000 Hz / 24 bit".into()));
        assert_eq!(
            text,
            format!(" {} · playing 96000 Hz / 24 bit", s::FOOTER_RUNNING)
        );
    }
}
