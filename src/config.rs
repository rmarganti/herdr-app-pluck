//! Config shared with herdr-pluck: `~/.config/herdr-pluck/config.toml` plus project
//! `.herdr-pluck.toml` files. Same schema; `clipboard` is the old plugin's and ignored here.

use crate::model::PatternSpec;
use crate::patterns::CustomPatternDefinition;
use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "config.toml";
const APP_CONFIG_DIR: &str = "herdr-pluck";
const DEFAULT_PROJECT_CONFIG_FILE: &str = ".herdr-pluck.toml";

/// Unknown fields are rejected so typos fail loudly, except `clipboard`, which belongs to
/// herdr-pluck (the TUI plugin) sharing this file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    #[allow(dead_code)]
    clipboard: Option<toml::Value>,
    #[serde(default)]
    project: ProjectConfig,
    #[serde(default)]
    patterns: Vec<PatternConfigEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectConfig {
    #[serde(default = "default_true")]
    patterns: bool,
    #[serde(default = "default_project_pattern_files")]
    pattern_files: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            patterns: true,
            pattern_files: default_project_pattern_files(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct PatternConfigEntry {
    name: String,
    regex: String,
    #[serde(default = "default_custom_priority")]
    priority: u16,
}

fn default_custom_priority() -> u16 {
    25
}

fn default_true() -> bool {
    true
}

fn default_project_pattern_files() -> Vec<String> {
    vec![DEFAULT_PROJECT_CONFIG_FILE.to_string()]
}

/// Custom patterns for one invocation, plus problems worth telling the user about.
#[derive(Debug, Default)]
pub struct LoadedPatterns {
    pub patterns: Vec<CustomPatternDefinition>,
    pub warnings: Vec<String>,
}

/// `$XDG_CONFIG_HOME/herdr-pluck/config.toml`, else `~/.config/herdr-pluck/config.toml`.
pub fn default_config_path() -> Option<PathBuf> {
    config_dir_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
    .map(|dir| dir.join(CONFIG_FILE))
}

fn config_dir_from(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let non_empty = |value: Option<OsString>| value.filter(|value| !value.is_empty());
    if let Some(path) = non_empty(xdg_config_home) {
        return Some(PathBuf::from(path).join(APP_CONFIG_DIR));
    }
    non_empty(home).map(|home| PathBuf::from(home).join(".config").join(APP_CONFIG_DIR))
}

/// Reads the global config and, from `cwd`, the nearest project file (read every time).
///
/// A broken global file means defaults (no custom patterns) and a warning; a broken
/// project file is skipped with a warning; an invalid regex skips that pattern.
pub fn load_patterns(config_path: Option<&Path>, cwd: Option<&Path>) -> LoadedPatterns {
    let mut warnings = Vec::new();
    let global = match config_path.map(load_config_file) {
        Some(Ok(Some(config))) => config,
        Some(Ok(None)) | None => ConfigFile::default(),
        Some(Err(error)) => {
            warnings.push(format!("{error}; using defaults"));
            ConfigFile::default()
        }
    };

    let mut specs = Vec::new();
    if global.project.patterns {
        if let Some(cwd) = cwd {
            specs.extend(load_project_pattern_specs(
                cwd,
                &global.project.pattern_files,
                &mut warnings,
            ));
        }
    }
    specs.extend(entries_to_specs(global.patterns));

    let patterns = specs
        .iter()
        .filter_map(|spec| {
            CustomPatternDefinition::compile(spec.name.clone(), spec.priority, &spec.regex)
                .map_err(|error| {
                    warnings.push(format!(
                        "pattern `{}` has an invalid regex, skipped: {}",
                        spec.name,
                        first_line(&error.to_string())
                    ))
                })
                .ok()
        })
        .collect();
    LoadedPatterns { patterns, warnings }
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or(text)
        .to_string()
}

fn load_project_pattern_specs(
    cwd: &Path,
    pattern_files: &[String],
    warnings: &mut Vec<String>,
) -> Vec<PatternSpec> {
    let Some(git_root) = find_git_root(cwd) else {
        return Vec::new();
    };

    for dir in cwd
        .ancestors()
        .take_while(|ancestor| ancestor.starts_with(&git_root))
    {
        for file_name in pattern_files {
            match load_config_file(&dir.join(file_name)) {
                Ok(Some(config)) => return entries_to_specs(config.patterns),
                Ok(None) => continue,
                Err(error) => {
                    warnings.push(format!("{error}; project patterns skipped"));
                    return Vec::new();
                }
            }
        }
    }
    Vec::new()
}

/// `Ok(None)` when the file doesn't exist. Errors are short enough for a toast and
/// name the file, line and problem: "config.toml line 2: unknown field `foo`".
fn load_config_file(path: &Path) -> Result<Option<ConfigFile>, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{name}: {error}")),
    };
    toml::from_str(&content)
        .map(Some)
        .map_err(|error| describe_toml_error(&name, &content, &error))
}

fn describe_toml_error(name: &str, content: &str, error: &toml::de::Error) -> String {
    let message = error.message().trim().replace('\n', " ");
    // "unknown field `foo`, expected one of ..." : the key is what matters.
    let message = message
        .split_once(", expected")
        .map_or(message.as_str(), |(head, _)| head)
        .to_string();
    match error.span() {
        Some(span) => {
            let line = content[..span.start.min(content.len())]
                .matches('\n')
                .count()
                + 1;
            format!("{name} line {line}: {message}")
        }
        None => format!("{name}: {message}"),
    }
}

