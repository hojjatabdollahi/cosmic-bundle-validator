use std::io::Read;
use std::path::Path;
use std::process::Command;

use appstream::xmltree::{Element, XMLNode};

use crate::pin::Pin;

pub fn fetch(pin: &Pin, zip: &Path) -> Result<(), String> {
    let src = tempfile::tempdir().map_err(|err| err.to_string())?;
    let git = |args: &[&str]| -> Result<Vec<u8>, String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(src.path())
            .args(args)
            // Fail instead of asking for credentials when a repo is private or gone.
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|err| format!("git: {err}"))?;
        if !out.status.success() {
            return Err(format!(
                "git {}: {}",
                args[0],
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(out.stdout)
    };

    git(&["init", "-q"])?;
    // A named remote, so git-lfs knows where to download from.
    git(&["remote", "add", "origin", &pin.url])?;
    git(&["fetch", "-q", "--depth", "1", "origin", &pin.commit])?;

    let metainfo = format!("{}.metainfo.xml", pin.id);
    let meta = git(&["show", &format!("FETCH_HEAD:{metainfo}")])
        .map_err(|_| format!("the repo has no {metainfo} at its root"))?;
    let meta = Element::parse(meta.as_slice()).map_err(|err| format!("{metainfo}: {err}"))?;
    let id = meta
        .get_child("id")
        .and_then(|e| e.get_text())
        .unwrap_or_default();
    if id.trim() != pin.id {
        return Err(format!("{metainfo} names a different id: {id}"));
    }

    // Download LFS files before archiving, so git archive writes them instead of their pointers.
    // Without git-lfs this is skipped, and the pointer check below fails the build instead.
    if git(&["lfs", "ls-files", "--name-only", &pin.commit]).is_ok_and(|files| !files.is_empty()) {
        git(&["lfs", "install", "--local"])?;
        git(&["lfs", "fetch", "origin", &pin.commit])
            .map_err(|err| format!("could not download the bundle's Git LFS files: {err}"))?;
    }
    version(&meta).map_err(|err| format!("{metainfo}: {err}"))?;

    let names =
        String::from_utf8_lossy(&git(&["ls-tree", "--name-only", "FETCH_HEAD"])?).into_owned();
    let zip = std::path::absolute(zip).map_err(|err| err.to_string())?;
    let mut args = vec![
        "archive",
        "--format=zip",
        "-o",
        zip.to_str().ok_or("output path is not UTF-8")?,
        "FETCH_HEAD",
        &metainfo,
    ];
    args.extend(names.lines().filter(|name| is_packaged(name)));
    git(&args)?;
    no_lfs_pointers(&zip)
}

/// An LFS file that was never downloaded is zipped as a small text pointer. The importer only
/// copies files, so it would accept a wallpaper that is really a pointer.
fn no_lfs_pointers(zip: &Path) -> Result<(), String> {
    const POINTER: &[u8] = b"version https://git-lfs.github.com/spec/v1";
    let file = std::fs::File::open(zip).map_err(|err| format!("{}: {err}", zip.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|err| format!("{}: {err}", zip.display()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|err| err.to_string())?;
        let name = entry.name().map_err(|err| err.to_string())?.into_owned();
        let mut head = Vec::new();
        entry
            .by_ref()
            .take(POINTER.len() as u64)
            .read_to_end(&mut head)
            .map_err(|err| format!("{name}: {err}"))?;
        if head == POINTER {
            return Err(format!(
                "{name} is a Git LFS pointer, not the file; the LFS object could not be downloaded"
            ));
        }
    }
    Ok(())
}

pub fn version(meta: &Element) -> Result<String, String> {
    let version = meta
        .get_child("custom")
        .into_iter()
        .flat_map(|custom| &custom.children)
        .find_map(|node| match node {
            XMLNode::Element(e)
                if e.attributes
                    .get("key")
                    .is_some_and(|k| k == "x-cosmic-version") =>
            {
                e.get_text().map(|t| t.trim().to_owned())
            }
            _ => None,
        })
        .unwrap_or_default();
    if version.is_empty() {
        return Err(r#"needs <value key="x-cosmic-version">1.0.0</value> in <custom>"#.into());
    }
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || ".+~-_".contains(c))
    {
        return Err(format!(
            "x-cosmic-version may only use letters, digits and . + ~ - _: {version:?}"
        ));
    }
    Ok(version)
}

fn is_packaged(name: &str) -> bool {
    matches!(name, "config" | "data" | "LICENSE" | "LICENSES")
        || ["LICENSE.", "COPYING", "NOTICE"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}
