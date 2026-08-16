use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, IsTerminal, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{
    process::{command, require_success},
    util::sha256_file,
};

pub const METADATA_FILE: &str = ".v4hook.toml";
pub const LOCK_FILE: &str = ".v4hook-template-lock.json";
pub static SCAFFOLD: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/v4-template");

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectMetadata {
    pub schema_version: u32,
    pub created_with_cli: String,
    pub last_updated_with_cli: String,
    pub template: ProjectTemplateMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProjectTemplateMetadata {
    pub version: String,
    pub channel: String,
    pub source: String,
    pub revision: String,
    pub manifest_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateLock {
    pub schema_version: String,
    #[serde(default)]
    pub template_version: String,
    pub snapshot: String,
    pub repository: String,
    pub commit: String,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub files: BTreeMap<String, TemplateFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateFile {
    pub sha256: String,
    pub ownership: FileOwnership,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FileOwnership {
    Managed,
    Generated,
    Seed,
}

#[derive(Debug, Clone, Copy)]
pub enum ConflictPolicy {
    Abort,
    Preserve,
    Overwrite,
}

pub struct ScaffoldUpdateInput<'a> {
    pub directory: &'a Path,
    pub dry_run: bool,
    pub conflicts: Option<ConflictPolicy>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldUpdateReport {
    pub directory: String,
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    pub preserved: Vec<String>,
    pub conflicts: Vec<String>,
    pub dry_run: bool,
    pub applied: bool,
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create(label: &str) -> Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..100 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let candidate = base.join(format!("v4hook-{label}-{}-{sequence}", std::process::id()));
            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self(candidate)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("create temporary directory"),
            }
        }
        bail!("could not create a unique temporary directory")
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_public(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("file")
    ));
    fs::write(&temporary, bytes).with_context(|| format!("write {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("replace {}", path.display()))
}

fn write_lock(path: &Path, lock: &TemplateLock) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(lock).context("serialize scaffold lock")?;
    bytes.push(b'\n');
    write_public(path, &bytes)
}

fn write_metadata(path: &Path, metadata: &ProjectMetadata) -> Result<()> {
    let mut text = toml::to_string_pretty(metadata).context("serialize scaffold metadata")?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    write_public(path, text.as_bytes())
}

pub fn read_metadata(root: &Path) -> Result<ProjectMetadata> {
    let path = root.join(METADATA_FILE);
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let metadata: ProjectMetadata =
        toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    if metadata.schema_version != 1 {
        bail!(
            "unsupported .v4hook.toml schema: {}",
            metadata.schema_version
        );
    }
    Version::parse(&metadata.template.version).context("parse template version")?;
    Ok(metadata)
}

pub fn read_lock(root: &Path) -> Result<TemplateLock> {
    let path = root.join(LOCK_FILE);
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

fn relative_string(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            _ => bail!("scaffold path is not relative: {}", path.display()),
        }
    }
    Ok(parts.join("/"))
}

fn ignored_top_level(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".v4hook" | "broadcast" | "cache" | "node_modules" | "out"
    )
}

