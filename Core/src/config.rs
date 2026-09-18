use crate::text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Rules {
    pub names: BTreeSet<String>,
    pub path_prefixes: Vec<String>,
    pub exclude_hidden: bool,
    pub exclude_dev_folders: bool,
    #[serde(rename = "excludeVCSFolders", alias = "excludeVcsFolders")]
    pub exclude_vcs_folders: bool,
    pub exclude_trash: bool,
    pub exclude_file_patterns: Vec<String>,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            names: BTreeSet::new(),
            path_prefixes: vec![],
            exclude_hidden: false,
            exclude_dev_folders: true,
            exclude_vcs_folders: true,
            exclude_trash: true,
            exclude_file_patterns: vec![],
        }
    }
}
pub const MARKERS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "CMakeLists.txt",
    "Makefile",
    "GNUmakefile",
    "composer.json",
    "Gemfile",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "tsconfig.json",
    "Package.swift",
    ".git",
];
const DEV: &[&str] = &[
    "node_modules",
    "bower_components",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".astro",
    ".angular",
    ".turbo",
    ".parcel-cache",
    ".build",
    "DerivedData",
    "Pods",
    "Carthage",
    ".gradle",
    ".cargo",
    ".rustup",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    ".eggs",
    "venv",
    ".venv",
    ".terraform",
    ".cache",
];
pub const SCOPED: &[&str] = &["build", "dist", "target", "out", "vendor", "coverage"];

pub fn within(path: &str, root: &str) -> bool {
    within_canonical(&text::canonical(path), &text::canonical(root))
}
fn within_canonical(path: &str, root: &str) -> bool {
    root == "/"
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}
impl Rules {
    fn excludes_path(&self, path: &str) -> bool {
        let path = text::canonical(path);
        self.path_prefixes
            .iter()
            .any(|p| within_canonical(&path, p.trim_end_matches('/')))
    }
    pub fn excluded_without_metadata(&self, name: &str, path: &str) -> bool {
        self.exclude_hidden && name.starts_with('.')
            || self.names.contains(text::canonical(name).as_ref())
            || self.excludes_path(path)
            || self.exclude_vcs_folders && [".git", ".hg", ".svn"].contains(&name)
            || self.exclude_trash && [".Trash", ".Trashes"].contains(&name)
    }
    pub fn excluded(&self, name: &str, path: &str, directory: bool, project: bool) -> bool {
        if self.excluded_without_metadata(name, path)
            || self.exclude_dev_folders
                && directory
                && (DEV.contains(&name) || project && SCOPED.contains(&name))
        {
            return true;
        }
        if directory {
            return false;
        }
        let name = text::normalize(name, true);
        self.exclude_file_patterns
            .iter()
            .filter(|s| !s.is_empty())
            .any(|pattern| {
                let p = text::normalize(pattern, true);
                if p.contains(['*', '?']) {
                    text::matches(&p, &name, false)
                } else {
                    p == name
                }
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub roots: Vec<String>,
    #[serde(default)]
    pub rules: Rules,
}
impl Config {
    pub fn normalize_exclusions(&mut self) {
        self.rules.names = self
            .rules
            .names
            .iter()
            .map(|s| text::normalize(s, false))
            .collect();
        for path in &mut self.rules.path_prefixes {
            *path = text::normalize(path, false);
        }
        self.rules.path_prefixes.sort();
        self.rules.path_prefixes.dedup();
    }
    pub fn includes(&self, path: &str) -> bool {
        self.roots.iter().any(|r| within(path, r))
    }
    pub fn excluded_path(&self, path: &str) -> bool {
        // Explicit paths also apply to a configured root. Do this before any
        // project-marker probes so excluded trees require no filesystem access.
        if self.rules.excludes_path(path) {
            return true;
        }
        let mut scoped = Vec::new();
        for ancestor in std::path::Path::new(path).ancestors() {
            let full = ancestor.to_string_lossy();
            if self
                .roots
                .iter()
                .any(|r| text::canonical(r) == text::canonical(&full))
            {
                break;
            }
            let Some(name) = ancestor.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if self.rules.excluded(name, &full, true, false) {
                return true;
            }
            if self.rules.exclude_dev_folders && SCOPED.contains(&name) {
                scoped.push(ancestor);
            }
        }
        // Only probe project markers after all ancestors pass pure exclusions.
        scoped.iter().any(|ancestor| {
            ancestor
                .parent()
                .is_some_and(|parent| MARKERS.iter().any(|marker| parent.join(marker).exists()))
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.roots.is_empty() {
            return Err("At least one index root is required".into());
        }
        for root in &self.roots {
            if !root.starts_with('/')
                || root.contains("/../")
                || root.contains("/./")
                || root != "/" && root.ends_with('/')
            {
                return Err(format!("Invalid index root: {root}"));
            }
        }
        Ok(())
    }
}
