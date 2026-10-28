// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// Only the desktop downloads from GitHub (feature `update-check`: the AppImage and the .exe)
// look for updates: they download the new file beside themselves and restart into it.
// Everywhere else the store or the packager updates the app.

use std::path::{Path, PathBuf};

use egui::RichText;
use rubiyomi_core::theme::Scheme;
use rubiyomi_core::update::{self, Progress, Release, UpdateSettings};

use crate::task::{self, Task};
use crate::ui::m3::{self, c, font, text};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "update-check"), allow(dead_code))]
enum Build {
    // The AppImage file, not the binary mounted from it.
    AppImage(PathBuf),
    Exe(PathBuf),
}

impl Build {
    #[cfg(feature = "update-check")]
    fn detect() -> Option<Build> {
        if cfg!(target_os = "android") {
            return None;
        }
        if cfg!(windows) {
            return std::env::current_exe().ok().map(Build::Exe);
        }
        if Path::new("/.flatpak-info").exists() {
            return None;
        }
        std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file()).map(Build::AppImage)
    }

    #[cfg(not(feature = "update-check"))]
    fn detect() -> Option<Build> {
        None
    }

    fn suffix(&self) -> &'static str {
        match self {
            Build::AppImage(_) => "-x86_64.AppImage",
            Build::Exe(_) => "-x86_64.exe",
        }
    }

    // Beside the running file, so the new one takes its place; in Downloads if that folder is read-only.
    fn folder(&self) -> PathBuf {
        let (Build::AppImage(file) | Build::Exe(file)) = self;
        let beside = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let probe = beside.join(".rubiyomi-write-test");
        if std::fs::write(&probe, b"").is_ok() {
            let _ = std::fs::remove_file(probe);
            return beside;
        }
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        home.map(|h| h.join("Downloads")).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir)
    }

    // Starts the downloaded file, telling it which one it replaces; the caller then closes this one.
    fn restart(&self, new: &Path) -> Result<(), String> {
        let (Build::AppImage(old) | Build::Exe(old)) = self;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        let mut command = std::process::Command::new(new);
        if old.parent() == new.parent() && old != new {
            command.arg(REPLACES).arg(old);
        }
        command.spawn().map(drop).map_err(|e| format!("starting {}: {e}", new.display()))
    }
}

pub const REPLACES: &str = "--replaces";

