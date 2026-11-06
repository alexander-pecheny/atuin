//! Each harness once per home directory it keeps sessions in: the one its environment names, then
//! each one `[[threads.homes]]` adds.

use atuin_common::harnesstools::AnyHarness;
use atuin_common::path::{AbsPath, AbsPathBuf};
use atuin_common::userio;

use super::HarnessKind;
use crate::settings::HarnessHome;

/// The harnesses `[[threads.homes]]` adds, each at its home.
#[derive(Clone, Debug, Default)]
pub struct Homes(Vec<AnyHarness>);

impl Homes {
    #[must_use]
    pub fn new(homes: &[HarnessHome]) -> Self {
        let mut seen: Vec<(HarnessKind, &AbsPathBuf)> = Vec::new();
        let mut added = Vec::new();
        for home in homes {
            let Ok(harness) = AnyHarness::from_name(&home.harness) else {
                userio::log::warn!("threads-home-unknown-harness", harness = home.harness.clone());
                continue;
            };
            if !harness.takes_home() {
                userio::log::warn!("threads-home-unsupported", harness = home.harness.clone());
                continue;
            }
            let key = (HarnessKind::from(&harness), &home.path);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            // Harnesses are `Copy` values passed everywhere, and a process reads its few homes
            // once: leaking each lets them hold it.
            let path: &'static AbsPath = Box::leak(Box::new(home.path.clone()));
            added.extend(harness.at(path));
        }
        Self(added)
    }

    /// Every harness: each at its environment's home, then each added one.
    pub fn all(&self) -> impl Iterator<Item = AnyHarness> + '_ {
        AnyHarness::all().iter().copied().chain(self.0.iter().copied())
    }

    /// [`Self::all`] of kind `kind`.
    pub fn of(&self, kind: HarnessKind) -> impl Iterator<Item = AnyHarness> + '_ {
        self.all().filter(move |harness| HarnessKind::from(harness) == kind)
    }
}

#[cfg(test)]
mod tests {
    use atuin_common::env::MockEnv;
    use rstest::{fixture, rstest};

    use super::*;

    #[fixture]
    fn home() -> (tempfile::TempDir, MockEnv) {
        let dir = tempfile::tempdir().unwrap();
        let env = MockEnv::install();
        env.set("HOME", dir.path());
        env.set("CLAUDE_CONFIG_DIR", dir.path().join("work"));
        (dir, env)
    }

    /// One more harness per distinct home a harness can be pointed at, besides the one its
    /// environment names.
    #[rstest]
    #[case::another_home(&[("claude-code", "personal")], 2)]
    #[case::the_same_home_twice(&[("claude-code", "personal"), ("claude-code", "personal")], 2)]
    #[case::the_environments_home(&[("claude-code", "work")], 1)]
    #[case::by_an_alias(&[("claude", "personal")], 2)]
    #[case::another_harness(&[("codex", "personal")], 1)]
    #[case::a_harness_without_a_home(&[("opencode", "personal")], 1)]
    #[case::an_unknown_harness(&[("no-such-harness", "personal")], 1)]
    fn each_distinct_home_is_one_more_harness(
        home: (tempfile::TempDir, MockEnv),
        #[case] entries: &[(&str, &str)],
        #[case] expected: usize,
    ) {
        let (dir, _env) = home;
        let entries: Vec<_> = entries
            .iter()
            .map(|(harness, name)| HarnessHome {
                harness: (*harness).to_owned(),
                path: AbsPathBuf::new(dir.path().join(name)).unwrap(),
            })
            .collect();
        assert_eq!(Homes::new(&entries).of(HarnessKind::ClaudeCode).count(), expected);
    }
}