fn collect_files(root: &Path, directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("read {}", directory.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).context("strip scaffold root")?;
        if relative.components().count() == 1
            && relative
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(ignored_top_level)
        {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &path, output)?;
        } else if file_type.is_file() {
            if relative == Path::new(METADATA_FILE) || relative == Path::new(LOCK_FILE) {
                continue;
            }
            output.push(relative.to_path_buf());
        } else {
            bail!(
                "scaffold contains an unsupported file type: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn ownership(path: &str) -> FileOwnership {
    if path.starts_with("vendor/") {
        FileOwnership::Generated
    } else if path.starts_with("src/") || path.starts_with("test/") || path == "README.md" {
        FileOwnership::Seed
    } else {
        FileOwnership::Managed
    }
}

pub fn seal_scaffold(root: &Path) -> Result<(ProjectMetadata, TemplateLock)> {
    let mut metadata = read_metadata(root)?;
    let previous_lock = read_lock(root)?;
    if metadata.template.source != previous_lock.repository {
        bail!("template source does not match the scaffold lock")
    }
    metadata.template.revision.clone_from(&previous_lock.commit);
    env!("CARGO_PKG_VERSION").clone_into(&mut metadata.last_updated_with_cli);

    let mut paths = Vec::new();
    collect_files(root, root, &mut paths)?;
    let mut files = BTreeMap::new();
    for path in paths {
        let relative = relative_string(&path)?;
        files.insert(
            relative.clone(),
            TemplateFile {
                sha256: sha256_file(root.join(&path))?,
                ownership: ownership(&relative),
            },
        );
    }
    let lock = TemplateLock {
        schema_version: "v4hook.template-lock.v2".to_owned(),
        template_version: metadata.template.version.clone(),
        snapshot: format!("v4hook-template-{}", metadata.template.version),
        repository: previous_lock.repository,
        commit: previous_lock.commit,
        dependencies: previous_lock.dependencies,
        files,
    };
    let lock_path = root.join(LOCK_FILE);
    write_lock(&lock_path, &lock)?;
    metadata.template.manifest_digest = sha256_file(&lock_path)?;
    write_metadata(&root.join(METADATA_FILE), &metadata)?;
    Ok((metadata, lock))
}

fn require_clean_repository(root: &Path) -> Result<()> {
    let result = require_success(
        &command(&["git", "status", "--porcelain=v1", "--untracked-files=all"]),
        root,
        None,
        false,
    )?;
    if !result.stdout.trim().is_empty() {
        bail!("commit or stash project changes before updating the scaffold")
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OperationKind {
    Add,
    Update,
    Remove,
}

#[derive(Debug)]
struct Operation {
    path: String,
    kind: OperationKind,
}

fn current_hash(root: &Path, path: &str) -> Result<Option<String>> {
    let target = root.join(path);
    if target.is_file() {
        Ok(Some(sha256_file(target)?))
    } else if target.exists() {
        bail!("scaffold path is not a regular file: {path}")
    } else {
        Ok(None)
    }
}

fn choose_conflict_policy(conflicts: &[String]) -> Result<ConflictPolicy> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        bail!(
            "{} scaffold conflicts need a choice; use --conflicts preserve or --conflicts overwrite",
            conflicts.len()
        );
    }
    eprintln!("The template and your project changed these files:");
    for path in conflicts {
        eprintln!("  {path}");
    }
    eprint!("Keep your versions of these files? [Y/n] ");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim().is_empty() || answer.trim().eq_ignore_ascii_case("y") {
        return Ok(ConflictPolicy::Preserve);
    }
    eprint!("Replace them with the template versions? [y/N] ");
    io::stderr().flush()?;
    answer.clear();
    io::stdin().read_line(&mut answer)?;
    if answer.trim().eq_ignore_ascii_case("y") {
        Ok(ConflictPolicy::Overwrite)
    } else {
        Ok(ConflictPolicy::Abort)
    }
}

fn backup_file(backups: &mut BTreeMap<PathBuf, Option<Vec<u8>>>, path: &Path) -> Result<()> {
    if backups.contains_key(path) {
        return Ok(());
    }
    let bytes = if path.is_file() {
        Some(fs::read(path).with_context(|| format!("read {}", path.display()))?)
    } else {
        None
    };
    backups.insert(path.to_path_buf(), bytes);
    Ok(())
}

fn restore_files(backups: &BTreeMap<PathBuf, Option<Vec<u8>>>) -> Result<()> {
    let mut failures = Vec::new();
    for (path, bytes) in backups {
        let result = match bytes {
            Some(bytes) => (|| {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("restore directory {}", parent.display()))?;
                }
                fs::write(path, bytes)
                    .with_context(|| format!("restore scaffold file {}", path.display()))
            })(),
            None => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error)
                    .with_context(|| format!("remove added scaffold file {}", path.display())),
            },
        };
        if let Err(error) = result {
            failures.push(error);
        }
    }
    let Some(primary) = failures.pop() else {
        return Ok(());
    };
    if failures.is_empty() {
        return Err(primary);
    }
    let additional = failures
        .iter()
        .map(|error| format!("{error:#}"))
        .collect::<Vec<_>>()
        .join("; ");
    Err(primary).context(format!("additional restoration failures: {additional}"))
}

