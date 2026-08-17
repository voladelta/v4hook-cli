use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::{
    process::{command, require_success},
    scaffold::{
        LOCK_FILE, METADATA_FILE, ProjectMetadata, ProjectTemplateMetadata, TemplateLock,
        read_metadata, seal_scaffold,
    },
    util::status,
};

const PRESERVED_PATHS: [&str; 21] = [
    ".env.example",
    ".gas-snapshot",
    ".github/workflows/test.yml",
    ".gitignore",
    "AGENTS.md",
    "README.md",
    "foundry.toml",
    "remappings.txt",
    "script/00_DeployHook.s.sol",
    "script/base/BaseScript.sol",
    "test/Counter.t.sol",
    "test/utils/BaseTest.sol",
    "test/utils/libraries/EasyPosm.t.sol",
    "test/utils/v4hook-testkit/PROVENANCE.md",
    "test/utils/v4hook-testkit/V4Bindings.sol",
    "test/utils/v4hook-testkit/V4HookTestkit.sol",
    "test/utils/v4hook-testkit/artifacts/DeployHelper.sol",
    "test/utils/v4hook-testkit/artifacts/Permit2.sol",
    "test/utils/v4hook-testkit/artifacts/V4PoolManager.sol",
    "test/utils/v4hook-testkit/artifacts/V4PositionManager.sol",
    "v4hook.config.example.json",
];

const REMOVED_UPSTREAM_PATHS: [&str; 3] = [
    "script/03_Swap.s.sol",
    "script/testing/00_DeployV4.s.sol",
    "test/utils/Deployers.sol",
];

pub struct TemplateRefreshInput<'a> {
    pub repository: &'a Path,
    pub version: &'a str,
    pub source: &'a str,
    pub reference: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRefreshReport {
    pub template_version: String,
    pub source: String,
    pub reference: String,
    pub commit: String,
    pub dependencies: BTreeMap<String, String>,
    pub destination: String,
    pub preserved_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSealReport {
    pub template_version: String,
    pub destination: String,
    pub manifest_digest: String,
}

pub fn seal_template(repository: &Path) -> Result<TemplateSealReport> {
    let repository_root = fs::canonicalize(repository)
        .with_context(|| format!("resolve {}", repository.display()))?;
    if !repository_root.join("Cargo.toml").is_file()
        || !repository_root.join("src/main.rs").is_file()
    {
        bail!("template seal must run against the v4hook CLI repository")
    }
    let destination = repository_root.join("assets/v4-template");
    let (metadata, _) = seal_scaffold(&destination)?;
    Ok(TemplateSealReport {
        template_version: metadata.template.version,
        destination: destination.to_string_lossy().into_owned(),
        manifest_digest: metadata.template.manifest_digest,
    })
}

#[derive(Deserialize)]
struct CommitResponse {
    sha: String,
}

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    mode: String,
    sha: String,
}

fn github_client() -> Result<Client> {
    Client::builder()
        .user_agent(format!("v4hook/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_mins(2))
        .build()
        .context("build GitHub client")
}

fn github_json<T: serde::de::DeserializeOwned>(client: &Client, url: &str) -> Result<T> {
    client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .with_context(|| format!("request {url}"))?
        .error_for_status()
        .with_context(|| format!("GitHub rejected {url}"))?
        .json()
        .with_context(|| format!("parse GitHub response from {url}"))
}

fn download(client: &Client, url: &str) -> Result<Vec<u8>> {
    Ok(client
        .get(url)
        .send()
        .with_context(|| format!("download {url}"))?
        .error_for_status()
        .with_context(|| format!("download failed for {url}"))?
        .bytes()?
        .to_vec())
}

fn validate_source(source: &str) -> Result<(&str, &str)> {
    let (owner, repository) = source
        .split_once('/')
        .context("template source must use the owner/repository form")?;
    let valid = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    if !valid(owner) || !valid(repository) || repository.contains("..") {
        bail!("template source contains unsupported characters")
    }
    Ok((owner, repository))
}

fn validate_revision(revision: &str) -> Result<()> {
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("GitHub returned an invalid commit revision")
    }
    Ok(())
}

fn validate_reference(reference: &str) -> Result<()> {
    if reference.is_empty()
        || reference.len() > 200
        || reference.starts_with('/')
        || reference.ends_with('/')
        || reference.contains("..")
        || reference.contains("//")
        || !reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
    {
        bail!("template reference is not a supported Git branch, tag or commit")
    }
    Ok(())
}

fn extract_zip(bytes: &[u8], destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open GitHub archive")?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .context("read GitHub archive entry")?;
        let enclosed = file
            .enclosed_name()
            .context("GitHub archive contains an unsafe path")?;
        let mut components = enclosed.components();
        let Some(Component::Normal(_root)) = components.next() else {
            bail!("GitHub archive entry has no root directory")
        };
        let relative = components.collect::<PathBuf>();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let output = destination.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        fs::write(&output, bytes).with_context(|| format!("write {}", output.display()))?;
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)
        .with_context(|| format!("read {}", source.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        } else {
            bail!(
                "cannot copy unsupported file type: {}",
                source_path.display()
            )
        }
    }
    Ok(())
}

