#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefAction {
    FontDown,
    FontUp,
    OpacityDown,
    OpacityUp,
    PaddingDown,
    PaddingUp,
    ScrollbackDown,
    ScrollbackUp,
    ImageOff,
    ImageBanner,
    ImageFull,
    ChooseImage,
    ClearImage,
    GifFpsDown,
    GifFpsUp,
    Save,
    Cancel,
}

#[derive(Debug, Clone, Copy)]
pub struct HitRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub action: PrefAction,
}

impl HitRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Clone)]
pub struct PreferencesPanel {
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub scale: f32,
    pub hovered: Option<PrefAction>,
    pub hovered_row: Option<usize>,
}

impl PreferencesPanel {
    pub const WIDTH: f32 = 680.0;
    pub const HEIGHT: f32 = 596.0;
    pub const ROWS: [(f32, f32); 7] = [
        (110.0, 44.0),
        (154.0, 44.0),
        (198.0, 44.0),
        (277.0, 44.0),
        (356.0, 44.0),
        (404.0, 76.0),
        (484.0, 44.0),
    ];

    pub fn new() -> Self {
        Self {
            visible: false,
            x: 0.0,
            y: 0.0,
            width: Self::WIDTH,
            scale: 1.0,
            hovered: None,
            hovered_row: None,
        }
    }

    pub fn open(&mut self, surface_width: u32, surface_height: u32, scale: f32) {
        // Fit all controls into the current terminal, even after a resize.
        self.scale = scale
            .max(0.1)
            .min((surface_width as f32 - 16.0).max(1.0) / Self::WIDTH)
            .min((surface_height as f32 - 16.0).max(1.0) / Self::HEIGHT);
        self.width = Self::WIDTH * self.scale;
        self.x = (surface_width as f32 - self.width) / 2.0;
        self.y = (surface_height as f32 - self.height()) / 2.0;
        self.visible = true;
        self.hovered = None;
        self.hovered_row = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.hovered = None;
        self.hovered_row = None;
    }

    pub fn height(&self) -> f32 {
        Self::HEIGHT * self.scale
    }

    pub fn pos(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x * self.scale, self.y + y * self.scale)
    }

    pub fn row_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let (top, height) = Self::ROWS[index];
        let (x, y) = self.pos(8.0, top);
        (x, y, self.width - 16.0 * self.scale, height * self.scale)
    }

    pub fn button_rects(&self) -> Vec<HitRect> {
        use PrefAction::*;
        let mut buttons = Vec::with_capacity(17);
        let mut add = |x: f32, y: f32, w: f32, h: f32, action| {
            let (x, y) = self.pos(x, y);
            buttons.push(HitRect {
                x,
                y,
                w: w * self.scale,
                h: h * self.scale,
                action,
            });
        };
        for (row, down, up) in [
            (0, FontDown, FontUp),
            (1, OpacityDown, OpacityUp),
            (2, PaddingDown, PaddingUp),
            (3, ScrollbackDown, ScrollbackUp),
            (6, GifFpsDown, GifFpsUp),
        ] {
            let y = Self::ROWS[row].0 + 5.0;
            add(568.0, y, 34.0, 34.0, down);
            add(610.0, y, 34.0, 34.0, up);
        }
        add(416.0, 361.0, 68.0, 34.0, ImageOff);
        add(484.0, 361.0, 88.0, 34.0, ImageBanner);
        add(572.0, 361.0, 72.0, 34.0, ImageFull);
        add(490.0, 422.0, 92.0, 34.0, ChooseImage);
        add(590.0, 422.0, 54.0, 34.0, ClearImage);
        add(468.0, 546.0, 82.0, 36.0, Cancel);
        add(558.0, 546.0, 86.0, 36.0, Save);
        buttons
    }

    pub fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let old = (self.hovered, self.hovered_row);
        self.hovered = self
            .button_rects()
            .into_iter()
            .find(|rect| rect.contains(x, y))
            .map(|rect| rect.action);
        self.hovered_row = (0..Self::ROWS.len()).find(|&i| {
            let (rx, ry, rw, rh) = self.row_rect(i);
            x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
        });
        old != (self.hovered, self.hovered_row)
    }

    pub fn action_at(&self, x: f32, y: f32) -> Option<PrefAction> {
        self.visible.then(|| self.button_rects()).and_then(|rects| {
            rects
                .into_iter()
                .find(|rect| rect.contains(x, y))
                .map(|rect| rect.action)
        })
    }
}
