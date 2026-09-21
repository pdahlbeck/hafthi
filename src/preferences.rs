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
    ToggleBranding,
    ChooseImage,
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
    pub row_height: f32,
    pub hovered: Option<PrefAction>,
}

impl PreferencesPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            x: 0.0,
            y: 0.0,
            width: 820.0,
            row_height: 68.0,
            hovered: None,
        }
    }

    pub fn open(&mut self, surface_width: u32, surface_height: u32, scale: f32) {
        self.width = 820.0 * scale;
        self.row_height = 68.0 * scale;
        let height = self.height();
        self.x = ((surface_width as f32 - self.width) / 2.0).max(8.0);
        self.y = ((surface_height as f32 - height) / 2.0).max(8.0);
        self.visible = true;
        self.hovered = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.hovered = None;
    }

    pub fn height(&self) -> f32 {
        self.row_height * 8.0
    }

    pub fn button_rects(&self) -> Vec<HitRect> {
        let small_w = self.width * 0.12;
        let gap = self.width * 0.025;
        let plus_x = self.x + self.width - gap - small_w;
        let minus_x = plus_x - gap - small_w;
        let button_h = self.row_height * 0.72;
        let y_inset = (self.row_height - button_h) / 2.0;
        let wide_w = small_w * 2.0 + gap;

        let row_button = |row: usize, x: f32, action: PrefAction| HitRect {
            x,
            y: self.y + row as f32 * self.row_height + y_inset,
            w: small_w,
            h: button_h,
            action,
        };

        vec![
            row_button(0, minus_x, PrefAction::FontDown),
            row_button(0, plus_x, PrefAction::FontUp),
            row_button(1, minus_x, PrefAction::OpacityDown),
            row_button(1, plus_x, PrefAction::OpacityUp),
            row_button(2, minus_x, PrefAction::PaddingDown),
            row_button(2, plus_x, PrefAction::PaddingUp),
            row_button(3, minus_x, PrefAction::ScrollbackDown),
            row_button(3, plus_x, PrefAction::ScrollbackUp),
            HitRect {
                x: minus_x,
                y: self.y + 4.0 * self.row_height + y_inset,
                w: wide_w,
                h: button_h,
                action: PrefAction::ToggleBranding,
            },
            HitRect {
                x: minus_x,
                y: self.y + 5.0 * self.row_height + y_inset,
                w: wide_w,
                h: button_h,
                action: PrefAction::ChooseImage,
            },
            row_button(6, minus_x, PrefAction::GifFpsDown),
            row_button(6, plus_x, PrefAction::GifFpsUp),
            HitRect {
                x: self.x + self.width * 0.50,
                y: self.y + 7.0 * self.row_height + y_inset,
                w: self.width * 0.21,
                h: button_h,
                action: PrefAction::Cancel,
            },
            HitRect {
                x: self.x + self.width * 0.74,
                y: self.y + 7.0 * self.row_height + y_inset,
                w: self.width * 0.21,
                h: button_h,
                action: PrefAction::Save,
            },
        ]
    }

    pub fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let old = self.hovered;
        self.hovered = self
            .button_rects()
            .into_iter()
            .find(|rect| rect.contains(x, y))
            .map(|rect| rect.action);
        old != self.hovered
    }

    pub fn action_at(&self, x: f32, y: f32) -> Option<PrefAction> {
        if !self.visible {
            return None;
        }

        self.button_rects()
            .into_iter()
            .find(|rect| rect.contains(x, y))
            .map(|rect| rect.action)
    }
}
