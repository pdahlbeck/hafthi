#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefPage {
    Appearance,
    Terminal,
    Background,
}

impl PrefPage {
    pub const ALL: [Self; 3] = [Self::Appearance, Self::Terminal, Self::Background];

    pub fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Terminal => "Terminal",
            Self::Background => "Background",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefAction {
    SelectPage(PrefPage),
    FontDown,
    FontUp,
    OpacitySet(u8),
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
    pub page: PrefPage,
    pub hovered: Option<PrefAction>,
    pub hovered_page: Option<PrefPage>,
}

impl PreferencesPanel {
    pub const WIDTH: f32 = 736.0;
    pub const HEIGHT: f32 = 476.0;
    pub const SIDEBAR_WIDTH: f32 = 192.0;
    pub const CONTENT_X: f32 = 208.0;
    pub const CONTENT_WIDTH: f32 = 512.0;

    pub fn new() -> Self {
        Self {
            visible: false,
            x: 0.0,
            y: 0.0,
            width: Self::WIDTH,
            scale: 1.0,
            page: PrefPage::Appearance,
            hovered: None,
            hovered_page: None,
        }
    }

    pub fn open(&mut self, surface_width: u32, surface_height: u32, scale: f32) {
        self.scale = scale
            .max(0.1)
            .min((surface_width as f32 - 16.0).max(1.0) / Self::WIDTH)
            .min((surface_height as f32 - 16.0).max(1.0) / Self::HEIGHT);
        self.width = Self::WIDTH * self.scale;
        self.x = (surface_width as f32 - self.width) / 2.0;
        self.y = (surface_height as f32 - self.height()) / 2.0;
        self.visible = true;
        self.hovered = None;
        self.hovered_page = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.hovered = None;
        self.hovered_page = None;
    }

    pub fn height(&self) -> f32 {
        Self::HEIGHT * self.scale
    }

    pub fn pos(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x * self.scale, self.y + y * self.scale)
    }

    pub fn page_rect(&self, page: PrefPage) -> HitRect {
        let index = PrefPage::ALL
            .iter()
            .position(|candidate| *candidate == page)
            .unwrap();
        let (x, y) = self.pos(9.0, 83.0 + index as f32 * 43.0);
        HitRect {
            x,
            y,
            w: 174.0 * self.scale,
            h: 37.0 * self.scale,
            action: PrefAction::SelectPage(page),
        }
    }

    pub fn slider_rect(&self) -> (f32, f32, f32, f32) {
        let (x, y) = self.pos(226.0, 223.0);
        (x, y, 452.0 * self.scale, 28.0 * self.scale)
    }

    pub fn button_rects(&self) -> Vec<HitRect> {
        use PrefAction::*;
        let mut buttons = Vec::new();
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
        match self.page {
            PrefPage::Appearance => {
                add(626.0, 126.0, 35.0, 34.0, FontDown);
                add(669.0, 126.0, 35.0, 34.0, FontUp);
                add(626.0, 289.0, 35.0, 34.0, PaddingDown);
                add(669.0, 289.0, 35.0, 34.0, PaddingUp);
            }
            PrefPage::Terminal => {
                add(626.0, 126.0, 35.0, 34.0, ScrollbackDown);
                add(669.0, 126.0, 35.0, 34.0, ScrollbackUp);
            }
            PrefPage::Background => {
                add(226.0, 126.0, 106.0, 35.0, ImageOff);
                add(337.0, 126.0, 126.0, 35.0, ImageBanner);
                add(468.0, 126.0, 106.0, 35.0, ImageFull);
                add(494.0, 260.0, 117.0, 34.0, ChooseImage);
                add(618.0, 260.0, 86.0, 34.0, ClearImage);
                add(626.0, 348.0, 35.0, 34.0, GifFpsDown);
                add(669.0, 348.0, 35.0, 34.0, GifFpsUp);
            }
        }
        add(526.0, 430.0, 86.0, 34.0, Cancel);
        add(620.0, 430.0, 84.0, 34.0, Save);
        buttons
    }

    pub fn opacity_for_x(&self, x: f32) -> u8 {
        let (sx, _, sw, _) = self.slider_rect();
        (((x - sx) / sw) * 100.0).round().clamp(0.0, 100.0) as u8
    }

    fn slider_value_at(&self, x: f32, y: f32) -> Option<u8> {
        if self.page != PrefPage::Appearance {
            return None;
        }
        let (sx, sy, sw, sh) = self.slider_rect();
        if x >= sx && x <= sx + sw && y >= sy && y <= sy + sh {
            Some(self.opacity_for_x(x))
        } else {
            None
        }
    }

    pub fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let old = (self.hovered, self.hovered_page);
        self.hovered = self
            .button_rects()
            .into_iter()
            .find(|rect| rect.contains(x, y))
            .map(|rect| rect.action);
        self.hovered_page = PrefPage::ALL
            .into_iter()
            .find(|page| self.page_rect(*page).contains(x, y));
        old != (self.hovered, self.hovered_page)
    }

    pub fn action_at(&self, x: f32, y: f32) -> Option<PrefAction> {
        if !self.visible {
            return None;
        }
        if let Some(page) = PrefPage::ALL
            .into_iter()
            .find(|page| self.page_rect(*page).contains(x, y))
        {
            return Some(PrefAction::SelectPage(page));
        }
        if let Some(value) = self.slider_value_at(x, y) {
            return Some(PrefAction::OpacitySet(value));
        }
        self.button_rects()
            .into_iter()
            .find(|rect| rect.contains(x, y))
            .map(|rect| rect.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_and_slider_hit_targets_follow_the_active_page() {
        let mut panel = PreferencesPanel::new();
        panel.open(980, 640, 1.0);

        let background = panel.page_rect(PrefPage::Background);
        assert_eq!(
            panel.action_at(background.x + 10.0, background.y + 10.0),
            Some(PrefAction::SelectPage(PrefPage::Background)),
        );
        let (x, y, w, h) = panel.slider_rect();
        assert_eq!(
            panel.action_at(x + w / 2.0, y + h / 2.0),
            Some(PrefAction::OpacitySet(50))
        );

        panel.page = PrefPage::Background;
        assert_eq!(panel.action_at(x + w / 2.0, y + h / 2.0), None);
        let choose = panel
            .button_rects()
            .into_iter()
            .find(|button| button.action == PrefAction::ChooseImage)
            .unwrap();
        assert_eq!(
            panel.action_at(choose.x + 5.0, choose.y + 5.0),
            Some(PrefAction::ChooseImage)
        );
    }
}
