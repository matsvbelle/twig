//! Per-branch editor background tint: a solid-colour PNG in `.idea/` plus the
//! project-scoped `idea.background.editor` property.
use crate::config::Tint;
use crate::error::Result;
use crate::{idea, out};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::BufWriter;
use std::path::Path;

/// One 8K tile covers any monitor with 'tile' fill, so there are no seams.
const WIDTH: u32 = 7680;
const HEIGHT: u32 = 4320;
pub const IMAGE_NAME: &str = "worktree-bg.png";

/// Hue from the branch name (sha1 mod 360).
pub fn branch_color(branch: &str, tint: &Tint) -> (u8, u8, u8) {
    let digest = Sha1::digest(branch.as_bytes());
    let hue = digest.iter().fold(0u32, |acc, b| (acc * 256 + *b as u32) % 360);
    hls_to_rgb(hue as f64 / 360.0, tint.lightness, tint.saturation)
}

/// HLS → RGB (same formula and rounding as Python's `colorsys`).
fn hls_to_rgb(h: f64, l: f64, s: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let m2 = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let m1 = 2.0 * l - m2;
    let v = |hue: f64| {
        let hue = hue.rem_euclid(1.0);
        let c = if hue < 1.0 / 6.0 {
            m1 + (m2 - m1) * hue * 6.0
        } else if hue < 0.5 {
            m2
        } else if hue < 2.0 / 3.0 {
            m1 + (m2 - m1) * (2.0 / 3.0 - hue) * 6.0
        } else {
            m1
        };
        (c * 255.0).round() as u8
    };
    (v(h + 1.0 / 3.0), v(h), v(h - 1.0 / 3.0))
}

pub fn write_image(path: &Path, (r, g, b): (u8, u8, u8)) -> Result<()> {
    let file = BufWriter::new(fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, WIDTH, HEIGHT);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Best);
    // Sub filter turns a solid row into one pixel + zeros: compresses best.
    enc.set_filter(png::FilterType::Sub);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    let row: Vec<u8> = [r, g, b].repeat(WIDTH as usize);
    let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;
    for _ in 0..HEIGHT {
        std::io::Write::write_all(&mut stream, &row)?;
    }
    stream.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Tint the worktree's editor/tool windows (not the IDE frame) at low opacity,
/// so panels keep their own theme shade while shifting toward the branch hue.
pub fn apply(wt: &Path, branch: &str, tint: &Tint) -> Result<()> {
    let idea_dir = wt.join(".idea");
    fs::create_dir_all(&idea_dir)?;
    let img = idea_dir.join(IMAGE_NAME);
    write_image(&img, branch_color(branch, tint))?;
    let spec = format!("{},{},tile,top_left,none", img.display(), tint.opacity);
    let ws = idea_dir.join("workspace.xml");
    idea::ensure_workspace(&ws)?;
    let xml = fs::read_to_string(&ws)?;
    let updated = idea::set_properties(&xml, &[("idea.background.editor", &spec)], &["idea.background.frame"])?;
    fs::write(&ws, updated)?;
    out::say(format!("Tinted worktree editor background ({}% opacity)", tint.opacity));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_match_reference() {
        let t = Tint::default();
        assert_eq!(branch_color("feature/login-form", &t), (77, 104, 203));
        assert_eq!(branch_color("hotfix/crash-on-start", &t), (203, 77, 201));
        assert_eq!(branch_color("main", &t), (203, 77, 92));
        let grey = Tint { saturation: 0.0, lightness: 0.5, opacity: 7 };
        assert_eq!(branch_color("anything", &grey), (128, 128, 128));
    }

    #[test]
    fn image_is_valid_small_and_solid() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("bg.png");
        write_image(&p, (203, 77, 123)).unwrap();
        assert!(fs::metadata(&p).unwrap().len() < 150_000, "png too large");
        let dec = png::Decoder::new(fs::File::open(&p).unwrap());
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (WIDTH, HEIGHT));
        assert_eq!(&buf[..3], &[203, 77, 123]);
        assert_eq!(&buf[buf.len() - 3..], &[203, 77, 123]);
    }

    #[test]
    fn apply_writes_property() {
        let tmp = tempfile::tempdir().unwrap();
        apply(tmp.path(), "main", &Tint::default()).unwrap();
        let ws = fs::read_to_string(tmp.path().join(".idea/workspace.xml")).unwrap();
        assert!(ws.contains("idea.background.editor"));
        assert!(ws.contains(",7,tile,top_left,none"));
    }
}
