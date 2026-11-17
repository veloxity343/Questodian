//! Checking GitHub for a newer Questodian, as WattsLeft does.
//!
//! On first launch the UI asks whether to check; the answer is kept in `updates.json` in the
//! app's config folder (not the data folder, which holds campaigns). While it's on, the app
//! checks once a day, or sooner after a check that failed. A check is one HTTPS request:
//! GitHub answers `/releases/latest` with a redirect to that release's page, whose address
//! names the version. Nothing about the user is sent, and the reply's body isn't read; the
//! release page opened is built from the version, not taken from the reply.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const REPOSITORY: &str = "veloxity343/Questodian";
/// How often to check while checking is on, and how soon to try again after a failed check.
pub const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
pub const RETRY: Duration = Duration::from_secs(3 * 60 * 60);

/// A version as the releases are tagged: `v1.2.3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// `1.2.3`, with any `-beta` or `+build` suffix ignored.
    pub fn parse(text: &str) -> Option<Version> {
        let core = text.trim().trim_start_matches('v').split(['-', '+']).next()?;
        let mut parts = core.split('.').map(|p| p.parse::<u32>().ok().filter(|_| !p.is_empty() && p.len() <= 5));
        let version = Version(parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then_some(version)
    }

    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("the package version is x.y.z")
    }

    pub fn release_page(self) -> String {
        format!("https://github.com/{REPOSITORY}/releases/tag/v{self}")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The version a `/releases/latest` redirect points to. Only exactly a release page of this
/// repository with a plain version tag counts; anything else is ignored.
pub fn version_from_redirect(location: &str) -> Option<Version> {
    let prefix = format!("https://github.com/{REPOSITORY}/releases/tag/v");
    let tag = location.strip_prefix(&prefix)?;
    tag.chars().all(|c| c.is_ascii_digit() || c == '.').then(|| Version::parse(tag)).flatten()
}

/// One check: when, and whether GitHub answered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LastCheck {
    /// RFC 3339.
    pub when: String,
    pub answered: bool,
}

/// What's kept between launches.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Saved {
    /// `None` until the first-launch question is answered.
    pub check: Option<bool>,
    pub last_check: Option<LastCheck>,
    /// The newest release found, if it was newer than this app then.
    pub latest: Option<Version>,
    /// The newest version the pop-up has been shown for.
    pub announced: Option<Version>,
}

impl Saved {
    pub fn load(path: &Path) -> Saved {
        std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    /// Written to a temporary file first, so a crash never leaves half a file.
    pub fn store(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(self).expect("settings serialize"))?;
        std::fs::rename(temporary, path)
    }

    /// A newer release than this app, if one was found.
    pub fn available(&self) -> Option<Version> {
        self.latest.filter(|v| *v > Version::current())
    }

    /// Whether a check is due at `now`.
    pub fn due(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        if self.check != Some(true) {
            return false;
        }
        let Some(last) = &self.last_check else { return true };
        let Ok(when) = chrono::DateTime::parse_from_rfc3339(&last.when) else { return true };
        let wait = if last.answered { INTERVAL } else { RETRY };
        // A clock set back makes the last check look like it's in the future: check then too.
        let elapsed = now.signed_duration_since(when.with_timezone(&chrono::Utc));
        elapsed.num_seconds() < 0 || elapsed.to_std().unwrap_or_default() >= wait
    }
}

/// `updates.json` in the app's config folder.
pub fn settings_path(config_dir: PathBuf) -> PathBuf {
    config_dir.join("updates.json")
}

// --------------------------------------------------------------------------- at run time

