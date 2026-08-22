#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod detect;
mod eject;
mod logutil;
mod notify;
mod startup;
mod sync;
mod winutil;

use config::AppConfig;
use eframe::egui;
use logutil::Logger;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sync::{new_shared_status, SharedStatus, SyncPhase};

/// Seconds to wait after plug-in before auto-starting sync (visible countdown).
const AUTO_SYNC_DELAY_SECS: u64 = 3;

/// Shared state written by the background drive watcher (always-on thread).
/// Must NOT depend on the egui update loop — that freezes while the window is hidden.
struct WatchState {
    kindle: Option<detect::DetectedDrive>,
    /// Live label the watcher matches (updated from UI / Save Config).
    volume_label: String,
    poll_interval_secs: u64,
    /// Set by watcher on rising edge; UI takes() it to arm countdown.
    plug_event: Option<String>,
    /// Set by watcher on falling edge; UI takes it to cancel countdown.
    unplug_event: bool,
}

struct App {
    cfg: AppConfig,
    converter_dir: String,
    python_path: String,
    volume_label: String,
    vault_folder: String,
    run_at_startup: bool,

    logger: Logger,
    watch: Arc<Mutex<WatchState>>,
    status: SharedStatus,
    busy: Arc<AtomicBool>,

    status_line: String,
    last_log_len: usize,
    auto_scroll: bool,
    show_first_run_hint: bool,
    config_msg: Option<String>,