fn parse_gitmodules(path: &Path, source_owner: &str) -> Result<BTreeMap<String, String>> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut output = BTreeMap::new();
    let mut current_path: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            current_path = None;
        } else if let Some(value) = line.strip_prefix("path =") {
            current_path = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("url =") {
            let path = current_path
                .clone()
                .context("submodule URL appeared before its path")?;
            output.insert(path, normalize_github_source(value.trim(), source_owner)?);
        }
    }
    Ok(output)
}

fn repository_tree(client: &Client, source: &str, revision: &str) -> Result<TreeResponse> {
    let (owner, repository) = validate_source(source)?;
    let tree: TreeResponse = github_json(
        client,
        &format!(
            "https://api.github.com/repos/{owner}/{repository}/git/trees/{revision}?recursive=1"
        ),
    )?;
    if tree.truncated {
        bail!("GitHub truncated the tree for {source}; refusing an incomplete refresh")
    }
    Ok(tree)
}

fn resolve_dependencies(
    client: &Client,
    source: &str,
    revision: &str,
    repository_root: &Path,
    vendor_root: &Path,
    allowed: &BTreeSet<String>,
    dependencies: &mut BTreeMap<String, String>,
) -> Result<()> {
    let gitmodules_path = repository_root.join(".gitmodules");
    if !gitmodules_path.is_file() {
        return Ok(());
    }
    let (owner, _) = validate_source(source)?;
    let gitmodules = parse_gitmodules(&gitmodules_path, owner)?;
    let tree = repository_tree(client, source, revision)?;
    for entry in tree.tree.iter().filter(|entry| entry.mode == "160000") {
        validate_revision(&entry.sha)?;
        let dependency_source = gitmodules
            .get(&entry.path)
            .with_context(|| format!("missing .gitmodules entry for {}", entry.path))?;
        let name = Path::new(&entry.path)
            .file_name()
            .and_then(|value| value.to_str())
            .context("submodule path has no file name")?;
        if !allowed.contains(name) {
            continue;
        }
        if let Some(existing) = dependencies.get(name) {
            if existing != &entry.sha {
                eprintln!(
                    "Using pinned {name} revision {existing}; ignored nested revision {} from {source}.",
                    entry.sha
                );
            }
            continue;
        }
        dependencies.insert(name.to_owned(), entry.sha.clone());
        let dependency_destination = vendor_root.join(name);
        let bytes = download(
            client,
            &format!(
                "https://github.com/{dependency_source}/archive/{}.zip",
                entry.sha
            ),
        )?;
        extract_zip(&bytes, &dependency_destination)?;
        resolve_dependencies(
            client,
            dependency_source,
            &entry.sha,
            &dependency_destination,
            vendor_root,
            allowed,
            dependencies,
        )?;
        prune_dependency(name, &dependency_destination)?;
    }
    Ok(())
}

fn prune_dependency(name: &str, root: &Path) -> Result<()> {
    let kept: &[&str] = match name {
        "forge-std" => &["src", "LICENSE-APACHE", "LICENSE-MIT"],
        "openzeppelin-contracts" => &["contracts", "LICENSE"],
        "permit2" | "solmate" | "uniswap-hooks" | "v4-periphery" => &["src", "LICENSE"],
        "v4-core" => &["src", "test/utils", "licenses"],
        _ => bail!("no pruning policy exists for vendored dependency {name}"),
    };
    let parent = root
        .parent()
        .context("dependency has no parent directory")?;
    let prepared = parent.join(format!(".{name}-pruned"));
    remove_if_exists(&prepared)?;
    fs::create_dir(&prepared)?;
    for relative in kept {
        let source = root.join(relative);
        if !source.exists() {
            continue;
        }
        let destination = prepared.join(relative);
        if source.is_dir() {
            copy_tree(&source, &destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source, &destination)?;
        }
    }
    remove_if_exists(root)?;
    fs::rename(&prepared, root).with_context(|| format!("install pruned dependency {name}"))
}

