pub mod font;
pub mod framebuffer;

#[derive(Clone, Copy)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy)]
pub struct Region {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Region {
    pub fn intersects(&self, other: &Region) -> bool {
        if self.x > other.x + other.width {
            return false;
        }
        if other.x > self.x + self.width {
            return false;
        }
        if self.y > other.y + other.height {
            return false;
        }
        if other.y > self.y + self.height {
            return false;
        }
        true
    }

    pub fn fully_contains(&self, other: &Region) -> bool {
        if self.x > other.x {
            return false;
        }
        if self.x + self.width < other.x + other.width {
            return false;
        }
        if self.y > other.y {
            return false;
        }
        if self.y + self.height < other.y + other.height {
            return false;
        }
        true
    }

    pub fn merge(&self, other: &Region) -> Region {
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.width).max(other.x + other.width);
        let y2 = (self.y + self.height).max(other.y + other.height);
        Region {
            x: x1,
            y: y1,
            width: x2 - x1,
            height: y2 - y1,
        }
    }
}
