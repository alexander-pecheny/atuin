use std::path::PathBuf;

use super::rehydrate::{RehydrateError, RehydrateSession};
use super::resume::{self, CwdRequirement, ResumeError, ResumePlan, ResumeTarget};
use super::{Harness, InstallHookError, json_hooks};
use crate::dirs::home_dir;
use crate::env::var_nonempty;
use crate::path::{AbsPath, PathBufExt as _};

mod hook_trust;
pub mod rehydrate;
pub mod session;
pub(crate) mod state_db;

/// Codex's home: `$CODEX_HOME`, else `~/.codex`.
pub(crate) fn home() -> PathBuf {
    var_nonempty("CODEX_HOME").map_or_else(|| home_dir().joined(".codex"), PathBuf::from)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Codex {
    /// A home other than the one the environment names (`[[threads.homes]]`).
    home: Option<&'static AbsPath>,
}

impl Codex {
    /// Codex with the home the environment names.
    pub const DEFAULT: Self = Self { home: None };

    #[must_use]
    pub fn new() -> Self {
        Self::DEFAULT
    }

    /// Codex with the home `home`, `None` when the environment names it already.
    #[must_use]
    pub fn at(home: &'static AbsPath) -> Option<Self> {
        (home.as_path() != self::home()).then_some(Self { home: Some(home) })
    }

    /// The home this reads and writes.
    pub(crate) fn home(&self) -> PathBuf {
        self.home.map_or_else(self::home, |home| home.to_path_buf())
    }

    /// The sessions directory, which holds the rollouts.
    pub(crate) fn root(&self) -> PathBuf {
        self.home().joined("sessions")
    }
}

impl Harness for Codex {
    fn name(&self) -> &'static str {
        "codex"
    }

    /// Install the hooks on the current active user's machine, trusted (see `hook_trust`).
    async fn install_hooks(&self) -> Result<PathBuf, InstallHookError> {
        // Codex reads Claude-Code-style hooks from $CODEX_HOME/hooks.json.
        let home = self.home();
        let config_path = home.join("hooks.json");
        let config = hook_trust::config(&home).await?;
        let installed = match json_hooks::install(&config_path, "^Bash$", self.name()).await {
            Ok(()) => true,
            Err(InstallHookError::AlreadyInstalled) => false,
            Err(err) => return Err(err),
        };
        // Hooks installed before atuin trusted them are trusted on installing them again.
        let trusted = hook_trust::trust(&home, self.name(), config).await?;
        if !installed && !trusted {
            return Err(InstallHookError::AlreadyInstalled);
        }
        Ok(config_path)
    }

    /// `codex resume <id>`. Codex finds the id from any directory, but asks whether to work in
    /// the session's directory or the current one when they differ.
    fn resume_plan(&self, target: &ResumeTarget) -> Result<ResumePlan, ResumeError> {
        let id = session::resume_id(&target.id);
        let mut plan = resume::plan(target, CwdRequirement::Preferred, "codex", [
            "resume".to_owned(),
            id.to_owned(),
        ]);
        if let Some(home) = self.home {
            let unset = home_dir().joined(".codex");
            plan.env.push(resume::home_env("CODEX_HOME", home.as_path(), &unset)?);
        }
        Ok(plan)
    }

    async fn locate(&self, id: &str) -> Option<PathBuf> {
        let (root, id) = (self.root(), id.to_owned());
        resume::blocking(move || session::locate(&root, &id)).await
    }

    async fn rehydrate(&self, session: &RehydrateSession) -> Result<PathBuf, RehydrateError> {
        rehydrate::rehydrate_into(&self.root(), &self.home(), session).await
    }
}
