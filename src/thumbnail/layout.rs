use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use cosmic_bg_config::{Color as BgColor, Source};
use cosmic_config::ConfigGet;
use cosmic_theme::Theme;
use cosmic_theme::palette::Srgba;
use cosmic_theme_bundle_config::{Bundle, Part, Store};
use image::imageops::FilterType;

// sizes from cosmic-settings.
const W: f32 = 204.0;
const H: f32 = 138.0;
const BAND_H: f32 = 44.0;
const LINE_W: f32 = 80.0;
const LINE_H: f32 = 4.0;
const LINE_GAP: f32 = 4.0;
const PILL_H: f32 = 20.0;
const PILL_W: f32 = PILL_H * 2.0;
const GAP: f32 = 8.0;

pub const WIDTH: u32 = (W * 2.0) as u32;
pub const HEIGHT: u32 = (H * 2.0) as u32;

pub fn render(store: &Store, bundle: &Bundle, theme: &Theme, out: &Path) -> Result<(), String> {
    let entry: Option<cosmic_bg_config::Entry> = store
        .bundle_config(&bundle.slug, Part::Background)
        .ok()
        .and_then(|config| config.get("base").ok());
    let background = match entry.map(|entry| entry.source) {
        Some(Source::Path(path)) => {
            let file = wallpaper_file(&path)?;
            let image = image::open(&file).map_err(|err| format!("{}: {err}", file.display()))?;
            let mut jpeg = Vec::new();
            image
                .resize_to_fill(WIDTH, HEIGHT, FilterType::Lanczos3)
                .to_rgb8()
                .write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
                .map_err(|err| format!("{}: {err}", file.display()))?;
            let data = base64::engine::general_purpose::STANDARD.encode(jpeg);
            format!(r#"<image width="{W}" height="{H}" href="data:image/jpeg;base64,{data}"/>"#)
        }
        Some(Source::Color(BgColor::Single([r, g, b]))) => solid(Srgba::new(r, g, b, 1.0)),
        Some(Source::Color(BgColor::Gradient(g))) => {
            let [r, g, b] = g.colors.first().copied().unwrap_or_default();
            solid(Srgba::new(r, g, b, 1.0))
        }
        None => solid(theme.bg_color()),
    };

    // The lines
    let band_y = H - BAND_H;
    let x = (W - (LINE_W + GAP + PILL_W)) / 2.0;
    let y = band_y + (BAND_H - PILL_H) / 2.0;
    let line = |i: f32| {
        let y = y + i * (LINE_H + LINE_GAP);
        format!(
            r#"<rect x="{x}" y="{y}" width="{LINE_W}" height="{LINE_H}" rx="{}" fill="{}"/>"#,
            LINE_H / 2.0,
            hex(theme.on_bg_color())
        )
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {W} {H}">
<clipPath id="card"><rect width="{W}" height="{H}" rx="{radius}"/></clipPath>
<g clip-path="url(#card)">
{background}
<rect y="{band_y}" width="{W}" height="{BAND_H}" fill="{bg}"/>
{line0}
{line1}
{line2}
<rect x="{pill_x}" y="{y}" width="{PILL_W}" height="{PILL_H}" rx="{pill_r}" fill="{accent}"/>
</g>
</svg>
"#,
        radius = theme.corner_radii.radius_s[0],
        bg = hex(theme.bg_color()),
        line0 = line(0.0),
        line1 = line(1.0),
        line2 = line(2.0),
        pill_x = x + LINE_W + GAP,
        pill_r = PILL_H / 2.0,
        accent = hex(theme.accent_color()),
    );
    std::fs::write(out, svg).map_err(|err| format!("{}: {err}", out.display()))
}

fn solid(color: Srgba) -> String {
    format!(r#"<rect width="{W}" height="{H}" fill="{}"/>"#, hex(color))
}

fn hex(color: Srgba) -> String {
    let c = color.into_format::<u8, u8>();
    format!("#{:02x}{:02x}{:02x}{:02x}", c.red, c.green, c.blue, c.alpha)
}

fn wallpaper_file(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    //TODO: slide show?
    unimplemented!()
}
