use alloc::vec;
use alloc::vec::Vec;

use fontdue::{FontHasherBuilder, FontSettings};
use hashbrown::hash_map::Entry;
use hashbrown::HashMap;
use smallmap::Map;
use tiny_std::io::Read;
use xcb_rust_protocol::proto::render::{Glyphinfo, Glyphset};

use pgwm_core::colors::Color;
use pgwm_core::config::{
    FontCfg, CHAR_REMAP, CHAR_REMAP_FONTS, SHORTCUT_SECTION, TAB_BAR_SECTION,
    WINDOW_NAME_DISPLAY_SECTION, WORKSPACE_SECTION_FONTS,
};
use pgwm_core::geometry::Dimensions;
use pgwm_core::render::{DoubleBufferedRenderPicture, RenderVisualInfo};

use crate::error::{Error, Result};
use crate::x11::call_wrapper::CallWrapper;

pub(crate) struct FontDrawer<'a> {
    loaded_render_fonts: &'a LoadedFonts<'a>,
}

impl<'a> FontDrawer<'a> {
    pub(crate) fn new(loaded_xrender_fonts: &'a LoadedFonts<'a>) -> Self {
        Self {
            loaded_render_fonts: loaded_xrender_fonts,
        }
    }

    pub(crate) fn text_geometry(&self, text: &str, fonts: &[FontCfg]) -> (i16, u16) {
        self.loaded_render_fonts.geometry(text, fonts)
    }

    pub(crate) fn draw(
        &self,
        call_wrapper: &mut CallWrapper,
        dbw: &DoubleBufferedRenderPicture,
        text: &str,
        fonts: &[FontCfg],
        fill_area: Dimensions,
        text_width: i16,
        text_x: i16,
        text_y: i16,
        bg: Color,
        text_color: Color,
    ) -> Result<i16> {
        let encoded = self
            .loaded_render_fonts
            .encode(text, fonts, text_width - text_x);
        call_wrapper.fill_xrender_rectangle(
            dbw.pixmap.picture,
            text_color.as_render_color(),
            Dimensions::new(1, 1, 0, 0),
        )?;
        call_wrapper.fill_xrender_rectangle(dbw.window.picture, bg.as_render_color(), fill_area)?;
        let mut offset = fill_area.x + text_x;
        let mut drawn_width = 0;
        for chunk in encoded {
            drawn_width += chunk.width;
            let box_shift = (fill_area.height - chunk.font_height) / 2;

            call_wrapper.draw_glyphs(
                offset,
                fill_area.y + text_y + box_shift,
                chunk.glyph_set,
                dbw,
                &chunk.glyph_ids,
            )?;

            offset += chunk.width;
        }

        Ok(drawn_width)
    }
}