// In the new version, started with `--replaces <old file>`: removes the old file once its app
// has closed, and takes its name if it had no version in it, so shortcuts keep working.
pub fn finish_replacing(old: PathBuf) {
    let Some(new) = std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok()) else { return };
    let ours = old.extension().is_some_and(|e| e.eq_ignore_ascii_case("AppImage") || e.eq_ignore_ascii_case("exe"));
    if !ours || old.parent() != new.parent() || old == new {
        return;
    }
    std::thread::spawn(move || {
        // Windows keeps a running .exe from being deleted, so this waits for the old app to close.
        for _ in 0..60 {
            if std::fs::remove_file(&old).is_ok() || !old.exists() {
                if !versioned(&old.file_name().unwrap_or_default().to_string_lossy())
                    && let Err(e) = std::fs::rename(&new, &old)
                {
                    log::warn!("renaming {} to {}: {e}", new.display(), old.display());
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        log::warn!("couldn't remove the old version, {}", old.display());
    });
}

// "Rubiyomi-0.5.1-x86_64.AppImage" names a version; "Rubiyomi.exe" or "rubiyomi-x86_64.AppImage" don't.
fn versioned(name: &str) -> bool {
    let b = name.as_bytes();
    b.windows(3).any(|w| w[0].is_ascii_digit() && w[1] == b'.' && w[2].is_ascii_digit())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release),
    // The new version is starting.
    Done,
    // With the release when it was its download that failed, for the pop-up to offer again.
    Failed(String, Option<Release>),
}

pub struct Updates {
    build: Option<Build>,
    data_dir: PathBuf,
    pub settings: UpdateSettings,
    pub status: Status,
    // The pop-up was answered with Later, or not asked for.
    quiet: bool,
    progress: Progress,
    checking: Option<Task<Result<Release, String>>>,
    downloading: Option<Task<Result<PathBuf, String>>>,
}

impl Updates {
    pub fn new(ctx: &egui::Context, data_dir: PathBuf) -> Self {
        let mut me = Updates {
            build: Build::detect(),
            settings: UpdateSettings::load(&data_dir),
            data_dir,
            status: Status::Idle,
            quiet: false,
            progress: Progress::default(),
            checking: None,
            downloading: None,
        };
        if me.available() && me.settings.check_on_start {
            me.check(ctx);
        }
        me
    }

    // Whether this build looks for updates at all, so their settings are worth showing.
    pub fn available(&self) -> bool {
        self.build.is_some()
    }

    pub fn set_check_on_start(&mut self, on: bool) {
        self.settings.check_on_start = on;
        if let Err(e) = self.settings.save(&self.data_dir) {
            log::warn!("saving the update settings: {e}");
        }
    }

    pub fn check(&mut self, ctx: &egui::Context) {
        if !self.available() || self.checking.is_some() || self.downloading.is_some() {
            return;
        }
        self.status = Status::Checking;
        self.quiet = false;
        // A pretend release from tool/update-feed.py.
        let feed = std::env::var("RUBIYOMI_UPDATE_URL").ok();
        self.checking = Some(Task::spawn(ctx, move || update::latest(feed.as_deref())));
    }

    fn download(&mut self, ctx: &egui::Context, release: Release) {
        let Some(build) = self.build.clone() else { return };
        let folder = build.folder();
        let progress = Progress::default();
        self.progress = progress.clone();
        self.status = Status::Downloading(release.clone());
        self.downloading = Some(Task::spawn(ctx, move || {
            let name = release.asset(build.suffix()).map(|a| a.name.clone()).ok_or("this release has no file for this computer")?;
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let to = folder.join(name);
            update::download(&release, build.suffix(), &to, &progress)?;
            Ok(to)
        }));
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(answer) = task::take(&mut self.checking) {
            self.status = match answer {
                Ok(release) if update::newer(release.version(), env!("CARGO_PKG_VERSION")) => Status::Available(release),
                Ok(_) => Status::UpToDate,
                Err(e) => {
                    log::info!("update check: {e}");
                    Status::Failed(e, None)
                }
            };
        }
        if let Some(done) = task::take(&mut self.downloading) {
            let release = self.offered().cloned();
            let started = done.and_then(|new| self.build.as_ref().map_or(Ok(()), |b| b.restart(&new)));
            match started {
                Ok(()) => {
                    self.status = Status::Done;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(e) => {
                    log::warn!("update: {e}");
                    self.status = Status::Failed(e, release);
                }
            }
        }
        if matches!(self.status, Status::Downloading(_)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    // The pop-up offering a newer release, and following its download.
    pub fn popup(&mut self, ctx: &egui::Context, s: &Scheme) {
        if self.quiet {
            return;
        }
        let Some(release) = self.offered().cloned() else { return };
        let mut later = false;
        let mut go = false;
        egui::Modal::new(egui::Id::new("update-available")).show(ctx, |ui| {
            m3::show_next_frame(ui);
            ui.set_width(400.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.label(
                RichText::new(format!("Rubiyomi {} is available", release.version()))
                    .font(font(text::HEADLINE_SMALL))
                    .color(c(s.on_surface)),
            );
            ui.add_space(12.0);
            let notes = release.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("No release notes.");
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() * 0.5).min(320.0)).show(ui, |ui| {
                ui.label(RichText::new(notes).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant)));
            });
            ui.add_space(16.0);
            match &self.status {
                Status::Downloading(_) => {
                    let fraction = self.progress.fraction();
                    ui.add(egui::ProgressBar::new(fraction.unwrap_or(0.0)).animate(fraction.is_none()));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Downloading and checking…").font(font(text::BODY_SMALL)).color(c(s.on_surface_variant)));
                }
                status => {
                    if let Status::Failed(e, _) = status {
                        ui.label(RichText::new(format!("Couldn't update: {e}")).font(font(text::BODY_MEDIUM)).color(c(s.error)));
                        ui.add_space(8.0);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        go = m3::filled_button(ui, s, "Update").clicked();
                        ui.add_space(8.0);
                        later = m3::text_button(ui, s, "Later").clicked();
                    });
                }
            }
        });
        if later {
            self.quiet = true;
        } else if go {
            self.download(ctx, release);
        }
    }

    // The release the pop-up is about, kept while it downloads and after a failed try.
    fn offered(&self) -> Option<&Release> {
        match &self.status {
            Status::Available(r) | Status::Downloading(r) | Status::Failed(_, Some(r)) => Some(r),
            _ => None,
        }
    }
}