/// Everything the UI shows about updates (`GET /desktop/updates`).
#[derive(Debug, Clone, Serialize)]
pub struct State {
    pub current: String,
    pub check: Option<bool>,
    pub checking: bool,
    pub last_check: Option<LastCheck>,
    pub available: Option<Available>,
    pub announced: Option<String>,
    pub can_install: bool,
    pub installing: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Available {
    pub version: String,
    pub url: String,
}

/// The updater's state while the app runs; `Saved` is the part kept on disk.
pub struct Updates {
    path: PathBuf,
    inner: std::sync::Mutex<Live>,
}

struct Live {
    saved: Saved,
    checking: bool,
    installing: Option<f64>,
    error: Option<String>,
}

impl Updates {
    pub fn load(path: PathBuf) -> Updates {
        let saved = Saved::load(&path);
        Updates { path, inner: std::sync::Mutex::new(Live { saved, checking: false, installing: None, error: None }) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Live> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn state(&self) -> State {
        let live = self.lock();
        State {
            current: Version::current().to_string(),
            check: live.saved.check,
            checking: live.checking,
            last_check: live.saved.last_check.clone(),
            available: live.saved.available().map(|v| Available { version: v.to_string(), url: v.release_page() }),
            announced: live.saved.announced.map(|v| v.to_string()),
            can_install: cfg!(feature = "updater"),
            installing: live.installing,
            error: live.error.clone(),
        }
    }

    fn save(&self, live: &Live) {
        if let Err(e) = live.saved.store(&self.path) {
            eprintln!("questodian: couldn't save the update settings to {}: {e}", self.path.display());
        }
    }

    /// The first-launch answer or the Settings switch, and the pop-up being seen.
    pub fn change(&self, check: Option<bool>, announced: Option<&str>) {
        let mut live = self.lock();
        if let Some(on) = check {
            live.saved.check = Some(on);
        }
        if let Some(version) = announced.and_then(Version::parse) {
            live.saved.announced = Some(version);
        }
        self.save(&live);
    }

    pub fn due(&self) -> bool {
        self.lock().saved.due(chrono::Utc::now())
    }

    pub fn available(&self) -> Option<Version> {
        self.lock().saved.available()
    }

    /// Ask GitHub now, unless a check is already under way or checking is off.
    pub async fn check_now(&self) {
        {
            let mut live = self.lock();
            if live.checking || live.saved.check != Some(true) {
                return;
            }
            live.checking = true;
            live.error = None;
        }
        let found = latest_release().await;
        let mut live = self.lock();
        live.checking = false;
        let answered = found.is_ok();
        live.saved.last_check = Some(LastCheck { when: chrono::Utc::now().to_rfc3339(), answered });
        match found {
            Ok(version) => live.saved.latest = (version > Version::current()).then_some(version),
            Err(e) => {
                eprintln!("questodian: checking for updates failed: {e}");
                live.error = Some("Couldn't reach GitHub to check for updates. It tries again in a few hours.".into());
            }
        }
        self.save(&live);
    }

    #[cfg_attr(not(feature = "updater"), allow(dead_code))] // only installing in place uses it
    pub fn set_installing(&self, progress: Option<f64>, error: Option<String>) {
        let mut live = self.lock();
        live.installing = progress;
        if error.is_some() || progress.is_some() {
            live.error = error;
        }
    }
}

/// The version of the latest release: one HEAD request, whose redirect names it.
async fn latest_release() -> Result<Version, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent(format!("Questodian/{}", Version::current()))
        .build()
        .map_err(|e| e.to_string())?;
    let address = format!("https://github.com/{REPOSITORY}/releases/latest");
    let reply = client.head(&address).send().await.map_err(|e| e.to_string())?;
    let location = reply
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .filter(|_| reply.status().is_redirection())
        .ok_or_else(|| format!("GitHub didn't point to a release ({})", reply.status()))?;
    let base = reqwest::Url::parse(&address).expect("a valid URL");
    let page = base.join(location).map_err(|e| e.to_string())?;
    version_from_redirect(page.as_str()).ok_or_else(|| format!("GitHub pointed somewhere unexpected: {page}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asks GitHub for real: `cargo test -p questodian-app -- --ignored`.
    #[test]
    #[ignore = "needs the network"]
    fn github_names_the_latest_release() {
        let latest = tauri::async_runtime::block_on(latest_release()).expect("an answer from GitHub");
        assert!(latest >= Version(0, 1, 1), "{latest}");
    }

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(Version::parse("0.1.1"), Some(Version(0, 1, 1)));
        assert_eq!(Version::parse("v1.10.0-beta.1"), Some(Version(1, 10, 0)));
        for bad in ["", "1.2", "1.2.3.4", "a.b.c", "1..2", "v1.2.x"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        assert_eq!(Version(0, 2, 0).release_page(), "https://github.com/veloxity343/Questodian/releases/tag/v0.2.0");
    }

    #[test]
    fn only_this_repositorys_release_pages_count() {
        let page = "https://github.com/veloxity343/Questodian/releases/tag/v0.2.0";
        assert_eq!(version_from_redirect(page), Some(Version(0, 2, 0)));
        for other in [
            "https://github.com/someone/Questodian/releases/tag/v9.0.0",
            "https://github.com/veloxity343/Questodian/releases/tag/v0.2.0-rc1",
            "https://github.com/veloxity343/Questodian/releases/tag/v0.2.0/../../x",
            "https://github.com/veloxity343/Questodian/releases",
            "http://github.com/veloxity343/Questodian/releases/tag/v0.2.0",
        ] {
            assert_eq!(version_from_redirect(other), None, "{other}");
        }
    }

    #[test]
    fn checks_are_daily_and_sooner_after_a_failure() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-17T12:00:00Z").unwrap().with_timezone(&chrono::Utc);
        let at = |hours_ago: i64, answered: bool| Saved {
            check: Some(true),
            last_check: Some(LastCheck { when: (now - chrono::Duration::hours(hours_ago)).to_rfc3339(), answered }),
            ..Saved::default()
        };
        assert!(Saved { check: Some(true), ..Saved::default() }.due(now), "never checked");
        assert!(!Saved::default().due(now), "not asked yet");
        assert!(!Saved { check: Some(false), ..Saved::default() }.due(now), "turned off");
        assert!(!at(23, true).due(now));
        assert!(at(24, true).due(now));
        assert!(!at(2, false).due(now));
        assert!(at(3, false).due(now));
        assert!(at(-5, true).due(now), "clock set back");
    }

    #[test]
    fn settings_survive_a_round_trip_and_bad_files() {
        let dir = std::env::temp_dir().join(format!("questodian-updates-test-{}", std::process::id()));
        let path = settings_path(dir.clone());
        let saved = Saved { check: Some(true), latest: Some(Version(9, 0, 0)), ..Saved::default() };
        saved.store(&path).unwrap();
        assert_eq!(Saved::load(&path), saved);
        assert_eq!(saved.available(), Some(Version(9, 0, 0)));
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Saved::load(&path), Saved::default());
        let _ = std::fs::remove_dir_all(dir);
    }
}
