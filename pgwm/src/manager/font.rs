use crate::error::{Error, Result};
use crate::x11::call_wrapper::CallWrapper;
use fontdue::FontSettings;
use pgwm_core::render::{DoubleBufferedRenderPicture, RenderVisualInfo};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use x11rb::protocol::render::{Glyphinfo, Glyphset};

use pgwm_core::colors::Color;
use pgwm_core::config::{FontCfg, Fonts};
use pgwm_core::geometry::Dimensions;

pub(crate) struct FontDrawer<'a> {
    call_wrapper: &'a CallWrapper<'a>,
    loaded_render_fonts: &'a LoadedFonts<'a>,
}

impl<'a> FontDrawer<'a> {
    pub(crate) fn new(
        call_wrapper: &'a CallWrapper<'a>,
        loaded_xrender_fonts: &'a LoadedFonts<'a>,
    ) -> Self {
        Self {
            call_wrapper,
            loaded_render_fonts: loaded_xrender_fonts,
        }
    }

    pub(crate) fn text_geometry(&self, text: &str, fonts: &[FontCfg]) -> (i16, u16) {
        self.loaded_render_fonts.geometry(text, fonts)
    }

    pub(crate) fn draw(
        &self,
        dbw: &DoubleBufferedRenderPicture,
        text: &str,
        fonts: &[FontCfg],
        area: Dimensions,
        area_x: i16,
        area_y: i16,
        bg: Color,
        text_color: Color,
    ) -> Result<()> {
        let encoded = self.loaded_render_fonts.encode(text, fonts);
        pgwm_core::debug!("Encoded {encoded:?}");
        self.call_wrapper
            .fill_xrender_rectangle(dbw.window.picture, bg.as_render_color(), area)?;
        self.call_wrapper.fill_xrender_rectangle(
            dbw.pixmap.picture,
            text_color.as_render_color(),
            Dimensions::new(1, 1, 0, 0),
        )?;
        let mut offset = area.x + area_x;
        for chunk in encoded {
            let box_shift = if chunk.glyph_ids.len() == 1 {
                (area.height - chunk.font_height as i16) / 2
            } else {
                -((area.height - chunk.height as i16) / 2 - (area.height - chunk.font_height))
            };

            self.call_wrapper.draw_glyphs(
                offset,
                area.y + area_y + box_shift,
                chunk.glyph_set,
                dbw,
                &chunk.glyph_ids,
            )?;

            offset += chunk.width as i16;
        }
        Ok(())
    }
}

pub(crate) fn load_alloc_fonts<'a>(
    call_wrapper: &'a CallWrapper<'a>,
    vis_info: &RenderVisualInfo,
    fonts: &'a Fonts,
    char_remap: &'a HashMap<heapless::String<4>, FontCfg>,
) -> Result<HashMap<&'a FontCfg, LoadedFont>> {
    let mut map = HashMap::new();
    for f_cfg in fonts
        .workspace_section
        .iter()
        .chain(fonts.window_name_display_section.iter())
        .chain(fonts.status_section.iter())
        .chain(fonts.shortcut_section.iter())
        .chain(fonts.tab_bar_section.iter())
        .chain(char_remap.values())
    {
        // Ugly and kind of dumb
        if let Entry::Vacant(v) = map.entry(f_cfg) {
            let data = std::fs::read(&f_cfg.path)?;

            let gs = call_wrapper.create_glyphset(vis_info)?;

            let mut ids = vec![];
            let mut infos = vec![];
            let mut raw_data = vec![];
            let mut char_map = HashMap::new();
            let rasterized = fontdue::rasterize_all(
                data.as_slice(),
                f_cfg.size.parse::<f32>().map_err(|_| Error::ParseFloat)?,
                FontSettings::default(),
            )
            .map_err(Error::FontLoad)?;
            for data in rasterized.data {
                ids.push(data.ch as u32);
                for byte in data.buf {
                    raw_data.extend_from_slice(&[byte, byte, byte, byte]);
                }
                let glyph_info = Glyphinfo {
                    width: data.metrics.width as u16,
                    height: data.metrics.height as u16,
                    x: -data.metrics.xmin as i16,
                    y: data.metrics.height as i16 - rasterized.max_height as i16
                        + data.metrics.ymin as i16, // pt2
                    x_off: data.metrics.advance_width.ceil() as i16,
                    y_off: data.metrics.advance_height.ceil() as i16,
                };
                infos.push(glyph_info);
                char_map.insert(
                    data.ch,
                    CharInfo {
                        glyph_id: data.ch as u32,
                        glyph_info,
                    },
                );
            }
            call_wrapper.add_glyphs(gs, &ids, &infos, &raw_data)?;
            v.insert(LoadedFont {
                glyph_set: gs,
                char_map,
                font_height: rasterized.max_height as i16,
            });
        }
    }
    Ok(map)
}

