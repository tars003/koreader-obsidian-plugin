#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod detect;
mod eject;
mod logutil;
mod sync;

use config::AppConfig;
use eframe::egui;
use logutil::Logger;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use sync::{new_shared_status, SharedStatus, SyncPhase};

/// Shared detection result from the background poller.
#[derive(Clone, Default)]
struct DetectState {
    kindle: Option<detect::DetectedDrive>,
}

struct App {
    cfg: AppConfig,
    // Editable UI fields (mirrored from cfg; written back on Save)
    converter_dir: String,
    python_path: String,
    volume_label: String,
    vault_folder: String,

    logger: Logger,
    detect: Arc<Mutex<DetectState>>,
    status: SharedStatus,
    busy: Arc<AtomicBool>,

    // UI-only
    status_line: String,
    last_log_len: usize,
    auto_scroll: bool,
    show_first_run_hint: bool,
    config_msg: Option<String>,

    // Tray: keep the handle alive for the process lifetime.
    _tray: Option<tray_icon::TrayIcon>,
    tray_show_requested: Arc<AtomicBool>,
    tray_sync_requested: Arc<AtomicBool>,
    tray_quit_requested: Arc<AtomicBool>,
    last_notified_root: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Slightly larger UI for readability
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

        let detect = Arc::new(Mutex::new(DetectState::default()));
        let status = new_shared_status();
        let busy = Arc::new(AtomicBool::new(false));

        // Drive polling runs on the UI thread (~2 Hz) via refresh_detection().

        let tray_show_requested = Arc::new(AtomicBool::new(false));
        let tray_sync_requested = Arc::new(AtomicBool::new(false));
        let tray_quit_requested = Arc::new(AtomicBool::new(false));

        let tray = build_tray(
            Arc::clone(&tray_show_requested),
            Arc::clone(&tray_sync_requested),
            Arc::clone(&tray_quit_requested),
            &logger,
        );

        let show_first_run_hint = !cfg.is_configured();

        let converter_dir = cfg.paths.converter_dir.clone();
        let python_path = cfg.paths.python_path.clone();
        let volume_label = cfg.kindle.volume_label.clone();
        let vault_folder = cfg.kindle.vault_folder.clone();

