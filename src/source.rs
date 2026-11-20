use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use appstream::xmltree::{Element, XMLNode};
use zip::write::SimpleFileOptions;

use crate::pin::Pin;

pub fn fetch(pin: &Pin, zip: &Path) -> Result<(), String> {
    let src = tempfile::tempdir().map_err(|err| err.to_string())?;
    let git = |args: &[&str]| -> Result<String, String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(src.path())
            .args(args)
            // Fail instead of asking for credentials when a repo is private or gone.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("TZ", "UTC")
            .output()
            .map_err(|err| format!("git: {err}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("git {}: {}", args[0], err.trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };

    let folder = pin
        .path
        .as_ref()
        .map(|path| format!("{path}/"))
        .unwrap_or_default();
    let metainfo = format!("{}.metainfo.xml", pin.id);
    let wanted = [
        format!("/{folder}{metainfo}"),
        format!("/{folder}config/"),
        format!("/{folder}data/"),
    ];
    git(&["init", "-q"])?;
    git(&["remote", "add", "origin", &pin.url])?;
    git(&[
        "sparse-checkout",
        "set",
        "--no-cone",
        &wanted[0],
        &wanted[1],
        &wanted[2],
    ])?;
    git(&[
        "fetch",
        "-q",
        "--depth=1",
        "--filter=blob:none",
        "origin",
        &pin.commit,
    ])?;
    git(&["checkout", "-q", "FETCH_HEAD"])?;
    let bundle = src.path().join(&folder);

    let meta = std::fs::read(bundle.join(&metainfo))
        .map_err(|_| format!("the repo has no {folder}{metainfo}"))?;
    let meta = Element::parse(meta.as_slice()).map_err(|err| format!("{metainfo}: {err}"))?;
    let id = meta
        .get_child("id")
        .and_then(|e| e.get_text())
        .unwrap_or_default();
    if id.trim() != pin.id {
        return Err(format!("{metainfo} names a different id: {id}"));
    }
    version(&meta).map_err(|err| format!("{metainfo}: {err}"))?;

    let time = git(&[
        "log",
        "-1",
        "--format=%cd",
        "--date=format-local:%Y %m %d %H %M %S",
    ])?;
    let time: Vec<u16> = time
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [y, mo, d, h, mi, s] = time[..] else {
        return Err(format!("unreadable commit time: {time:?}"));
    };
    let time = zip::DateTime::from_date_and_time(y, mo as u8, d as u8, h as u8, mi as u8, s as u8)
        .map_err(|err| format!("commit time: {err}"))?;
    zip_folder(
        &bundle,
        zip,
        SimpleFileOptions::default().last_modified_time(time),
    )
}

fn zip_folder(dir: &Path, zip: &Path, options: SimpleFileOptions) -> Result<(), String> {
    const LFS_POINTER: &[u8] = b"version https://git-lfs.github.com/spec/v1";
    let error = |err: &dyn std::fmt::Display| format!("{}: {err}", zip.display());

    let mut writer = zip::ZipWriter::new(File::create(zip).map_err(|e| error(&e))?);
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|e| error(&e))?;
        let name = entry.path().strip_prefix(dir).map_err(|e| error(&e))?;
        let name = name.to_str().ok_or("a file name is not UTF-8")?;
        if name.is_empty() || name == ".git" || name.starts_with(".git/") {
            continue;
        }
        if entry.path_is_symlink() {
            return Err(format!(
                "{name} is a symlink, which the importer does not accept"
            ));
        }
        if entry.file_type().is_dir() {
            writer.add_directory(name, options).map_err(|e| error(&e))?;
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|e| error(&e))?;
        if bytes.starts_with(LFS_POINTER) {
            return Err(format!(
                "{name} is a Git LFS pointer, not the file; the LFS object could not be downloaded"
            ));
        }
        writer.start_file(name, options).map_err(|e| error(&e))?;
        writer.write_all(&bytes).map_err(|e| error(&e))?;
    }
    writer.finish().map_err(|e| error(&e))?;
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
