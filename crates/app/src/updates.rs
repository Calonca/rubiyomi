// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// Only the desktop downloads from GitHub (feature `update-check`: the AppImage and the .exe)
// look for updates: they download the new file beside themselves and restart into it.
// Everywhere else the store or the packager updates the app.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nsh_base::{AlertDialogResult, alert_dialog};
use nsh_core::prelude::*;
use rubiyomi_core::update::{self, Progress, Release, UpdateSettings};

use crate::ui;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Build {
    // The AppImage file, not the binary mounted from it.
    AppImage(PathBuf),
    Exe(PathBuf),
}

impl Build {
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
    // With the percent done, once the size is known.
    Downloading(Release, Option<u32>),
    // With the release when it was its download that failed.
    Failed(String, Option<Release>),
}

#[derive(Clone)]
pub struct Updates {
    build: Build,
    data_dir: PathBuf,
    pub check_on_start: Signal<bool>,
    pub status: Signal<Status>,
    // The app's root: checks and downloads go on when the screen that started them closes.
    owner: Owner,
}

// The updates of this build, if it is one that looks for them.
pub fn get() -> Option<Updates> {
    use_context::<Updates>()
}

// At the app's start: provides the updates to the settings, and checks if that's wanted.
pub fn start(data_dir: PathBuf) {
    let Some(build) = Build::detect() else { return };
    let settings = UpdateSettings::load(&data_dir);
    let updates =
        Updates { build, data_dir, check_on_start: signal(settings.check_on_start), status: signal(Status::Idle), owner: Owner::current() };
    provide_context(updates.clone());
    if settings.check_on_start {
        updates.check(false);
    }
}

impl Updates {
    fn busy(&self) -> bool {
        self.status.with_untracked(|s| matches!(s, Status::Checking | Status::Downloading(..)))
    }

    pub fn set_check_on_start(&self, on: bool) {
        self.check_on_start.set(on);
        if let Err(e) = (UpdateSettings { check_on_start: on }).save(&self.data_dir) {
            log::warn!("saving the update settings: {e}");
        }
    }

    // `asked`: from Check now, so the page says how it went; at the start only a newer release speaks up.
    pub fn check(&self, asked: bool) {
        if self.busy() {
            return;
        }
        self.status.set(Status::Checking);
        // A pretend release from tool/update-feed.py.
        let feed = std::env::var("RUBIYOMI_UPDATE_URL").ok();
        let me = self.clone();
        self.owner.run(|| {
            spawn(async move {
                let answer = blocking(move || update::latest(feed.as_deref())).await.unwrap_or_else(|e| Err(e.to_string()));
                match answer {
                    Ok(release) if update::newer(release.version(), env!("CARGO_PKG_VERSION")) => {
                        me.status.set(Status::Available(release.clone()));
                        me.offer(release);
                    }
                    Ok(_) => me.status.set(Status::UpToDate),
                    Err(e) => {
                        log::info!("update check: {e}");
                        me.status.set(if asked { Status::Failed(e, None) } else { Status::Idle });
                    }
                }
            });
        });
    }

    fn offer(&self, release: Release) {
        let notes = release.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("No release notes.");
        let answer = alert_dialog()
            .title(format!("Rubiyomi {} is out", release.version()))
            .description(notes.to_owned())
            .cancel("later", "Later")
            .action("download", "Download")
            .show();
        self.on_answer(answer, release, "download");
    }

    fn on_answer(&self, answer: impl Future<Output = Result<AlertDialogResult, String>> + 'static, release: Release, go: &'static str) {
        let me = self.clone();
        self.owner.run(|| {
            spawn(async move {
                if answer.await.ok().and_then(|r| r.action).as_deref() == Some(go) {
                    me.download(release);
                }
            });
        });
    }

    pub fn download(&self, release: Release) {
        if self.busy() {
            return;
        }
        let (build, progress) = (self.build.clone(), Progress::default());
        let (me, status) = (self.clone(), self.status);
        let version = release.version().to_owned();
        status.set(Status::Downloading(release.clone(), None));
        ui::snackbar(format!("Downloading Rubiyomi {version}\u{2026}"));
        self.owner.run(|| {
            let (shown, said) = (progress.clone(), version.clone());
            let watch = set_interval(Duration::from_secs(1), move || {
                let percent = shown.fraction().map(|f| (f * 100.0) as u32);
                let done = percent.map(|p| format!(", {p}%")).unwrap_or_default();
                ui::snackbar(format!("Downloading Rubiyomi {said}{done}\u{2026}"));
                status.update(|s| {
                    if let Status::Downloading(_, p) = s {
                        *p = percent;
                    }
                });
            });
            spawn(async move {
                let job = release.clone();
                let saved = blocking(move || {
                    let name = job.asset(build.suffix()).map(|a| a.name.clone()).ok_or("this release has no file for this computer")?;
                    let folder = build.folder();
                    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                    let to = folder.join(name);
                    update::download(&job, build.suffix(), &to, &progress)?;
                    build.restart(&to)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                watch.cancel();
                match saved {
                    Ok(()) => nsh_core::element::desktop::quit(),
                    Err(e) => {
                        log::warn!("update: {e}");
                        status.set(Status::Failed(e.clone(), Some(release.clone())));
                        let answer = alert_dialog()
                            .title("Couldn't update")
                            .description(format!("Rubiyomi {version}: {e}"))
                            .cancel("later", "Later")
                            .action("retry", "Try again")
                            .show();
                        me.on_answer(answer, release, "retry");
                    }
                }
            });
        });
    }
}

// The Updates page of the settings.
pub fn page(updates: Updates) -> AnyElement {
    let (on, status) = (updates.check_on_start, updates.status);
    let switch = updates.clone();
    view()
        .child(ui::switch_row(
            "Check when it starts",
            "Looks for a new version on GitHub",
            move || on.get(),
            move |v| switch.set_check_on_start(v),
        ))
        .child(dynamic(move || {
            let updates = updates.clone();
            let check = |said: Option<AnyElement>| {
                let updates = updates.clone();
                view().child(said).child(ui::button_row("Check now", move || updates.check(true))).into_element()
            };
            let download = |said: AnyElement, label: &'static str, release: Release| {
                let updates = updates.clone();
                view().child(said).child(ui::button_row(label, move || updates.download(release.clone()))).into_element()
            };
            match status.get() {
                Status::Idle => check(None),
                Status::Checking => ui::hint("Checking\u{2026}").into_element(),
                Status::UpToDate => {
                    check(Some(ui::hint(concat!("Rubiyomi ", env!("CARGO_PKG_VERSION"), " is the newest.")).into_element()))
                }
                Status::Failed(e, None) => check(Some(ui::error(format!("Couldn't check: {e}")).into_element())),
                Status::Available(r) => download(ui::hint(format!("Rubiyomi {} is out.", r.version())).into_element(), "Download", r),
                Status::Failed(e, Some(r)) => download(ui::error(format!("Couldn't update: {e}")).into_element(), "Try again", r),
                Status::Downloading(r, percent) => {
                    let done = percent.map(|p| format!(", {p}%")).unwrap_or_default();
                    ui::hint(format!("Downloading Rubiyomi {}{done}\u{2026}", r.version())).into_element()
                }
            }
        }))
        .into_element()
}
