use crate::error::{Result, XftError};
use pgwm_core::colors::{Color, Colors, Rgba8};
use pgwm_core::config::{Fonts, FONT_WRITE_BUF_LIMIT, UTF8_CHAR_MAX_BYTES};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_uchar, c_ulong};
use std::ptr::null;
use x11::xft::{
    XftCharExists, XftColor, XftColorAllocValue, XftColorFree, XftDraw, XftDrawCreate,
    XftDrawDestroy, XftDrawStringUtf8, XftFont, XftFontClose, XftFontOpenName, XftTextExtentsUtf8,
};
use x11::xlib::{
    Display, Visual, XCloseDisplay, XDefaultColormap, XDefaultScreen, XDefaultVisual, XOpenDisplay,
    XSync,
};
use x11::xrender::{XGlyphInfo, XRenderColor};
use x11rb::protocol::xproto::Window;

pub(crate) struct FontManager<'a> {
    dpy: *mut Display,
    visual: *mut Visual,
    color_map: c_ulong,
    loaded_fonts: HashMap<&'a str, *mut XftFont>,
    loaded_colors: HashMap<u32, MaybeUninit<XftColor>>,
    icon_mapping: HashMap<char, *mut XftFont>,
}

#[allow(unsafe_code)]
impl<'a> FontManager<'a> {
    pub fn new(
        colors: &'a Colors,
        fonts: &'a Fonts,
        font_icon_overrides: &'a HashMap<heapless::String<UTF8_CHAR_MAX_BYTES>, String>,
    ) -> Result<Self> {
        unsafe {
            let dpy = XOpenDisplay(null());
            let screen = XDefaultScreen(dpy);
            let visual = XDefaultVisual(dpy, screen);
            let color_map = XDefaultColormap(dpy, screen);
            if dpy.is_null() {
                Err(XftError::OpenDisplay.into())
            } else {
                let mut fm = FontManager {
                    dpy,
                    visual,
                    color_map,
                    loaded_fonts: HashMap::new(),
                    loaded_colors: HashMap::new(),
                    icon_mapping: HashMap::new(),
                };
                for color in colors.get_all() {
                    fm.load_color(color)?;
                }
                let mut font_set = HashSet::new();
                fonts.status_section.iter().for_each(|f| {
                    font_set.insert(f);
                });
                fonts.window_name_display_section.iter().for_each(|f| {
                    font_set.insert(f);
                });
                fonts.workspace_section.iter().for_each(|f| {
                    font_set.insert(f);
                });
                fonts.tab_bar_section.iter().for_each(|f| {
                    font_set.insert(f);
                });
                for font in font_icon_overrides.values() {
                    font_set.insert(font);
                }
                for font in &font_set {
                    fm.load_font(font)?;
                }
                for (k, font_name) in font_icon_overrides.iter() {
                    let mut chars = k.chars();
                    let count = k.chars().count();
                    assert_eq!(
                        count, 1,
                        "Icon remapping expected a single char, {count} supplied in string {k:?}"
                    );
                    let char = chars.next().unwrap();
                    let _ = fm
                        .icon_mapping
                        .insert(char, fm.loaded_fonts[font_name.as_str()]);
                }
                Ok(fm)
            }
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn draw_text(
        &self,
        win: Window,
        text: &str,
        x: u32,
        y: u32,
        fonts: &[String],
        color: &u32,
        status_bar_height: i16,
    ) -> Result<u16> {
        let draw = self.create_draw(win)?;
        unsafe {
            let color = self.loaded_colors[color];
            let drawn = self.draw_with_available(
                draw,
                text,
                fonts,
                color.as_ptr(),
                x,
                y,
                status_bar_height,
            )?;
            if !draw.is_null() {
                XftDrawDestroy(draw);
            }
            XSync(self.dpy, 0);
            Ok(drawn)
        }
    }

    fn draw_with_available(
        &self,
        drawable: *mut XftDraw,
        text: &str,
        fonts: &[String],
        color: *const XftColor,
        x_start: u32,
        y: u32,
        status_bar_height: i16,
    ) -> Result<u16> {
        let (width, _) =
            self.pass_over_buf(text, fonts, x_start, |str, font, current_offset| unsafe {
                let res = self.do_draw(
                    drawable,
                    str,
                    font,
                    color,
                    current_offset as u32,
                    y,
                    status_bar_height,
                )?;
                Ok(res as u32)
            })?;
        Ok(width - x_start as u16)
    }

    pub fn get_width_and_height(&self, text: &str, fonts: &[String]) -> Result<(u16, u16)> {
        self.pass_over_buf(text, fonts, 0, |str, font, _| {
            let repr = CString::new(str)?;
            let exts = self.get_exts(
                font,
                repr.as_ptr().cast::<u8>(),
                repr.as_bytes().len() as c_int,
            )?;
            Ok(exts.xOff as u32)
        })
    }

    fn pass_over_buf<F>(
        &self,
        text: &str,
        fonts: &[String],
        start_offset: u32,
        mut func: F,
    ) -> Result<(u16, u16)>
    where
        F: FnMut(&str, *mut XftFont, u32) -> Result<u32>,
    {
        let mut check_font_ind = 0;
        let mut cur_font = self.get_font(fonts[check_font_ind].as_str())?;
        let mut char_buf: heapless::String<FONT_WRITE_BUF_LIMIT> = heapless::String::new();
        let mut current_offset = start_offset;
        let mut max_height = 0;
        for cur_char in text.chars() {
            unsafe {
                let utf8val = cur_char as u32;
                loop {
                    if let Some(manual_mapping) = self.icon_mapping.get(&cur_char) {
                        if !char_buf.is_empty() {
                            let str = char_buf.as_str();
                            current_offset += func(str, cur_font, current_offset)?;
                            let height = (*cur_font).ascent + (*cur_font).descent;
                            if height >= max_height {
                                max_height = height;
                            }
                            char_buf = heapless::String::new();
                        }
                        cur_font = *manual_mapping;
                        char_buf.push(cur_char).unwrap();
                        let str = char_buf.as_str();
                        current_offset += func(str, cur_font, current_offset)?;
                        let height = (*cur_font).ascent + (*cur_font).descent;
                        if height >= max_height {
                            max_height = height;
                        }
                        char_buf = heapless::String::new();
                        check_font_ind = 0; // Use default font if possible
                        cur_font = self.get_font(fonts[check_font_ind].as_str())?;
                        break;
                    } else if XftCharExists(self.dpy, cur_font, utf8val) == 0 {
                        if !char_buf.is_empty() {
                            let str = char_buf.as_str();
                            current_offset += func(str, cur_font, current_offset)?;
                            let height = (*cur_font).ascent + (*cur_font).descent;
                            if height >= max_height {
                                max_height = height;
                            }
                            char_buf = heapless::String::new();
                        }
                        if check_font_ind < fonts.len() - 1 {
                            check_font_ind += 1;
                            cur_font = self.get_font(fonts[check_font_ind].as_str())?;
                        } else {
                            check_font_ind = 0;
                            cur_font = self.get_font(fonts[check_font_ind].as_str())?;
                            if !char_buf.is_empty() {
                                let str = char_buf.as_str();
                                current_offset += func(str, cur_font, current_offset)?;
                                let height = (*cur_font).ascent + (*cur_font).descent;
                                if height >= max_height {
                                    max_height = height;
                                }
                                char_buf = heapless::String::new();
                            }
                            break;
                        }
                    } else {
                        char_buf.push(cur_char).unwrap();
                        // Dump buf if too big or if we can switch to normal font
                        if check_font_ind != 0
                            || char_buf.len() > FONT_WRITE_BUF_LIMIT - UTF8_CHAR_MAX_BYTES
                        {
                            check_font_ind = 0; // Use default font if possible
                            let str = char_buf.as_str();
                            current_offset += func(str, cur_font, current_offset)?;
                            let height = (*cur_font).ascent + (*cur_font).descent;
                            if height >= max_height {
                                max_height = height;
                            }
                            char_buf = heapless::String::new();
                            cur_font = self.get_font(fonts[check_font_ind].as_str())?;
                        }
                        break;
                    }
                }
            }
        }
        if !char_buf.is_empty() {
            let str = char_buf.as_str();
            current_offset += func(str, cur_font, current_offset)?;
            unsafe {
                let height = (*cur_font).ascent + (*cur_font).descent;
                if height >= max_height {
                    max_height = height;
                }
            }
        }
        Ok((current_offset as u16, max_height as u16))
    }

    unsafe fn do_draw(
        &self,
        drawable: *mut XftDraw,
        text: &str,
        font: *mut XftFont,
        color: *const XftColor,
        x: u32,
        y: u32,
        status_bar_height: i16,
    ) -> Result<u16> {
        let len = text.as_bytes().len() as c_int;
        let converted = text.as_ptr().cast::<u8>();
        let font_deref = *font;
        let y =
            y as c_int + (status_bar_height as c_int - font_deref.height) / 2 + font_deref.ascent;
        XftDrawStringUtf8(drawable, color, font, x as c_int, y, converted, len);
        let exts = self.get_exts(font, converted, len)?;
        Ok(exts.xOff as u16)
    }

    fn get_exts(&self, font: *mut XftFont, text: *const c_uchar, len: c_int) -> Result<XGlyphInfo> {
        let mut glyph_info = MaybeUninit::uninit();
        unsafe {
            XftTextExtentsUtf8(self.dpy, font, text, len, glyph_info.as_mut_ptr());
            if glyph_info.as_ptr().is_null() {
                Err(XftError::GetGlyphInfo.into())
            } else {
                Ok(glyph_info.assume_init())
            }
        }
    }

    fn create_draw(&self, win: Window) -> Result<*mut XftDraw> {
        unsafe {
            if !self.visual.is_null() {
                let draw_create = XftDrawCreate(self.dpy, win as u64, self.visual, self.color_map);
                if !draw_create.is_null() {
                    return Ok(draw_create);
                }
            }
        }
        Err(XftError::CreateDraw.into())
    }

    unsafe fn load_color(&mut self, user_color: Color) -> Result<()> {
        let mut color: MaybeUninit<XftColor> = MaybeUninit::uninit();
        let (red, green, blue, alpha) = user_color.rgba8.to_rgba16();
        let render_col = XRenderColor {
            red,
            green,
            blue,
            alpha,
        };
        let ptr = std::ptr::addr_of!(render_col);
        let exit_code = XftColorAllocValue(
            self.dpy,
            self.visual,
            self.color_map,
            ptr,
            color.as_mut_ptr(),
        );
        if exit_code == 0 || color.as_ptr().is_null() {
            return Err(XftError::AllocColorByRgb(format!("{:?}", user_color)).into());
        }
        pgwm_core::debug!(
            "Allocated RGBA ({}, {}, {}, {}) as pixel {}",
            red,
            green,
            blue,
            alpha,
            color.assume_init().pixel
        );
        self.loaded_colors.insert(user_color.pixel, color);
        Ok(())
    }

    fn get_font(&self, font_name: &'a str) -> Result<*mut XftFont> {
        self.loaded_fonts
            .get(font_name)
            .copied()
            .ok_or_else(|| XftError::LoadFont(font_name.to_owned()).into())
    }

    fn load_font(&mut self, font_name: &'a str) -> Result<()> {
        if !self.loaded_fonts.contains_key(font_name) {
            let loaded = get_font(self.dpy, font_name)?;
            pgwm_core::debug!("Loaded font {}", font_name);
            self.loaded_fonts.insert(font_name, loaded);
        }
        Ok(())
    }
}

// Just clean up some stuff
#[allow(unsafe_code)]
impl<'a> Drop for FontManager<'a> {
    fn drop(&mut self) {
        unsafe {
            for font_ptr in self.loaded_fonts.values() {
                if !font_ptr.is_null() {
                    XftFontClose(self.dpy, *font_ptr);
                }
            }
            for col_ptr in self.loaded_colors.values_mut() {
                if !col_ptr.as_mut_ptr().is_null() {
                    XftColorFree(self.dpy, self.visual, self.color_map, col_ptr.as_mut_ptr());
                }
            }
            if !self.dpy.is_null() {
                XCloseDisplay(self.dpy);
            }
        }
    }
}

#[allow(unsafe_code)]
fn get_font(dpy: *mut Display, font_name: &str) -> Result<*mut XftFont> {
    unsafe {
        let cstr = CString::new(font_name)?;
        let xft_font = XftFontOpenName(dpy, 0, cstr.as_ptr());
        if xft_font.is_null() {
            pgwm_core::debug!("Nullptr from opening font");
            Err(XftError::LoadFont(font_name.to_owned()).into())
        } else {
            Ok(xft_font)
        }
    }
}
