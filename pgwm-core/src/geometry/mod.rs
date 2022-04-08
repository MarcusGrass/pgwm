use x11rb::protocol::xproto::Rectangle;

pub mod draw;
pub mod layout;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct Dimensions {
    pub width: i16,
    pub height: i16,
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Line {
    pub start: i16,
    pub length: i16,
}

impl Line {
    #[must_use]
    pub fn new(start: i16, length: i16) -> Self {
        Self { start, length }
    }

    #[must_use]
    pub fn contains(&self, x: i16) -> bool {
        x >= self.start && x <= self.start + self.length
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Size {
    pub width: i16,
    pub height: i16,
}

impl Size {
    #[must_use]
    pub fn new(start: i16, length: i16) -> Self {
        Self {
            width: start,
            height: length,
        }
    }

    #[must_use]
    pub fn contains(&self, pos: (i16, i16), x: i16, y: i16) -> bool {
        // Going from least likely
        // Bar is on top, and is small, click is likely outside
        y <= pos.1 + self.height &&
            // Horisontal position is about equally likely
            x <= pos.0 && x <= pos.0 + self.width &&
            // If above bottom line, most likely inside the bar
            y >= pos.1
    }
}

impl Dimensions {
    #[must_use]
    pub fn new(width: i16, height: i16, offset_x: i16, offset_y: i16) -> Self {
        Dimensions {
            width,
            height,
            x: offset_x,
            y: offset_y,
        }
    }

    #[must_use]
    pub fn to_rectangle(&self) -> Rectangle {
        Rectangle {
            x: self.x,
            y: self.y,
            width: self.width as u16,
            height: self.height as u16,
        }
    }

    #[must_use]
    pub fn contains(&self, x: i16, y: i16) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.width
    }
}