pub struct LoadedFonts<'a> {
    pub(crate) fonts: HashMap<&'a FontCfg, LoadedFont>,
    chars: HashMap<char, LoadedChar>,
}

struct LoadedChar {
    gsid: Glyphset,
    char_info: CharInfo,
    font_height: i16,
}

impl<'a> LoadedFonts<'a> {
    pub(crate) fn new(
        fonts: HashMap<&'a FontCfg, LoadedFont>,
        char_mapping: &HashMap<heapless::String<4>, FontCfg>,
    ) -> Result<Self> {
        let mut chars = HashMap::new();
        for (char, font) in char_mapping {
            let maybe_char = char.chars().next();
            match maybe_char {
                Some(char) => match fonts.get(font) {
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
                },
                None => return Err(Error::BadCharFontMapping("No char supplied")),
            }
        }
        Ok(Self { fonts, chars })
    }

    #[must_use]
    pub fn encode(&self, text: &str, fonts: &[FontCfg]) -> Vec<FontEncodedChunk> {
        let mut cur_width = 0;
        let mut cur_max_height = 0;
        let mut cur_gs = None;
        let mut cur_glyphs = vec![];
        let mut chunks = vec![];
        let mut cur_font_height = 0;
        for char in text.chars() {
            if let Some(lchar) = self.chars.get(&char) {
                if !cur_glyphs.is_empty() {
                    chunks.push(FontEncodedChunk {
                        width: std::mem::take(&mut cur_width),
                        height: std::mem::take(&mut cur_max_height),
                        font_height: std::mem::take(&mut cur_font_height),
                        glyph_set: cur_gs.unwrap(),
                        glyph_ids: std::mem::take(&mut cur_glyphs),
                    });
                }
                chunks.push(FontEncodedChunk {
                    width: lchar.char_info.glyph_info.x_off,
                    height: lchar.char_info.glyph_info.height,
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
                                width: std::mem::take(&mut cur_width),
                                height: std::mem::take(&mut cur_max_height),
                                font_height: mh,
                                glyph_set: cur_gs.unwrap(),
                                glyph_ids: std::mem::take(&mut cur_glyphs),
                            });
                            cur_gs = Some(gs);
                            cur_width = 0;
                        }
                        cur_width += info.glyph_info.x_off;
                        if cur_max_height < info.glyph_info.height {
                            cur_max_height = info.glyph_info.height;
                        }
                        cur_font_height = mh;
                        cur_glyphs.push(info.glyph_id);
                    }
                }
            }
        }
        if !cur_glyphs.is_empty() {
            chunks.push(FontEncodedChunk {
                width: cur_width,
                height: cur_max_height,
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
                width += lchar.char_info.glyph_info.x_off;
                if height < lchar.char_info.glyph_info.height {
                    height = lchar.char_info.glyph_info.height;
                }
            } else {
                for font_name in fonts {
                    if let Some(info) = self
                        .fonts
                        .get(font_name)
                        .and_then(|loaded| loaded.char_map.get(&char))
                    {
                        width += info.glyph_info.x_off;
                        if height < info.glyph_info.height {
                            height = info.glyph_info.height;
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
    height: u16,
    font_height: i16,
    glyph_set: Glyphset,
    glyph_ids: Vec<u32>,
}

#[allow(clippy::module_name_repetitions)]
pub struct LoadedFont {
    pub glyph_set: Glyphset,
    pub char_map: HashMap<char, CharInfo>,
    pub font_height: i16,
}

#[derive(Debug, Copy, Clone)]
pub struct CharInfo {
    pub glyph_id: u32,
    pub glyph_info: Glyphinfo,
}