fn entries_to_specs(entries: Vec<PatternConfigEntry>) -> Vec<PatternSpec> {
    entries
        .into_iter()
        .map(|entry| PatternSpec {
            name: entry.name,
            regex: entry.regex,
            priority: entry.priority,
        })
        .collect()
}

fn find_git_root(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("herdr-app-pluck-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn patterns_get_default_priority_and_project_defaults() {
        let config: ConfigFile =
            toml::from_str("[[patterns]]\nname = \"t\"\nregex = \"ABC-(?<match>[0-9]+)\"\n")
                .unwrap();
        assert_eq!(config.patterns[0].priority, 25);
        assert!(config.project.patterns);
        assert_eq!(config.project.pattern_files, vec![".herdr-pluck.toml"]);
    }

    #[test]
    fn shared_clipboard_key_is_accepted_and_ignored() {
        let dir = tempdir("clipboard");
        let path = dir.join(CONFIG_FILE);
        write(
            &path,
            "clipboard = \"system\"\n[[patterns]]\nname = \"t\"\nregex = \"ABC-[0-9]+\"\n",
        );
        let loaded = load_patterns(Some(&path), None);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.patterns.len(), 1);
    }

    #[test]
    fn unknown_keys_are_rejected_with_a_warning_and_defaults() {
        let dir = tempdir("unknown");
        let path = dir.join(CONFIG_FILE);
        write(
            &path,
            "foo = 1\n[[patterns]]\nname = \"t\"\nregex = \"x\"\n",
        );
        let loaded = load_patterns(Some(&path), None);
        assert!(loaded.patterns.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(
            loaded.warnings,
            vec!["config.toml line 1: unknown field `foo`; using defaults"]
        );
    }

    #[test]
    fn misplaced_and_misspelled_keys_are_rejected() {
        assert!(toml::from_str::<ConfigFile>("[project]\npaterns = false").is_err());
        assert!(toml::from_str::<ConfigFile>(
            "[[patterns]]\nname = \"t\"\nregex = \"x\"\nclipboard = \"system\"\n"
        )
        .is_err());
    }

    #[test]
    fn missing_config_is_silent_defaults() {
        let loaded = load_patterns(
            Some(Path::new("/nonexistent/herdr-pluck/config.toml")),
            None,
        );
        assert!(loaded.patterns.is_empty());
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn invalid_regex_is_skipped_with_a_warning() {
        let dir = tempdir("regex");
        let path = dir.join(CONFIG_FILE);
        write(
            &path,
            "[[patterns]]\nname = \"bad\"\nregex = \"(\"\n[[patterns]]\nname = \"ok\"\nregex = \"x\"\n",
        );
        let loaded = load_patterns(Some(&path), None);
        assert_eq!(loaded.patterns.len(), 1);
        assert_eq!(loaded.patterns[0].name(), "ok");
        assert!(loaded.warnings[0].contains("`bad`"));
    }

    #[test]
    fn project_config_is_found_up_to_git_root_and_comes_first() {
        let root = tempdir("project");
        std::fs::create_dir(root.join(".git")).unwrap();
        let nested = root.join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        write(
            &root.join(DEFAULT_PROJECT_CONFIG_FILE),
            "[[patterns]]\nname = \"project\"\nregex = \"P-[0-9]+\"\n",
        );
        let global = root.join("global.toml");
        write(
            &global,
            "[[patterns]]\nname = \"global\"\nregex = \"G-[0-9]+\"\n",
        );

        let loaded = load_patterns(Some(&global), Some(&nested));
        let names: Vec<_> = loaded.patterns.iter().map(|p| p.name()).collect();
        assert_eq!(names, vec!["project", "global"]);
    }

    #[test]
    fn project_patterns_can_be_disabled() {
        let root = tempdir("project-off");
        std::fs::create_dir(root.join(".git")).unwrap();
        write(
            &root.join(DEFAULT_PROJECT_CONFIG_FILE),
            "[[patterns]]\nname = \"project\"\nregex = \"P\"\n",
        );
        let global = root.join("global.toml");
        write(&global, "[project]\npatterns = false\n");
        assert!(load_patterns(Some(&global), Some(&root))
            .patterns
            .is_empty());
    }

    #[test]
    fn broken_project_config_is_skipped_with_a_warning() {
        let root = tempdir("project-bad");
        std::fs::create_dir(root.join(".git")).unwrap();
        write(&root.join(DEFAULT_PROJECT_CONFIG_FILE), "not toml =");
        let loaded = load_patterns(None, Some(&root));
        assert!(loaded.patterns.is_empty());
        assert!(loaded.warnings[0].starts_with(".herdr-pluck.toml line 1:"));
    }

    #[test]
    fn config_dir_prefers_xdg_then_home() {
        let os = |v: &str| Some(OsString::from(v));
        assert_eq!(
            config_dir_from(os("/xdg"), os("/h")),
            Some(PathBuf::from("/xdg/herdr-pluck"))
        );
        assert_eq!(
            config_dir_from(os(""), os("/h")),
            Some(PathBuf::from("/h/.config/herdr-pluck"))
        );
        assert_eq!(config_dir_from(None, None), None);
    }
}