fn managed_dependency_names(remappings: &Path) -> Result<BTreeSet<String>> {
    let raw =
        fs::read_to_string(remappings).with_context(|| format!("read {}", remappings.display()))?;
    let mut names = BTreeSet::new();
    for line in raw.lines() {
        let Some((_, target)) = line.split_once('=') else {
            continue;
        };
        let Some(rest) = target.trim().strip_prefix("vendor/") else {
            continue;
        };
        if let Some(name) = rest.split('/').next()
            && !name.is_empty()
        {
            names.insert(name.to_owned());
        }
    }
    if names.is_empty() {
        bail!("remappings.txt does not define any vendored dependencies")
    }
    Ok(names)
}

fn normalize_github_source(url: &str, source_owner: &str) -> Result<String> {
    let source = if let Some(value) = url.strip_prefix("https://github.com/") {
        value
    } else if let Some(value) = url.strip_prefix("http://github.com/") {
        value
    } else if let Some(value) = url.strip_prefix("git@github.com:") {
        value
    } else if let Some(value) = url.strip_prefix("../") {
        return Ok(format!("{source_owner}/{}", value.trim_end_matches(".git")));
    } else {
        bail!("unsupported submodule URL: {url}")
    };
    let source = source.trim_end_matches('/').trim_end_matches(".git");
    validate_source(source)?;
    Ok(source.to_owned())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
    } else if path.exists() {
        fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

fn preserve_v4hook_files(current: &Path, next: &Path) -> Result<()> {
    for relative in PRESERVED_PATHS {
        let source = current.join(relative);
        if !source.is_file() {
            bail!("required v4hook scaffold file is missing: {relative}")
        }
        let destination = next.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
    }
    for relative in REMOVED_UPSTREAM_PATHS {
        remove_if_exists(&next.join(relative))?;
    }
    Ok(())
}

fn validate_scaffold(root: &Path) -> Result<()> {
    for parts in [
        &["forge", "fmt", "--check"][..],
        &["forge", "build"][..],
        &["forge", "test"][..],
    ] {
        require_success(&command(parts), root, None, false)?;
    }
    Ok(())
}

fn require_newer_version(current: &str, next: &Version) -> Result<()> {
    let current =
        Version::parse(current).context("current template version is not valid SemVer")?;
    if next <= &current {
        bail!("template version {next} must be greater than current version {current}")
    }
    Ok(())
}

fn replace_directory(next: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("template has no parent directory")?;
    let unique = format!(
        "{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let prepared = parent.join(format!(".v4-template-next-{unique}"));
    let backup = parent.join(format!(".v4-template-backup-{unique}"));
    copy_tree(next, &prepared)?;
    fs::rename(destination, &backup).context("move current scaffold to a backup")?;
    if let Err(install_error) = fs::rename(&prepared, destination) {
        if let Err(restore_error) = fs::rename(&backup, destination) {
            return Err(restore_error).context(format!(
                "install refreshed scaffold failed ({install_error}); could not restore the previous scaffold from {}",
                backup.display()
            ));
        }
        if let Err(cleanup_error) = fs::remove_dir_all(&prepared) {
            return Err(cleanup_error).context(format!(
                "install refreshed scaffold failed ({install_error}); restored the previous scaffold but could not remove {}",
                prepared.display()
            ));
        }
        return Err(install_error)
            .context("install refreshed scaffold; restored the previous scaffold");
    }
    fs::remove_dir_all(&backup).with_context(|| {
        format!(
            "installed refreshed scaffold but could not remove backup {}",
            backup.display()
        )
    })
}

#[allow(clippy::too_many_lines)]
pub fn refresh_template(input: &TemplateRefreshInput<'_>) -> Result<TemplateRefreshReport> {
    let version = Version::parse(input.version).context("template version must use SemVer")?;
    validate_reference(input.reference)?;
    let repository_root = fs::canonicalize(input.repository)
        .with_context(|| format!("resolve {}", input.repository.display()))?;
    if !repository_root.join("Cargo.toml").is_file()
        || !repository_root.join("src/main.rs").is_file()
    {
        bail!("template refresh must run against the v4hook CLI repository")
    }
    let destination = repository_root.join("assets/v4-template");
    if !destination.is_dir() {
        bail!(
            "v4hook scaffold directory is missing: {}",
            destination.display()
        )
    }
    require_newer_version(&read_metadata(&destination)?.template.version, &version)?;
    let (owner, repository) = validate_source(input.source)?;
    status("Resolving the upstream template revision...");
    let client = github_client()?;
    let commit: CommitResponse = github_json(
        &client,
        &format!(
            "https://api.github.com/repos/{owner}/{repository}/commits/{}",
            input.reference
        ),
    )?;
    validate_revision(&commit.sha)?;
    let temporary = repository_root.join(format!(
        "assets/.v4-template-download-{}",
        std::process::id()
    ));
    remove_if_exists(&temporary)?;
    fs::create_dir_all(&temporary)?;
    let result: Result<BTreeMap<String, String>> = (|| {
        status("Downloading and preparing the upstream template...");
        let archive = download(
            &client,
            &format!(
                "https://github.com/{owner}/{repository}/archive/{}.zip",
                commit.sha
            ),
        )?;
        extract_zip(&archive, &temporary)?;
        remove_if_exists(&temporary.join("foundry.lock"))?;
        remove_if_exists(&temporary.join(".vscode"))?;
        fs::create_dir_all(temporary.join("vendor"))?;
        let allowed_dependencies = managed_dependency_names(&destination.join("remappings.txt"))?;
        let mut dependencies = BTreeMap::new();
        resolve_dependencies(
            &client,
            input.source,
            &commit.sha,
            &temporary,
            &temporary.join("vendor"),
            &allowed_dependencies,
            &mut dependencies,
        )?;
        let missing = allowed_dependencies
            .iter()
            .filter(|name| !dependencies.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            bail!(
                "could not resolve vendored dependencies: {}",
                missing.join(", ")
            )
        }
        remove_if_exists(&temporary.join("lib"))?;
        remove_if_exists(&temporary.join(".gitmodules"))?;
        preserve_v4hook_files(&destination, &temporary)?;
        require_success(&command(&["forge", "fmt"]), &temporary, None, false)?;

        let metadata = ProjectMetadata {
            schema_version: 1,
            created_with_cli: env!("CARGO_PKG_VERSION").to_owned(),
            last_updated_with_cli: env!("CARGO_PKG_VERSION").to_owned(),
            template: ProjectTemplateMetadata {
                version: version.to_string(),
                channel: "stable".to_owned(),
                source: input.source.to_owned(),
                revision: commit.sha.clone(),
                manifest_digest: String::new(),
            },
        };
        let mut metadata_text = toml::to_string_pretty(&metadata)?;
        metadata_text.push('\n');
        fs::write(temporary.join(METADATA_FILE), metadata_text)?;
        let lock = TemplateLock {
            schema_version: "v4hook.template-lock.v2".to_owned(),
            template_version: version.to_string(),
            snapshot: format!("v4hook-template-{version}"),
            repository: input.source.to_owned(),
            commit: commit.sha.clone(),
            dependencies: dependencies.clone(),
            files: BTreeMap::new(),
        };
        let mut lock_bytes = serde_json::to_vec_pretty(&lock)?;
        lock_bytes.push(b'\n');
        fs::write(temporary.join(LOCK_FILE), lock_bytes)?;
        seal_scaffold(&temporary)?;
        status("Validating the prepared template with Foundry...");
        validate_scaffold(&temporary)?;
        remove_if_exists(&temporary.join("out"))?;
        remove_if_exists(&temporary.join("cache"))?;
        replace_directory(&temporary, &destination)?;
        Ok(dependencies)
    })();
    let cleanup = remove_if_exists(&temporary);
    let dependencies = result?;
    cleanup?;
    Ok(TemplateRefreshReport {
        template_version: version.to_string(),
        source: input.source.to_owned(),
        reference: input.reference.to_owned(),
        commit: commit.sha,
        dependencies,
        destination: destination.to_string_lossy().into_owned(),
        preserved_paths: PRESERVED_PATHS.iter().map(ToString::to_string).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_refresh_requires_a_version_increase() {
        assert!(require_newer_version("1.2.3", &Version::parse("1.2.4").unwrap()).is_ok());
        assert!(require_newer_version("1.2.3", &Version::parse("1.2.3").unwrap()).is_err());
        assert!(require_newer_version("1.2.3", &Version::parse("1.1.0").unwrap()).is_err());
    }

    #[test]
    fn normalizes_supported_github_urls() {
        assert_eq!(
            normalize_github_source("https://github.com/Uniswap/v4-core.git", "Uniswap").unwrap(),
            "Uniswap/v4-core"
        );
        assert_eq!(
            normalize_github_source("git@github.com:foundry-rs/forge-std", "Uniswap").unwrap(),
            "foundry-rs/forge-std"
        );
        assert_eq!(
            normalize_github_source("../v4-periphery", "Uniswap").unwrap(),
            "Uniswap/v4-periphery"
        );
    }

    #[test]
    fn validates_safe_template_references() {
        validate_reference("release/v1.2.3").unwrap();
        assert!(validate_reference("main?recursive=1").is_err());
        assert!(validate_reference("../main").is_err());
    }
}
