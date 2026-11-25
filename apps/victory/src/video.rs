//! The title's own saved video settings: its output resolution lives in
//! `<data>/<title id>/storage/settings/settings.xml`, and the title keeps
//! rendering for what is written there. The display mode the layer reports
//! (`victory_gpu::dxgi`, our `[graphics] display_mode`) only reaches a
//! settings file that is made new: a file written by an earlier run wins,
//! which is why a 4K file kept the title at 3840x2160 while the reported
//! mode was 640x480.
//!
//! So a run writes the configured size into that file before the title
//! starts (the file is outside the repository, in the title's data). Only
//! the two numbers of `<resolutionFullscreen>` are touched; the rest of the
//! file keeps its bytes. `VICTORY_DISPLAY_SETTINGS=0` leaves the file as it
//! is (the title's own display settings menu then decides, fed by the
//! reported modes).

use std::path::Path;

use anyhow::{Context, Result};

/// What a run did to the settings file.
#[derive(Debug)]
pub enum Applied {
    /// No settings file yet: the title will make one from the reported mode.
    NoFile,
    /// The file holds another shape (no `<resolutionFullscreen>`): not
    /// touched, and why is in the log.
    NotFound,
    /// Already the configured size.
    Same,
    /// Written: the size it held before, if it held one.
    Changed { was: Option<(u32, u32)> },
    /// `VICTORY_DISPLAY_SETTINGS=0`: the file was left as it is.
    Disabled,
}

/// Whether a run writes the configured resolution into the title's saved
/// video settings (`VICTORY_DISPLAY_SETTINGS=0` leaves the file alone).
fn on() -> bool {
    std::env::var_os("VICTORY_DISPLAY_SETTINGS").is_none_or(|v| v != "0")
}

/// Puts `size` into the title's saved video settings under `storage` (the
/// title id's storage folder), so the title renders at it. `display` is
/// the size the layer also reports to the title.
pub fn apply_display(storage: &Path, display: (u32, u32)) -> Result<Applied> {
    if !on() {
        return Ok(Applied::Disabled);
    }
    let file = storage.join("settings").join("settings.xml");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Ok(Applied::NoFile);
    };
    match patch_resolution(&text, display) {
        Patch::NotFound => Ok(Applied::NotFound),
        Patch::Same => Ok(Applied::Same),
        Patch::Changed { was, text } => {
            std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
            Ok(Applied::Changed { was })
        }
    }
}

/// What [`patch_resolution`] found and made.
#[derive(Debug, PartialEq, Eq)]
enum Patch {
    NotFound,
    Same,
    Changed { was: Option<(u32, u32)>, text: String },
}

/// `xml` with the numbers of `<resolutionFullscreen>` as `size`.
fn patch_resolution(xml: &str, size: (u32, u32)) -> Patch {
    let Some(open) = xml.find("<resolutionFullscreen>") else {
        return Patch::NotFound;
    };
    let block_start = open + "<resolutionFullscreen>".len();
    let Some(close) = xml[block_start..].find("</resolutionFullscreen>") else {
        return Patch::NotFound;
    };
    let block = &xml[block_start..block_start + close];
    let Some((was_width, width_at)) = number_after(block, "<width") else {
        return Patch::NotFound;
    };
    let Some((was_height, height_at)) = number_after(block, "<height") else {
        return Patch::NotFound;
    };
    let was = Some((was_width, was_height));
    if was == Some(size) {
        return Patch::Same;
    }
    // The later edit first, so the earlier one's offsets still hold.
    let (first, second) = if width_at > height_at {
        ((width_at, was_width, size.0), (height_at, was_height, size.1))
    } else {
        ((height_at, was_height, size.1), (width_at, was_width, size.0))
    };
    let mut text = xml.to_owned();
    for (at, was, now) in [first, second] {
        let start = block_start + at;
        text.replace_range(start..start + was.to_string().len(), &now.to_string());
    }
    Patch::Changed { was, text }
}

/// The number in `value="N"` of the element starting with `element` in
/// `block`, and where the digits are in `block`.
fn number_after(block: &str, element: &str) -> Option<(u32, usize)> {
    let element_at = block.find(element)?;
    let rest = &block[element_at..];
    let marker = "value=\"";
    let marker_at = rest.find(marker)? + marker.len();
    let digits: String = rest[marker_at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let value = digits.parse().ok()?;
    Some((value, element_at + marker_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape the title writes (only the parts the patch looks at).
    fn settings(width: u32, height: u32) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <rage__fwSystemSettingsCollection>\n\
             \x20 <video>\n\
             \x20   <refreshRate>\n\
             \x20     <numerator value=\"60\" />\n\
             \x20   </refreshRate>\n\
             \x20   <resolutionFullscreen>\n\
             \x20     <width value=\"{width}\" />\n\
             \x20     <height value=\"{height}\" />\n\
             \x20   </resolutionFullscreen>\n\
             \x20   <frameRateLimit value=\"30\" />\n\
             \x20 </video>\n\
             \x20 <waterReflectionResX value=\"512\" />\n\
             </rage__fwSystemSettingsCollection>\n"
        )
    }

    #[test]
    fn the_saved_resolution_is_replaced_and_nothing_else() {
        let before = settings(3840, 2160);
        let Patch::Changed { was, text } = patch_resolution(&before, (640, 480)) else {
            panic!("the size should change");
        };
        assert_eq!(was, Some((3840, 2160)));
        assert_eq!(text, settings(640, 480));
        // The refresh rate and every other number are as they were.
        assert!(text.contains("<numerator value=\"60\" />"));
        assert!(text.contains("<waterReflectionResX value=\"512\" />"));
    }

    #[test]
    fn a_file_at_the_configured_size_is_left_alone() {
        assert_eq!(patch_resolution(&settings(640, 480), (640, 480)), Patch::Same);
    }

    #[test]
    fn a_file_without_the_resolution_is_not_touched() {
        let other = "<root><video><frameRateLimit value=\"30\" /></video></root>";
        assert_eq!(patch_resolution(other, (640, 480)), Patch::NotFound);
        let half = "<root><resolutionFullscreen><width value=\"64\" /></root>";
        assert_eq!(patch_resolution(half, (640, 480)), Patch::NotFound);
    }

    #[test]
    fn sizes_with_fewer_digits_are_replaced_whole() {
        let Patch::Changed { text, .. } = patch_resolution(&settings(640, 480), (8, 6)) else {
            panic!("the size should change");
        };
        assert!(text.contains("<width value=\"8\" />"));
        assert!(text.contains("<height value=\"6\" />"));
        assert!(!text.contains("640"));
    }
}
