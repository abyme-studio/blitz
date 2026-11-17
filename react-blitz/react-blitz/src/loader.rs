use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use url::Url;

/// Where an engine's modules come from.
pub trait ModuleSource {
    /// The absolute URL `specifier` names, imported from `referrer`.
    fn resolve(&self, specifier: &str, referrer: &str) -> anyhow::Result<String>;
    /// The source of the module at `url`.
    fn load(&self, url: &str) -> anyhow::Result<String>;
}

/// A built page's folder (`vite build`'s `dist/`): its modules, served as `file:` URLs under it.
pub struct DistLoader {
    root: PathBuf,
}

impl DistLoader {
    pub fn new(root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("no build at {}", root.as_ref().display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The URL of a file in the build, by its path from the root (`assets/main.js`).
    pub fn url(&self, path: &str) -> String {
        Url::from_file_path(self.root.join(path.trim_start_matches('/')))
            .unwrap()
            .to_string()
    }
}

impl ModuleSource for DistLoader {
    fn resolve(&self, specifier: &str, referrer: &str) -> anyhow::Result<String> {
        if let Ok(url) = Url::parse(specifier) {
            return Ok(url.to_string());
        }
        if specifier.starts_with('/') {
            return Ok(self.url(specifier));
        }
        let base = Url::parse(referrer).with_context(|| format!("{specifier} from {referrer}"))?;
        let url = base
            .join(specifier)
            .with_context(|| format!("cannot resolve {specifier} from {referrer}"))?;
        Ok(url.to_string())
    }

    fn load(&self, url: &str) -> anyhow::Result<String> {
        let url = Url::parse(url)?;
        if url.scheme() != "file" {
            bail!("{url}: only the build's own files load");
        }
        let path = url.to_file_path().map_err(|_| anyhow::anyhow!("{url}: not a file path"))?;
        if !path.starts_with(&self.root) {
            bail!("{}: outside the build at {}", path.display(), self.root.display());
        }
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
    }
}