    /// Kept alive for process lifetime.
    _tray: Option<tray_icon::TrayIcon>,
    tray_sync_requested: Arc<AtomicBool>,
    /// When true, close really exits (tray Quit / Exit button).
    allow_exit: Arc<AtomicBool>,
    /// When set, auto-sync fires at this instant (countdown shown in UI).
    auto_sync_at: Option<Instant>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(20.0, egui::FontFamily::Proportional),
        );
        cc.egui_ctx.set_style(style);

        let cfg = AppConfig::load();
        let logger = Logger::new(AppConfig::log_path());
        logger.log("App started");

        let watch = Arc::new(Mutex::new(WatchState {
            kindle: None,
            volume_label: cfg.kindle.volume_label.clone(),
            // Never slower than 2s — hidden/tray mode must still detect quickly.
            poll_interval_secs: cfg.behavior.poll_interval_secs.clamp(2, 30),
            plug_event: None,
            unplug_event: false,
        }));

        let status = new_shared_status();
        let busy = Arc::new(AtomicBool::new(false));
        let tray_sync_requested = Arc::new(AtomicBool::new(false));
        let allow_exit = Arc::new(AtomicBool::new(false));

        // Background watcher — independent of window visibility / egui update.
        spawn_drive_watcher(
            Arc::clone(&watch),
            logger.clone(),
            Arc::clone(&busy),
            cc.egui_ctx.clone(),
        );

        // Light heartbeat so countdown UI stays smooth when visible.
        {
            let ctx = cc.egui_ctx.clone();
            thread::spawn(move || loop {
                thread::sleep(Duration::from_millis(200));
                ctx.request_repaint();
            });
        }

        let tray = build_tray(
            cc.egui_ctx.clone(),
            Arc::clone(&tray_sync_requested),
            Arc::clone(&allow_exit),
            logger.clone(),
        );

        let show_first_run_hint = !cfg.is_configured();
        let converter_dir = cfg.paths.converter_dir.clone();
        let python_path = cfg.paths.python_path.clone();
        let volume_label = cfg.kindle.volume_label.clone();
        let vault_folder = cfg.kindle.vault_folder.clone();
        let run_at_startup = startup::is_run_at_startup() || cfg.behavior.run_at_startup;

        Self {
            cfg,
            converter_dir,
            python_path,
            volume_label,
            vault_folder,
            run_at_startup,
            logger,
            watch,
            status,
            busy,
            status_line: "Waiting for Kindle...".into(),
            last_log_len: 0,
            auto_scroll: true,
            show_first_run_hint,
            config_msg: None,
            _tray: tray,
            tray_sync_requested,
            allow_exit,
            auto_sync_at: None,
        }
    }

    fn bring_window_to_front(&self, ctx: &egui::Context) {
        let _ = winutil::show_main_window();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn cancel_auto_sync(&mut self, reason: &str) {
        if self.auto_sync_at.take().is_some() {
            self.logger.log(format!("Auto-sync cancelled ({reason})"));
        }
    }

    fn apply_fields_to_cfg(&mut self) {
        self.cfg.paths.converter_dir = self.converter_dir.trim().to_string();
        self.cfg.paths.python_path = self.python_path.trim().to_string();
        self.cfg.kindle.volume_label = self.volume_label.trim().to_string();
        self.cfg.kindle.vault_folder = self.vault_folder.trim().to_string();
        self.cfg.behavior.run_at_startup = self.run_at_startup;
        // Keep watcher in sync with UI label without waiting for Save.
        if let Ok(mut g) = self.watch.lock() {
            g.volume_label = self.cfg.kindle.volume_label.clone();
            g.poll_interval_secs = self.cfg.behavior.poll_interval_secs.clamp(2, 30);
        }
    }

    fn save_config(&mut self) {
        self.apply_fields_to_cfg();
        match self.cfg.save() {
            Ok(()) => {
                match startup::set_run_at_startup(self.run_at_startup) {
                    Ok(()) => {
                        let msg = if self.run_at_startup {
                            "Config saved. Run at startup: ON"
                        } else {
                            "Config saved. Run at startup: OFF"
                        };
                        self.config_msg = Some(msg.into());
                        self.logger.log(format!(
                            "Config saved → {} (run_at_startup={})",
                            AppConfig::config_path().display(),
                            self.run_at_startup
                        ));
                    }
                    Err(e) => {
                        self.config_msg =
                            Some(format!("Config saved, but startup toggle failed: {e}"));
                        self.logger.log(format!("Startup toggle error: {e}"));
                    }
                }
                self.show_first_run_hint = !self.cfg.is_configured();
            }
            Err(e) => {
                self.config_msg = Some(format!("Save failed: {e}"));
                self.logger.log(format!("Config save error: {e}"));
            }
        }
    }

    /// Pull plug/unplug events from the background watcher + refresh status text.
    fn poll_watch_events(&mut self, ctx: &egui::Context) {
        let (plug, unplug, kindle) = if let Ok(mut g) = self.watch.lock() {
            let plug = g.plug_event.take();
            let unplug = std::mem::take(&mut g.unplug_event);
            (plug, unplug, g.kindle.clone())
        } else {
            (None, false, None)
        };

        if unplug {
            self.cancel_auto_sync("Kindle unplugged");
        }

        // Rising-edge only — never fires just because the user clicked Show.
        if let Some(root) = plug {
            self.on_kindle_plugged_in(ctx, &root);
        }

        self.refresh_status_line(kindle.as_ref());
    }

    fn refresh_status_line(&mut self, kindle: Option<&detect::DetectedDrive>) {
        let busy = self.busy.load(Ordering::SeqCst);
        let sync_status = self
            .status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        if !busy {
            if let Some(at) = self.auto_sync_at {
                let left = at.saturating_duration_since(Instant::now());
                let secs = left.as_secs().saturating_add(if left.subsec_nanos() > 0 {
                    1
                } else {
                    0
                });
                if let Some(d) = kindle {
                    self.status_line = format!(
                        "Kindle at {} — auto-sync in {}s…",
                        d.root,
                        secs.max(1)
                    );
                } else {
                    self.status_line = format!("Auto-sync in {}s…", secs.max(1));
                }
                return;
            }
        }

        if busy {
            self.status_line = match sync_status.phase {
                SyncPhase::Converting => format!("Converting… {}", sync_status.detail),
                SyncPhase::Copying => format!("Copying… {}", sync_status.detail),
                SyncPhase::Ejecting => "Ejecting Kindle…".into(),
                SyncPhase::Done => sync_status.detail.clone(),
                SyncPhase::Error => format!("Error: {}", sync_status.detail),
                SyncPhase::Idle => "Working…".into(),
            };
        } else if let Some(d) = kindle {
            self.status_line = format!("Kindle detected at {}", d.root);
        } else if matches!(sync_status.phase, SyncPhase::Done | SyncPhase::Error)
            && !sync_status.detail.is_empty()
        {
            self.status_line = sync_status.detail.clone();
        } else {
            self.status_line = "Waiting for Kindle...".into();
        }
    }

    fn on_kindle_plugged_in(&mut self, ctx: &egui::Context, root: &str) {
        // Window + balloon already fired from the watcher thread.
        // Here we only arm the visible countdown / handle config gaps.
        self.bring_window_to_front(ctx);
        self.apply_fields_to_cfg();

        if self.busy.load(Ordering::SeqCst) {
            self.logger.log("Already syncing — skip auto-sync countdown");
            return;
        }
        if !self.cfg.is_configured() {
            self.config_msg = Some("Kindle detected — set Converter dir and Save Config.".into());
            self.logger.log("Not configured — auto-sync not armed");
            return;
        }

        self.auto_sync_at = Some(Instant::now() + Duration::from_secs(AUTO_SYNC_DELAY_SECS));
        self.logger.log(format!(
            "Auto-sync armed ({AUTO_SYNC_DELAY_SECS}s countdown)"
        ));
        self.status_line =
            format!("Kindle at {root} — auto-sync in {AUTO_SYNC_DELAY_SECS}s…");
        ctx.request_repaint();
    }

    fn tick_auto_sync(&mut self, ctx: &egui::Context) {
        let Some(at) = self.auto_sync_at else {
            return;
        };
        if self.busy.load(Ordering::SeqCst) {
            self.cancel_auto_sync("sync already running");
            return;
        }
        // Keep UI smooth during countdown.
        ctx.request_repaint_after(Duration::from_millis(100));

        if Instant::now() < at {
            // Refresh status text with remaining seconds.
            let left = at.saturating_duration_since(Instant::now());
            let secs = left.as_secs().saturating_add(if left.subsec_nanos() > 0 { 1 } else { 0 });
            let root = self
                .watch
                .lock()
                .ok()
                .and_then(|g| g.kindle.as_ref().map(|d| d.root.clone()))
                .unwrap_or_else(|| "?".into());
            self.status_line = format!("Kindle at {root} — auto-sync in {}s…", secs.max(1));
            return;
        }

        self.auto_sync_at = None;
        self.logger.log("Auto-sync countdown finished — starting sync");
        self.start_sync();
    }

    fn start_sync(&mut self) {
        self.auto_sync_at = None; // manual or auto — clear countdown
        self.apply_fields_to_cfg();
        if !self.cfg.is_configured() {
            self.logger
                .log("Cannot sync: set Converter dir to the folder with md2kindle.toml");
            self.config_msg = Some("Set a valid Converter dir first.".into());
            return;
        }
        let kindle = detect::find_kindle(&self.cfg.kindle.volume_label);
        let Some(drive) = kindle else {
            self.logger.log("Cannot sync: Kindle not detected");
            return;
        };
        self.logger
            .log(format!("Sync requested for {}", drive.root));
        sync::spawn_sync(
            self.cfg.clone(),
            drive.root,
            self.logger.clone(),
            Arc::clone(&self.status),
            Arc::clone(&self.busy),
        );
    }

    fn request_exit(&self, ctx: &egui::Context) {
        self.logger.log("Exit requested");
        self.allow_exit.store(true, Ordering::SeqCst);
        // Make sure a close event can be delivered.
        let _ = winutil::show_main_window();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        // Hard fallback if the viewport close path is stuck.
        let allow = Arc::clone(&self.allow_exit);
        let logger = self.logger.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(800));
            if allow.load(Ordering::SeqCst) {
                logger.log("Force exit");
                std::process::exit(0);
            }
        });
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::from_rgb(32, 32, 36).to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Consume plug/unplug events from the always-on watcher thread.
        // (Do NOT poll drives here — that froze while the window was hidden.)
        self.poll_watch_events(ctx);
        self.tick_auto_sync(ctx);

        // Tray "Sync Now" — Show is handled in the tray thread via Win32.
        // Opening the window alone must NOT arm auto-sync.
        if self.tray_sync_requested.swap(false, Ordering::SeqCst) {
            self.logger.log("Sync from tray");
            self.bring_window_to_front(ctx);
            self.start_sync();
        }

        // Close-to-tray (unless real exit)
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.allow_exit.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            // Prefer Win32 hide — more reliable to reverse than egui Visible(false) alone.
            if !winutil::hide_main_window() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            self.logger.log("Minimized to tray");
        }

        let busy = self.busy.load(Ordering::SeqCst);
        let kindle_present = self
            .watch
            .lock()
            .ok()
            .and_then(|g| g.kindle.clone());

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.heading("Kindle Vault Sync");
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);

            if self.show_first_run_hint {
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(60, 50, 20))
                    .corner_radius(4.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(
                                "First run: set Converter dir to the folder that contains md2kindle.toml, then Save Config.",
                            )
                            .color(egui::Color32::from_rgb(255, 220, 120)),
                        );
                    });
                ui.add_space(8.0);
            }

            egui::Grid::new("cfg_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .min_col_width(120.0)
                .show(ui, |ui| {
                    ui.label("Converter dir:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.converter_dir)
                            .desired_width(420.0)
                            .hint_text(r"C:\path\to\converter"),
                    );
                    ui.end_row();

                    ui.label("Python path:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.python_path)
                            .desired_width(420.0)
                            .hint_text("python  (or full path to python.exe)"),
                    );
                    ui.end_row();

                    ui.label("Kindle label:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.volume_label).desired_width(420.0),
                    );
                    ui.end_row();

                    ui.label("Vault folder:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.vault_folder).desired_width(420.0),
                    );
                    ui.end_row();

                    ui.label("Startup:");
                    ui.checkbox(&mut self.run_at_startup, "Run at Windows startup");
                    ui.end_row();
                });

            ui.add_space(10.0);

            let countdown_active = self.auto_sync_at.is_some() && !busy;
            let status_color = if countdown_active {
                egui::Color32::from_rgb(255, 200, 80)
            } else if busy {
                egui::Color32::from_rgb(120, 180, 255)
            } else if kindle_present.is_some() {
                egui::Color32::from_rgb(120, 220, 140)
            } else {
                egui::Color32::from_rgb(180, 180, 180)
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Status:").strong());
                ui.label(
                    egui::RichText::new(&self.status_line)
                        .color(status_color)
                        .size(if countdown_active { 18.0 } else { 15.0 })
                        .strong(),
                );
            });

            if countdown_active {
                ui.add_space(6.0);
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(50, 40, 10))
                    .corner_radius(4.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Auto-sync starting…")
                                    .color(egui::Color32::from_rgb(255, 220, 120))
                                    .size(16.0)
                                    .strong(),
                            );
                            ui.add_space(12.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Cancel").strong(),
                                    )
                                    .min_size(egui::vec2(90.0, 28.0)),
                                )
                                .clicked()
                            {
                                self.cancel_auto_sync("user cancelled");
                                if let Some(d) = &kindle_present {
                                    self.status_line =
                                        format!("Kindle detected at {} — auto-sync cancelled", d.root);
                                }
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Sync now").strong(),
                                    )
                                    .min_size(egui::vec2(90.0, 28.0)),
                                )
                                .clicked()
                            {
                                self.start_sync();
                            }
                        });
                    });
            }

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                let sync_enabled = !busy && kindle_present.is_some() && {
                    self.apply_fields_to_cfg();
                    self.cfg.is_configured()
                };
                ui.add_enabled_ui(sync_enabled, |ui| {
                    let btn = egui::Button::new(
                        egui::RichText::new("  Sync Now  ").size(16.0).strong(),
                    )
                    .min_size(egui::vec2(120.0, 32.0));
                    if ui.add(btn).clicked() {
                        self.start_sync();
                    }
                });

                if ui
                    .add(egui::Button::new("Save Config").min_size(egui::vec2(110.0, 32.0)))
                    .clicked()
                {
                    self.save_config();
                }

                if ui
                    .add(egui::Button::new("Open Log").min_size(egui::vec2(90.0, 32.0)))
                    .clicked()
                {
                    let p = AppConfig::log_path();
                    let _ = open::that(p);
                }

                if ui
                    .add(egui::Button::new("Exit").min_size(egui::vec2(70.0, 32.0)))
                    .clicked()
                {
                    self.request_exit(ctx);
                }

                if busy {
                    ui.spinner();
                    ui.label("Working…");
                }
            });

            if let Some(msg) = &self.config_msg {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(msg).color(egui::Color32::LIGHT_BLUE));
            }

            ui.add_space(12.0);
            ui.separator();

            let lines = self.logger.snapshot();
            let log_text = lines.join("\n");

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Log").strong());
                ui.checkbox(&mut self.auto_scroll, "auto-scroll");
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(log_text.clone());
                    self.config_msg = Some("Log copied to clipboard.".into());
                }
                if ui.button("Clear").clicked() {
                    self.logger.clear_ui();
                }
            });

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(self.auto_scroll)
                .show(ui, |ui| {
                    ui.set_min_height(220.0);
                    ui.set_width(ui.available_width());
                    for line in &lines {
                        ui.add(
                            egui::Label::new(egui::RichText::new(line).monospace().size(13.0))
                                .selectable(true)
                                .wrap(),
                        );
                    }
                    if lines.is_empty() {
                        ui.weak("(empty)");
                    }
                    if self.auto_scroll && lines.len() != self.last_log_len {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    }
                });
            self.last_log_len = lines.len();
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.logger.log("App exited");
    }
}