pub(crate) fn load_alloc_fonts<'a>(
    call_wrapper: &mut CallWrapper,
    vis_info: &RenderVisualInfo,
) -> Result<HashMap<&'a FontCfg<'a>, LoadedFont, FontHasherBuilder>> {
    let mut map = HashMap::with_hasher(FontHasherBuilder);
    let it = WORKSPACE_SECTION_FONTS
        .iter()
        .chain(WINDOW_NAME_DISPLAY_SECTION.iter())
        .chain(SHORTCUT_SECTION.iter())
        .chain(TAB_BAR_SECTION.iter())
        .chain(CHAR_REMAP_FONTS.into_iter());
    #[cfg(feature = "status-bar")]
    let it = it.chain(pgwm_core::config::STATUS_SECTION.iter());
    // Reuse buffer
    let mut data = Vec::with_capacity(65536);
    for f_cfg in it {
        // Ugly and kind of dumb
        let mut id = 0;
        if let Entry::Vacant(v) = map.entry(f_cfg) {
            let mut file = tiny_std::fs::OpenOptions::new()
                .read(true)
                .open(f_cfg.path)?;
            data.clear();
            let read_bytes = file.read_to_end(&mut data)?;
            crate::debug!("Read {} bytes of font {}", read_bytes, f_cfg.path);
            let gs = call_wrapper.create_glyphset(vis_info)?;

            let mut ids = vec![];
            let mut infos = vec![];
            let mut raw_data = vec![];
            let mut char_map = HashMap::with_hasher(FontHasherBuilder);
            let size = f_cfg.size.parse::<f32>().map_err(|_| Error::ParseFloat)?;
            let raster_iter =
                fontdue::RasterIterator::new(&data[..read_bytes], size, FontSettings::default())
                    .map_err(|_e| {
                        crate::debug!("Font load failed {_e}");
                        Error::FontLoad("Failed to load font")
                    })?;
            crate::debug!("Loaded font at {}", f_cfg.path);
            let mut data = vec![];
            let mut max_height = 0;
            // DlMalloc seems to keep our dropped vec on the heap after use, really annoying
            for rasterized_char in raster_iter {
                let height =
                    rasterized_char.metrics.height as i16 + rasterized_char.metrics.ymin as i16;
                if height > max_height {
                    max_height = height;
                }
                data.push((
                    rasterized_char.ch,
                    rasterized_char.metrics,
                    rasterized_char.buf,
                ));
            }
            for (ch, metrics, buf) in data {
                for byte in buf {
                    raw_data.extend_from_slice(&[byte, byte, byte, byte]);
                }
                // When placing chars next to each other this is the appropriate width to use
                let horizontal_space = metrics.advance_width as i16;
                let glyph_info = Glyphinfo {
                    width: metrics.width as u16,
                    height: metrics.height as u16,
                    x: -metrics.xmin as i16,
                    y: metrics.height as i16 - max_height + metrics.ymin as i16, // pt2
                    x_off: horizontal_space,
                    y_off: metrics.advance_height as i16,
                };
                ids.push(id as u32);
                infos.push(glyph_info);
                char_map.insert(
                    ch,
                    CharInfo {
                        glyph_id: id,
                        horizontal_space,
                        height: metrics.height as u16,
                    },
                );
                let current_out_size = current_out_size(ids.len(), infos.len(), raw_data.len());

                if current_out_size >= 32768 {
                    call_wrapper.add_glyphs(gs, &ids, &infos, &raw_data)?;
                    // Have to flush here or we'll blow out the buffer
                    call_wrapper.uring.await_write_completions()?;
                    ids.clear();
                    infos.clear();
                    raw_data.clear();
                }
                id += 1;
            }
            call_wrapper.add_glyphs(gs, &ids, &infos, &raw_data)?;
            crate::debug!("Added {} glyphs", ids.len());
            crate::debug!(
                "Storing loaded font with size > {} bytes",
                calculate_font_size(char_map.len())
            );
            v.insert(LoadedFont {
                glyph_set: gs,
                char_map,
                font_height: max_height,
            });
        }
    }
    Ok(map)
}

#[inline]
fn current_out_size(ids_len: usize, infos_len: usize, raw_data_len: usize) -> usize {
    core::mem::size_of::<u32>()
        + core::mem::size_of::<u32>() * ids_len
        + core::mem::size_of::<Glyphinfo>() * infos_len
        + core::mem::size_of::<u8>() * raw_data_len
}

#[cfg(feature = "debug")]
#[inline]
fn calculate_font_size(map_len: usize) -> usize {
    core::mem::size_of::<u32>()
        + core::mem::size_of::<i16>()
        + (core::mem::size_of::<char>() + core::mem::size_of::<CharInfo>()) * map_len
}

pub struct LoadedFonts<'a> {
    pub(crate) fonts: HashMap<&'a FontCfg<'a>, LoadedFont, FontHasherBuilder>,
    // Simple key, use smallmap
    chars: Map<char, LoadedChar>,
}

struct LoadedChar {
    gsid: Glyphset,
    char_info: CharInfo,
    font_height: i16,
}

impl<'a> LoadedFonts<'a> {
    pub(crate) fn new(
        fonts: HashMap<&'a FontCfg<'a>, LoadedFont, FontHasherBuilder>,
    ) -> Result<Self> {
        let mut chars = Map::new();
        for (char, font) in CHAR_REMAP {
            let char = *char;
            match fonts.get(font) {
                Some(loaded) => match loaded.char_map.get(&char) {
                    Some(char_info) => {
                        chars.insert(
                            char,
                            LoadedChar {
                                gsid: loaded.glyph_set,
                                char_info: *char_info,
                                font_height: loaded.font_height,
                            },
                        );
                    }
                    None => return Err(Error::FontLoad("Char not in specified font")),
                },
                None => return Err(Error::FontLoad("Font not loaded when expected")),
            }
        }
        Ok(Self { fonts, chars })
    }

