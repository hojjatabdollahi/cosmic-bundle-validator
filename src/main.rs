use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod catalog;
mod import;
mod pin;
mod source;

const USAGE: &str = "usage:
  cosmic-bundle-validator validate <bundle.zip>
  cosmic-bundle-validator build <bundle/ID/ID.xml> [--out <dir>]
  cosmic-bundle-validator catalog <bundle-dir> <out-dir>";

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let args: Vec<&Path> = args.iter().map(PathBuf::as_path).collect();
    import::isolate_home();
    let result = match args.as_slice() {
        [cmd, zip] if *cmd == Path::new("validate") => validate(zip),
        [cmd, pin] if *cmd == Path::new("build") => build(pin, Path::new("out")),
        [cmd, pin, flag, out] if *cmd == Path::new("build") && *flag == Path::new("--out") => {
            build(pin, out)
        }
        [cmd, bundles, out] if *cmd == Path::new("catalog") => catalog::write(bundles, out),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("invalid bundle: {err}");
            ExitCode::FAILURE
        }
    }
}

fn validate(zip: &Path) -> Result<(), String> {
    import::import(zip)?.print();
    Ok(())
}

fn build(pin: &Path, out: &Path) -> Result<(), String> {
    let pin = pin::Pin::read(pin)?;
    println!("pin        {} @ {} {}", pin.url, pin.commit, pin.id);
    std::fs::create_dir_all(out).map_err(|err| format!("{}: {err}", out.display()))?;
    let zip = out.join(format!("{}.zip", pin.id));
    let result = source::fetch(&pin, &zip).and_then(|()| {
        println!("zip        {}", zip.display());
        validate(&zip)
    });
    // Don't leave a partial or invalid zip around
    if result.is_err() {
        let _ = std::fs::remove_file(&zip);
    }
    result
}