/// Always-on USB watcher. Runs even when the main window is hidden.
fn spawn_drive_watcher(
    watch: Arc<Mutex<WatchState>>,
    logger: Logger,
    busy: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        // Seed presence so an already-plugged Kindle at launch does NOT auto-sync.
        let mut was_present = {
            let label = watch
                .lock()
                .map(|g| g.volume_label.clone())
                .unwrap_or_else(|_| "Kindle".into());
            let found = detect::find_kindle(&label);
            if let Ok(mut g) = watch.lock() {
                g.kindle = found.clone();
            }
            let present = found.is_some();
            if present {
                logger.log(format!(
                    "Watcher: Kindle already present at {} — auto-sync waits for next plug-in",
                    found.as_ref().map(|d| d.root.as_str()).unwrap_or("?")
                ));
            } else {
                logger.log("Watcher: started (no Kindle yet)");
            }
            present
        };

        loop {
            let (label, interval) = match watch.lock() {
                Ok(g) => (
                    g.volume_label.clone(),
                    g.poll_interval_secs.clamp(2, 30),
                ),
                Err(_) => ("Kindle".into(), 2),
            };

            let found = detect::find_kindle(&label);
            let present = found.is_some();

            if present && !was_present {
                // Rising edge — real plug-in while app may be in tray.
                let root = found
                    .as_ref()
                    .map(|d| d.root.clone())
                    .unwrap_or_else(|| "?".into());
                logger.log(format!("Watcher: Kindle plugged in at {root}"));

                // Immediate, no UI loop required:
                let shown = winutil::show_main_window();
                logger.log(format!("Watcher: show_main_window → {shown}"));
                if !busy.load(Ordering::SeqCst) {
                    notify::show(
                        "Kindle Vault Sync",
                        &format!(
                            "Kindle detected at {root}. Auto-sync in {AUTO_SYNC_DELAY_SECS}s."
                        ),
                    );
                }

                if let Ok(mut g) = watch.lock() {
                    g.kindle = found.clone();
                    g.plug_event = Some(root);
                    g.unplug_event = false;
                }
                ctx.request_repaint();
            } else if !present && was_present {
                logger.log("Watcher: Kindle disconnected");
                if let Ok(mut g) = watch.lock() {
                    g.kindle = None;
                    g.unplug_event = true;
                    g.plug_event = None;
                }
                ctx.request_repaint();
            } else if let Ok(mut g) = watch.lock() {
                // Steady state — keep snapshot fresh for the UI.
                g.kindle = found.clone();
            }

            was_present = present;
            thread::sleep(Duration::from_secs(interval));
        }
    });
}

