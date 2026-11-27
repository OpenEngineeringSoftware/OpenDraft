//! Pictures of blocks, drawn by the canvas's CPU rasteriser and kept as
//! textures until the block, the library or the drawing changes. A few are
//! made each frame so opening a big library does not stall the window.

use std::collections::HashMap;

use eframe::egui;
use opendraft_canvas::preview::Preview;
use opendraft_content::blocks_payload::BlockUuid;
use opendraft_core::{BlockId, Document};

use super::Env;

/// Pictures made in one frame.
const PER_FRAME: usize = 4;

/// A picture, or `None` for a block that has nothing to draw.
type Slot = Option<egui::TextureHandle>;

#[derive(Default)]
pub struct Thumbnails {
    library: HashMap<BlockUuid, Slot>,
    drawing: HashMap<String, Slot>,
    library_stamp: u64,
    drawing_revision: u64,
    made: usize,
}

impl Thumbnails {
    /// Lets go of pictures that no longer match the library or the drawing.
    pub fn keep_current(&mut self, library_stamp: u64, drawing_revision: u64) {
        if self.library_stamp != library_stamp {
            self.library.clear();
            self.library_stamp = library_stamp;
        }
        if self.drawing_revision != drawing_revision {
            self.drawing.clear();
            self.drawing_revision = drawing_revision;
        }
        self.made = 0;
    }

    /// Follows a block of the drawing renamed from `from` to `to`, which
    /// took the drawing from revision `before` to `after`. No picture shows a
    /// block's name, so every picture still holds: the renamed one is kept
    /// under its new name instead of all being made again.
    pub fn drawing_block_renamed(&mut self, from: &str, to: &str, (before, after): (u64, u64)) {
        if self.drawing_revision != before {
            return;
        }
        if let Some(slot) = self.drawing.remove(from) {
            self.drawing.insert(to.to_owned(), slot);
        }
        self.drawing_revision = after;
    }

    /// The picture of a library block. `Err(())` while it has not been made
    /// yet this frame; `Ok(None)` when there is nothing to draw.
    pub fn library_block(
        &mut self,
        ctx: &egui::Context,
        env: &Env,
        id: BlockUuid,
    ) -> Result<Option<&egui::TextureHandle>, ()> {
        if !self.library.contains_key(&id) {
            if self.made >= PER_FRAME {
                ctx.request_repaint();
                return Err(());
            }
            self.made += 1;
            let picture = env
                .library
                .with(|lib| {
                    let (doc, block) = lib.source(id)?;
                    picture_of(doc, block)
                })
                .ok()
                .flatten();
            self.library.insert(id, upload(ctx, "cm_lib", picture));
        }
        Ok(self.library.get(&id).and_then(Option::as_ref))
    }

    /// As `library_block`, for a block of the open drawing.
    pub fn drawing_block(
        &mut self,
        ctx: &egui::Context,
        env: &Env,
        name: &str,
        id: BlockId,
    ) -> Result<Option<&egui::TextureHandle>, ()> {
        if !self.drawing.contains_key(name) {
            if self.made >= PER_FRAME {
                ctx.request_repaint();
                return Err(());
            }
            self.made += 1;
            let picture = picture_of(env.document, id);
            self.drawing
                .insert(name.to_owned(), upload(ctx, "cm_drawing", picture));
        }
        Ok(self.drawing.get(name).and_then(Option::as_ref))
    }
}

/// What the block draws, in the dark picture the drawing area has.
fn picture_of(doc: &Document, block: BlockId) -> Option<Preview> {
    crate::purge_cmd::block_picture(doc, block)
}

fn upload(ctx: &egui::Context, name: &str, picture: Option<Preview>) -> Slot {
    // Shrunk here so a tile (about 112 points wide) is not made by the GPU
    // squeezing a 512 pixel picture, which drops thin lines out of it.
    let by = if ctx.pixels_per_point() >= 1.5 { 2 } else { 4 };
    let p = shrink(&picture?, by);
    let image =
        egui::ColorImage::from_rgba_unmultiplied([p.width as usize, p.height as usize], &p.rgba);
    Some(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
}

/// How far a pixel is from the picture's background.
fn contrast(px: &[u8]) -> u32 {
    let bg = opendraft_canvas::preview::BACKGROUND;
    (0..3).map(|i| u32::from(px[i].abs_diff(bg[i]))).sum()
}

/// `p` at `1/by` the size. Each new pixel is half the average of the `by` x
/// `by` pixels it covers and half the one that differs most from the
/// background, so the picture's one-pixel lines stay lines, not faint dots.
fn shrink(p: &Preview, by: usize) -> Preview {
    let (w, h) = (p.width as usize / by, p.height as usize / by);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            let mut far = (0u32, [0u8; 3]);
            for dy in 0..by {
                for dx in 0..by {
                    let at = ((y * by + dy) * p.width as usize + x * by + dx) * 4;
                    let px = &p.rgba[at..at + 3];
                    for (total, c) in sum.iter_mut().zip(px) {
                        *total += u32::from(*c);
                    }
                    let c = contrast(px);
                    if c >= far.0 {
                        far = (c, [px[0], px[1], px[2]]);
                    }
                }
            }
            let n = (by * by) as u32;
            for (total, far) in sum.iter().zip(far.1) {
                rgba.push(((total / n + u32::from(far)) / 2) as u8);
            }
            rgba.push(255);
        }
    }
    Preview {
        width: w as u32,
        height: h as u32,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_pixel_line_is_still_clearly_there_after_shrinking() {
        let bg = opendraft_canvas::preview::BACKGROUND;
        let (w, h) = (16usize, 16usize);
        let mut rgba = Vec::new();
        for y in 0..h {
            for _ in 0..w {
                let on = y == 8;
                let c = if on { 255 } else { bg[0] };
                rgba.extend([c, c, c, 255]);
            }
        }
        let small = shrink(
            &Preview {
                width: w as u32,
                height: h as u32,
                rgba,
            },
            4,
        );
        assert_eq!((small.width, small.height), (4, 4));
        // Row 2 holds the line: well above the background and the plain average.
        let on = small.rgba[(2 * 4) * 4];
        let off = small.rgba[0];
        assert_eq!(off, bg[0]);
        assert!(on > 140, "line faded to {on}");
    }

    fn a_picture(ctx: &egui::Context) -> Slot {
        let rgba = vec![200; 8 * 8 * 4];
        let picture = Preview {
            width: 8,
            height: 8,
            rgba,
        };
        upload(ctx, "test", Some(picture))
    }

    #[test]
    fn renaming_a_drawing_block_keeps_every_picture_instead_of_making_them_again() {
        let ctx = egui::Context::default();
        let mut thumbs = Thumbnails::default();
        thumbs.keep_current(0, 5);
        thumbs.drawing.insert("Box".into(), a_picture(&ctx));
        thumbs.drawing.insert("Other".into(), a_picture(&ctx));

        thumbs.drawing_block_renamed("Box", "Crate", (5, 6));
        thumbs.keep_current(0, 6);
        let mut kept: Vec<&str> = thumbs.drawing.keys().map(String::as_str).collect();
        kept.sort_unstable();
        assert_eq!(kept, ["Crate", "Other"]);

        // Pictures made before another change are let go, as for any change.
        thumbs.drawing_block_renamed("Crate", "Tile", (5, 7));
        thumbs.keep_current(0, 7);
        assert!(thumbs.drawing.is_empty());
    }
}