        Self {
            cfg,
            converter_dir,
            python_path,
            volume_label,
            vault_folder,
            logger,
            detect,
            status,
            busy,
            status_line: "Waiting for Kindle...".into(),
            last_log_len: 0,
            auto_scroll: true,
            show_first_run_hint,
            config_msg: None,
            _tray: tray,
            tray_show_requested,
            tray_sync_requested,
            tray_quit_requested,
            last_notified_root: None,
        }
    }

    fn apply_fields_to_cfg(&mut self) {
        self.cfg.paths.converter_dir = self.converter_dir.trim().to_string();
        self.cfg.paths.python_path = self.python_path.trim().to_string();
        self.cfg.kindle.volume_label = self.volume_label.trim().to_string();
        self.cfg.kindle.vault_folder = self.vault_folder.trim().to_string();
    }

    fn save_config(&mut self) {
        self.apply_fields_to_cfg();
        match self.cfg.save() {
            Ok(()) => {
                self.config_msg = Some("Config saved.".into());
                self.logger.log(format!(
                    "Config saved → {}",
                    AppConfig::config_path().display()
                ));
                self.show_first_run_hint = !self.cfg.is_configured();
            }
            Err(e) => {
                self.config_msg = Some(format!("Save failed: {e}"));
                self.logger.log(format!("Config save error: {e}"));
            }
        }
    }

    fn refresh_detection(&mut self) {
        let label = self.volume_label.trim();
        let found = detect::find_kindle(label);
        if let Ok(mut g) = self.detect.lock() {
            g.kindle = found.clone();
        }

        let busy = self.busy.load(Ordering::SeqCst);
        let sync_status = self
            .status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        if busy {
            self.status_line = match sync_status.phase {
                SyncPhase::Converting => format!("Converting… {}", sync_status.detail),
                SyncPhase::Copying => format!("Copying… {}", sync_status.detail),
                SyncPhase::Ejecting => "Ejecting Kindle…".into(),
                SyncPhase::Done => sync_status.detail.clone(),
                SyncPhase::Error => format!("Error: {}", sync_status.detail),
                SyncPhase::Idle => "Working…".into(),
            };
        } else if let Some(d) = &found {
            self.status_line = format!("Kindle detected at {}", d.root);
            // Notify once per plug-in
            if self.last_notified_root.as_deref() != Some(d.root.as_str()) {
                self.last_notified_root = Some(d.root.clone());
                self.logger.log(format!("Kindle detected at {}", d.root));
                show_notification(
                    "Kindle Vault Sync",
                    &format!("Kindle detected at {}. Click Sync Now.", d.root),
                );
            }
        } else {
            if self.last_notified_root.is_some() {
                self.logger.log("Kindle disconnected");
            }
            self.last_notified_root = None;
            // Keep done/error message briefly if set
            if matches!(sync_status.phase, SyncPhase::Done | SyncPhase::Error)
                && !sync_status.detail.is_empty()
            {
                self.status_line = sync_status.detail.clone();
            } else {
                self.status_line = "Waiting for Kindle...".into();
            }
        }
    }

    fn start_sync(&mut self) {
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
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::from_rgb(32, 32, 36).to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Poll detection ~4x/sec from UI thread (cheap Win32 calls)
        if ctx.input(|i| i.time) as u64 % 1 == 0 {
            // always refresh; egui runs this every frame, throttle ourselves
        }
        // Throttle via memory of last second — simpler: just call every frame is fine
        // for 26 drive letter probes; still throttle to ~2 Hz via request_repaint.
        static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let now_ms = (ctx.input(|i| i.time) * 1000.0) as u64;
        if now_ms.saturating_sub(LAST.load(Ordering::Relaxed)) > 500 {
            LAST.store(now_ms, Ordering::Relaxed);
            self.refresh_detection();
        }
        ctx.request_repaint_after(Duration::from_millis(500));

        // Tray menu actions
        if self.tray_quit_requested.swap(false, Ordering::SeqCst) {
            self.logger.log("Quit from tray");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.tray_show_requested.swap(false, Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        }
        if self.tray_sync_requested.swap(false, Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.start_sync();
        }

        // Close-to-tray: intercept close and hide instead
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.logger.log("Minimized to tray");
        }

        let busy = self.busy.load(Ordering::SeqCst);
        let kindle_present = self
            .detect
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
                });

            ui.add_space(10.0);

            // Status
            let status_color = if busy {
                egui::Color32::from_rgb(120, 180, 255)
            } else if kindle_present.is_some() {
                egui::Color32::from_rgb(120, 220, 140)
            } else {
                egui::Color32::from_rgb(180, 180, 180)
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Status:").strong());
                ui.label(egui::RichText::new(&self.status_line).color(status_color));
            });

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                let sync_enabled = !busy && kindle_present.is_some() && {
                    self.apply_fields_to_cfg();
                    self.cfg.is_configured()
                };
                ui.add_enabled_ui(sync_enabled, |ui| {
                    let btn = egui::Button::new(
                        egui::RichText::new("  Sync Now  ")
                            .size(16.0)
                            .strong(),
                    )
                    .min_size(egui::vec2(120.0, 32.0));
                    if ui.add(btn).clicked() {
                        self.start_sync();
                    }
                });

                if ui
                    .add(
                        egui::Button::new("Save Config").min_size(egui::vec2(110.0, 32.0)),
                    )
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
                    // Selectable monospace lines — drag to select, Ctrl+C to copy.
                    for line in &lines {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(line).monospace().size(13.0),
                            )
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

        // Keep unused warning quiet
        let _ = frame;
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.logger.log("App exited");
    }
}

fn build_tray(
    show: Arc<AtomicBool>,
    sync: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    logger: &Logger,
) -> Option<tray_icon::TrayIcon> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::TrayIconBuilder;

    // 16x16 simple blue-ish icon (RGBA)
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

    // Menu event listener thread
    thread::spawn(move || {
        let rx = MenuEvent::receiver();
        while let Ok(ev) = rx.recv() {
            if ev.id == show_id {
                show.store(true, Ordering::SeqCst);
            } else if ev.id == sync_id {
                sync.store(true, Ordering::SeqCst);
            } else if ev.id == quit_id {
                quit.store(true, Ordering::SeqCst);
            }
        }
    });

    Some(tray)
}

fn make_icon() -> Result<tray_icon::Icon, String> {
    // Solid teal 32x32 PNG-less raw RGBA
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            // Simple rounded-square look
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

fn show_notification(title: &str, body: &str) {
    // Best-effort balloon via PowerShell toast (works without extra crates)
    let title = title.replace('\'', "''");
    let body = body.replace('\'', "''");
    let script = format!(
        "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
         $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
         $t.GetElementsByTagName('text').Item(0).AppendChild($t.CreateTextNode('{title}')) > $null; \
         $t.GetElementsByTagName('text').Item(1).AppendChild($t.CreateTextNode('{body}')) > $null; \
         [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('KindleVaultSync').Show([Windows.UI.Notifications.ToastNotification]::new($t))"
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 520.0])
            .with_min_inner_size([480.0, 400.0])
            .with_title("Kindle Vault Sync"),
        ..Default::default()
    };

    eframe::run_native(
        "Kindle Vault Sync",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