fn build_tray(
    ctx: egui::Context,
    sync: Arc<AtomicBool>,
    allow_exit: Arc<AtomicBool>,
    logger: Logger,
) -> Option<tray_icon::TrayIcon> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let icon = match make_icon() {
        Ok(i) => i,
        Err(e) => {
            logger.log(format!("Tray icon skipped: {e}"));
            return None;
        }
    };

    let show_i = MenuItem::new("Show", true, None);
    let sync_i = MenuItem::new("Sync Now", true, None);
    let quit_i = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    if let Err(e) = menu.append_items(&[
        &show_i,
        &sync_i,
        &PredefinedMenuItem::separator(),
        &quit_i,
    ]) {
        logger.log(format!("Tray menu error: {e}"));
        return None;
    }

    let show_id = show_i.id().clone();
    let sync_id = sync_i.id().clone();
    let quit_id = quit_i.id().clone();

    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Kindle Vault Sync")
        .with_icon(icon)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            logger.log(format!("Tray build failed: {e}"));
            return None;
        }
    };

    // Menu events: act immediately (Win32 show / process exit), don't wait for egui.
    let logger_menu = logger.clone();
    let sync_menu = Arc::clone(&sync);
    let allow_exit_menu = Arc::clone(&allow_exit);
    let ctx_menu = ctx.clone();
    thread::spawn(move || {
        let rx = MenuEvent::receiver();
        while let Ok(ev) = rx.recv() {
            if ev.id == show_id {
                logger_menu.log("Tray menu: Show");
                let ok = winutil::show_main_window();
                logger_menu.log(format!("Win32 show_main_window → {ok}"));
                ctx_menu.request_repaint();
            } else if ev.id == sync_id {
                logger_menu.log("Tray menu: Sync Now");
                let ok = winutil::show_main_window();
                logger_menu.log(format!("Win32 show_main_window → {ok}"));
                sync_menu.store(true, Ordering::SeqCst);
                ctx_menu.request_repaint();
            } else if ev.id == quit_id {
                logger_menu.log("Tray menu: Quit");
                allow_exit_menu.store(true, Ordering::SeqCst);
                let _ = winutil::show_main_window();
                ctx_menu.request_repaint();
                // Don't depend on egui close path — exit hard after a brief flush window.
                thread::sleep(Duration::from_millis(150));
                logger_menu.log("Exiting process");
                std::process::exit(0);
            }
        }
    });

    // Left-click tray icon → Show via Win32
    let logger_click = logger.clone();
    let ctx_click = ctx.clone();
    thread::spawn(move || {
        let rx = TrayIconEvent::receiver();
        while let Ok(ev) = rx.recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                logger_click.log("Tray icon left-click → Show");
                let ok = winutil::show_main_window();
                logger_click.log(format!("Win32 show_main_window → {ok}"));
                ctx_click.request_repaint();
            }
        }
    });

    logger.log("Tray icon ready");
    Some(tray)
}

fn make_icon() -> Result<tray_icon::Icon, String> {
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let edge = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
            if edge {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                rgba.extend_from_slice(&[32, 160, 140, 255]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).map_err(|e| e.to_string())
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 520.0])
            .with_min_inner_size([480.0, 400.0])
            .with_title(winutil::WINDOW_TITLE),
        ..Default::default()
    };

    eframe::run_native(
        winutil::WINDOW_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