    #[must_use]
    pub fn encode(&self, text: &str, fonts: &[FontCfg], max_width: i16) -> Vec<FontEncodedChunk> {
        let mut total_width = 0;
        let mut total_glyphs = 0;
        let mut cur_width = 0;
        let mut cur_gs = None;
        let mut cur_glyphs = vec![];
        let mut chunks = vec![];
        let mut cur_font_height = 0;
        for char in text.chars() {
            total_glyphs += 1;
            if let Some(lchar) = self.chars.get(&char) {
                if !cur_glyphs.is_empty() {
                    chunks.push(FontEncodedChunk {
                        width: core::mem::take(&mut cur_width),
                        font_height: core::mem::take(&mut cur_font_height),
                        glyph_set: cur_gs.unwrap(),
                        glyph_ids: core::mem::take(&mut cur_glyphs),
                    });
                }
                // Early return if next char would go OOB
                if total_width + lchar.char_info.horizontal_space > max_width {
                    if !cur_glyphs.is_empty() {
                        chunks.push(FontEncodedChunk {
                            width: cur_width,
                            font_height: cur_font_height,
                            glyph_set: cur_gs.unwrap(),
                            glyph_ids: cur_glyphs,
                        });
                    }
                    return chunks;
                }
                total_width += lchar.char_info.horizontal_space;
                chunks.push(FontEncodedChunk {
                    width: lchar.char_info.horizontal_space,
                    font_height: lchar.font_height,
                    glyph_set: lchar.gsid,
                    glyph_ids: vec![lchar.char_info.glyph_id],
                });
            } else {
                for font in fonts {
                    if let Some((gs, info, mh)) = self.fonts.get(font).and_then(|loaded| {
                        loaded
                            .char_map
                            .get(&char)
                            .map(|info| (loaded.glyph_set, info, loaded.font_height))
                    }) {
                        if cur_gs.is_none() {
                            cur_gs = Some(gs);
                        }
                        if gs != cur_gs.unwrap() {
                            chunks.push(FontEncodedChunk {
                                width: core::mem::take(&mut cur_width),
                                font_height: mh,
                                glyph_set: cur_gs.unwrap(),
                                glyph_ids: core::mem::take(&mut cur_glyphs),
                            });
                            cur_gs = Some(gs);
                            cur_width = 0;
                        }
                        // Early return if next char would go OOB
                        if total_width + info.horizontal_space > max_width {
                            if !cur_glyphs.is_empty() {
                                chunks.push(FontEncodedChunk {
                                    width: cur_width,
                                    font_height: cur_font_height,
                                    glyph_set: cur_gs.unwrap(),
                                    glyph_ids: cur_glyphs,
                                });
                            }
                            return chunks;
                        }
                        total_width += info.horizontal_space;
                        cur_width += info.horizontal_space;
                        cur_font_height = mh;
                        cur_glyphs.push(info.glyph_id);
                    }
                }
            }
            // Magic 254 glyph limit, might use a better solution than just cutting it off
            if total_glyphs == 254 {
                break;
            }
        }
        if !cur_glyphs.is_empty() {
            chunks.push(FontEncodedChunk {
                width: cur_width,
                font_height: cur_font_height,
                glyph_set: cur_gs.unwrap(),
                glyph_ids: cur_glyphs,
            });
        }
        chunks
    }

    pub fn geometry(&self, text: &str, fonts: &[FontCfg]) -> (i16, u16) {
        let mut width = 0;
        let mut height = 0;
        for char in text.chars() {
            if let Some(lchar) = self.chars.get(&char) {
                width += lchar.char_info.horizontal_space;
                if height < lchar.char_info.height {
                    height = lchar.char_info.height;
                }
            } else {
                for font_name in fonts {
                    if let Some(info) = self
                        .fonts
                        .get(font_name)
                        .and_then(|loaded| loaded.char_map.get(&char))
                    {
                        width += info.horizontal_space;
                        if height < info.height {
                            height = info.height;
                        }
                        continue;
                    }
                }
            }
        }
        (width, height)
    }
}

#[derive(Debug)]
pub struct FontEncodedChunk {
    width: i16,
    font_height: i16,
    glyph_set: Glyphset,
    glyph_ids: Vec<u16>,
}

#[allow(clippy::module_name_repetitions)]
pub struct LoadedFont {
    pub glyph_set: Glyphset,
    pub char_map: HashMap<char, CharInfo, FontHasherBuilder>,
    pub font_height: i16,
}

#[derive(Debug, Copy, Clone)]
pub struct CharInfo {
    pub glyph_id: u16,
    pub horizontal_space: i16,
    pub height: u16,
}