fn validate_updated_project(root: &Path) -> Result<()> {
    for parts in [&["forge", "build"][..], &["forge", "test"][..]] {
        require_success(&command(parts), root, None, false)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub fn update_scaffold(input: &ScaffoldUpdateInput<'_>) -> Result<ScaffoldUpdateReport> {
    let root = fs::canonicalize(input.directory)
        .with_context(|| format!("resolve {}", input.directory.display()))?;
    let old_metadata = if root.join(METADATA_FILE).is_file() {
        Some(read_metadata(&root)?)
    } else {
        None
    };
    let old_lock = if root.join(LOCK_FILE).is_file() {
        Some(read_lock(&root)?)
    } else {
        None
    };
    if let (Some(metadata), Some(_)) = (&old_metadata, &old_lock)
        && !metadata.template.manifest_digest.is_empty()
        && metadata.template.manifest_digest != sha256_file(root.join(LOCK_FILE))?
    {
        bail!(".v4hook.toml does not match the scaffold lock")
    }

    let temporary = TemporaryDirectory::create("scaffold-update")?;
    let target = temporary.path().join("template");
    fs::create_dir(&target)?;
    SCAFFOLD
        .extract(&target)
        .context("extract bundled scaffold")?;
    let (mut new_metadata, new_lock) = seal_scaffold(&target)?;
    let new_version = Version::parse(&new_metadata.template.version)?;
    if let Some(metadata) = &old_metadata {
        let old_version = Version::parse(&metadata.template.version)?;
        if old_version > new_version {
            bail!(
                "installed CLI template {new_version} is older than project template {old_version}"
            )
        }
        if old_version == new_version
            && metadata.template.manifest_digest != new_metadata.template.manifest_digest
        {
            bail!("template content changed without a template version increase")
        }
        new_metadata
            .created_with_cli
            .clone_from(&metadata.created_with_cli);
    }
    env!("CARGO_PKG_VERSION").clone_into(&mut new_metadata.last_updated_with_cli);

    let mut paths = BTreeSet::new();
    if let Some(lock) = &old_lock {
        paths.extend(lock.files.keys().cloned());
    }
    paths.extend(new_lock.files.keys().cloned());

    let mut operations = Vec::new();
    let mut conflicts = Vec::new();
    for path in paths {
        let old_hash = old_lock
            .as_ref()
            .and_then(|lock| lock.files.get(&path))
            .map(|file| file.sha256.as_str());
        let new_hash = new_lock.files.get(&path).map(|file| file.sha256.as_str());
        if old_hash == new_hash {
            continue;
        }
        let current = current_hash(&root, &path)?;
        match (old_hash, new_hash, current.as_deref()) {
            (_, Some(new), Some(current)) if new == current => {}
            (Some(old), Some(_), Some(current)) if old == current => operations.push(Operation {
                path,
                kind: OperationKind::Update,
            }),
            (_, Some(_), None) => operations.push(Operation {
                path,
                kind: OperationKind::Add,
            }),
            (Some(old), None, Some(current)) if old == current => operations.push(Operation {
                path,
                kind: OperationKind::Remove,
            }),
            (_, None, None) => {}
            _ => conflicts.push(path),
        }
    }

    let policy = if conflicts.is_empty() || input.dry_run {
        input.conflicts.unwrap_or(ConflictPolicy::Abort)
    } else {
        input
            .conflicts
            .map_or_else(|| choose_conflict_policy(&conflicts), Ok)?
    };
    if !conflicts.is_empty() && matches!(policy, ConflictPolicy::Abort) && !input.dry_run {
        bail!(
            "scaffold update stopped because {} files conflict",
            conflicts.len()
        )
    }
    if matches!(policy, ConflictPolicy::Overwrite) {
        for path in &conflicts {
            operations.push(Operation {
                path: path.clone(),
                kind: if new_lock.files.contains_key(path) {
                    if root.join(path).exists() {
                        OperationKind::Update
                    } else {
                        OperationKind::Add
                    }
                } else {
                    OperationKind::Remove
                },
            });
        }
    }

    let mut report = ScaffoldUpdateReport {
        directory: root.to_string_lossy().into_owned(),
        from_version: old_metadata
            .as_ref()
            .map(|metadata| metadata.template.version.clone()),
        to_version: new_metadata.template.version.clone(),
        added: Vec::new(),
        updated: Vec::new(),
        removed: Vec::new(),
        preserved: if matches!(policy, ConflictPolicy::Preserve) {
            conflicts.clone()
        } else {
            Vec::new()
        },
        conflicts,
        dry_run: input.dry_run,
        applied: false,
    };
    for operation in &operations {
        match operation.kind {
            OperationKind::Add => report.added.push(operation.path.clone()),
            OperationKind::Update => report.updated.push(operation.path.clone()),
            OperationKind::Remove => report.removed.push(operation.path.clone()),
        }
    }
    if input.dry_run {
        return Ok(report);
    }
    require_clean_repository(&root)?;

    let mut backups = BTreeMap::new();
    let apply_result: Result<()> = (|| {
        for operation in &operations {
            let destination = root.join(&operation.path);
            backup_file(&mut backups, &destination)?;
            match operation.kind {
                OperationKind::Add | OperationKind::Update => {
                    if let Some(parent) = destination.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(target.join(&operation.path), &destination)
                        .with_context(|| format!("update scaffold file {}", operation.path))?;
                }
                OperationKind::Remove => fs::remove_file(&destination)
                    .with_context(|| format!("remove scaffold file {}", operation.path))?,
            }
        }
        let metadata_path = root.join(METADATA_FILE);
        let lock_path = root.join(LOCK_FILE);
        backup_file(&mut backups, &metadata_path)?;
        backup_file(&mut backups, &lock_path)?;
        write_lock(&lock_path, &new_lock)?;
        new_metadata.template.manifest_digest = sha256_file(&lock_path)?;
        write_metadata(&metadata_path, &new_metadata)?;
        if !operations.is_empty() {
            validate_updated_project(&root)?;
        }
        Ok(())
    })();
    if let Err(error) = apply_result {
        if let Err(restore_error) = restore_files(&backups) {
            return Err(restore_error).context(format!(
                "scaffold update failed ({error:#}); project file restoration also failed"
            ));
        }
        return Err(error).context("scaffold update failed; restored the project files");
    }
    report.applied = true;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_protects_seed_files() {
        assert_eq!(ownership("src/Hook.sol"), FileOwnership::Seed);
        assert_eq!(ownership("test/Hook.t.sol"), FileOwnership::Seed);
        assert_eq!(
            ownership("vendor/v4-core/src/PoolManager.sol"),
            FileOwnership::Generated
        );
        assert_eq!(ownership("AGENTS.md"), FileOwnership::Managed);
        assert_eq!(ownership("foundry.toml"), FileOwnership::Managed);
    }

    #[test]
    fn reports_file_restoration_failures() {
        let temporary = TemporaryDirectory::create("restore-test").unwrap();
        let blocking_file = temporary.path().join("not-a-directory");
        fs::write(&blocking_file, b"blocking file").unwrap();
        let restore_path = blocking_file.join("file.txt");
        let later_path = temporary.path().join("restored-after-failure.txt");
        let backups = BTreeMap::from([
            (restore_path, Some(b"original".to_vec())),
            (later_path.clone(), Some(b"also original".to_vec())),
        ]);

        let error = restore_files(&backups).unwrap_err();

        assert!(error.to_string().contains("restore directory"));
        assert!(format!("{error:#}").contains(&blocking_file.display().to_string()));
        assert_eq!(fs::read(later_path).unwrap(), b"also original");
    }

    #[test]
    fn embedded_scaffold_metadata_is_sealed() {
        let temporary = TemporaryDirectory::create("sealed-scaffold").unwrap();
        let root = temporary.path().join("project");
        fs::create_dir(&root).unwrap();
        SCAFFOLD.extract(&root).unwrap();
        let metadata = read_metadata(&root).unwrap();
        assert_eq!(metadata.created_with_cli, env!("CARGO_PKG_VERSION"));
        assert_eq!(metadata.last_updated_with_cli, env!("CARGO_PKG_VERSION"));
        let expected_lock = fs::read(root.join(LOCK_FILE)).unwrap();
        let expected_metadata = fs::read(root.join(METADATA_FILE)).unwrap();

        seal_scaffold(&root).unwrap();

        assert_eq!(fs::read(root.join(LOCK_FILE)).unwrap(), expected_lock);
        assert_eq!(
            fs::read(root.join(METADATA_FILE)).unwrap(),
            expected_metadata
        );
    }

    #[test]
    fn metadata_versions_are_semver() {
        let raw = r#"
schema-version = 1
created-with-cli = "0.1.0"
last-updated-with-cli = "0.1.0"

[template]
version = "1.0.0"
channel = "stable"
source = "Uniswap/v4-template"
revision = "abc"
manifest-digest = "sha256:abc"
"#;
        let metadata: ProjectMetadata = toml::from_str(raw).unwrap();
        assert_eq!(
            Version::parse(&metadata.template.version).unwrap(),
            Version::new(1, 0, 0)
        );
        assert_eq!(crate::util::sha256_bytes(b"abc").len(), 71);
    }
}
