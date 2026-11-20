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
    git(&["fetch", "-q", "--depth", "1", &pin.url, &pin.commit])?;

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
