#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefPage {
    Appearance,
    Terminal,
    Background,
    Plugins,
}

impl PrefPage {
    pub const ALL: [Self; 4] = [Self::Appearance, Self::Terminal, Self::Background, Self::Plugins];

    pub fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Terminal => "Terminal",
            Self::Background => "Background",
            Self::Plugins => "Plugins",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plugin {
    Fish,
    Starship,
    Tgpt,
}

impl Plugin {
    pub const ALL: [Self; 3] = [Self::Fish, Self::Starship, Self::Tgpt];

    pub fn title(self) -> &'static str {
        match self {
            Self::Fish => "Fish",
            Self::Starship => "Starship",
            Self::Tgpt => "tgpt",
        }
    }

    pub fn github_url(self) -> &'static str {
        match self {
            Self::Fish => "https://github.com/fish-shell/fish-shell",
            Self::Starship => "https://github.com/starship/starship",
            Self::Tgpt => "https://github.com/aandrew-me/tgpt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefAction {
    SelectPage(PrefPage),
    OpenPlugin(Plugin),
    BackToPlugins,
    OpenPluginGithub(Plugin),
    FontDown,
    FontUp,
    OpacitySet(u8),
    PaddingDown,
    PaddingUp,
    ScrollbackDown,
    ScrollbackUp,
    FontFamilyPrev,
    FontFamilyNext,
    TextColor(u8),
    EditTextColor,
    ToggleCommandHelp,
    ToggleFish,
    ToggleFishGreeting,
    ToggleStarship,
    EditQuestion,
    AskQuestion,
    InstallTgpt,
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
    pub plugin: Option<Plugin>,
    pub hovered: Option<PrefAction>,
    pub hovered_page: Option<PrefPage>,
    pub color_editing: bool,
    pub color_input: String,
    pub color_error: bool,
    pub question_editing: bool,
    pub question_input: String,
}

impl PreferencesPanel {
    pub const WIDTH: f32 = 736.0;
    pub const HEIGHT: f32 = 476.0;
    pub const SIDEBAR_WIDTH: f32 = 192.0;

    pub fn new() -> Self {
        Self {
            visible: false,
            x: 0.0,
            y: 0.0,
            width: Self::WIDTH,
            scale: 1.0,
            page: PrefPage::Appearance,
            plugin: None,
            hovered: None,
            hovered_page: None,
            color_editing: false,
            color_input: String::new(),
            color_error: false,
            question_editing: false,
            question_input: String::new(),
        }
    }

    pub fn open(&mut self, surface_width: u32, surface_height: u32, _scale: f32) {
        // The window dimensions are already physical pixels. Grow the panel
        // with the available space instead of anchoring it to the monitor's
        // scale factor, which can be 1 on a very large Wayland window.
        self.scale = ((surface_width as f32 * 0.80) / Self::WIDTH)
            .min((surface_height as f32 * 0.78) / Self::HEIGHT)
            .min(2.0)
            .min((surface_width as f32 - 16.0).max(1.0) / Self::WIDTH)
            .min((surface_height as f32 - 16.0).max(1.0) / Self::HEIGHT);
        self.width = Self::WIDTH * self.scale;
        self.x = (surface_width as f32 - self.width) / 2.0;
        self.y = (surface_height as f32 - self.height()) / 2.0;
        self.visible = true;
        self.hovered = None;
        self.hovered_page = None;
        self.color_editing = false;
        self.color_error = false;
        self.question_editing = false;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.hovered = None;
        self.hovered_page = None;
        self.color_editing = false;
        self.question_editing = false;
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
                add(626.0, 125.0, 35.0, 34.0, FontFamilyPrev);
                add(669.0, 125.0, 35.0, 34.0, FontFamilyNext);
                for i in 0..TEXT_SWATCHES.len() {
                    add(226.0 + i as f32 * 54.0, 238.0, 36.0, 32.0, TextColor(i as u8));
                }
                add(526.0, 278.0, 178.0, 31.0, EditTextColor);
                add(626.0, 351.0, 35.0, 34.0, ScrollbackDown);
                add(669.0, 351.0, 35.0, 34.0, ScrollbackUp);
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
            PrefPage::Plugins => match self.plugin {
                None => {
                    for (index, plugin) in Plugin::ALL.into_iter().enumerate() {
                        let y = 139.0 + index as f32 * 89.0;
                        add(209.0, y, 404.0, 78.0, OpenPlugin(plugin));
                        add(620.0, y + 22.0, 84.0, 34.0, match plugin {
                            Plugin::Fish => ToggleFish,
                            Plugin::Starship => ToggleStarship,
                            Plugin::Tgpt => ToggleCommandHelp,
                        });
                    }
                }
                Some(plugin) => {
                    add(592.0, 20.0, 112.0, 34.0, BackToPlugins);
                    add(226.0, 349.0, 182.0, 34.0, OpenPluginGithub(plugin));
                    match plugin {
                        Plugin::Fish => {
                            add(620.0, 125.0, 84.0, 34.0, ToggleFish);
                            add(620.0, 221.0, 84.0, 34.0, ToggleFishGreeting);
                        }
                        Plugin::Starship => add(620.0, 125.0, 84.0, 34.0, ToggleStarship),
                        Plugin::Tgpt => {
                            add(620.0, 125.0, 84.0, 34.0, ToggleCommandHelp);
                            add(226.0, 224.0, 478.0, 38.0, EditQuestion);
                            add(590.0, 276.0, 114.0, 35.0, AskQuestion);
                            add(430.0, 349.0, 134.0, 34.0, InstallTgpt);
                        }
                    }
                }
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

    #[test]
    fn panel_grows_on_large_windows_and_fits_small_ones() {
        let mut panel = PreferencesPanel::new();
        panel.open(1816, 2316, 1.0);
        assert!(panel.width > 1400.0);
        assert!(panel.x >= 0.0 && panel.y >= 0.0);
        assert!(panel.x + panel.width <= 1816.0);
        assert!(panel.y + panel.height() <= 2316.0);

        panel.open(620, 440, 2.0);
        assert!(panel.x >= 0.0 && panel.y >= 0.0);
        assert!(panel.x + panel.width <= 620.0);
        assert!(panel.y + panel.height() <= 440.0);
    }

    #[test]
    fn plugin_card_switch_and_detail_actions_have_separate_targets() {
        let mut panel = PreferencesPanel::new();
        panel.open(980, 640, 1.0);
        panel.page = PrefPage::Plugins;

        let buttons = panel.button_rects();
        let fish_card = buttons.iter().find(|rect| rect.action == PrefAction::OpenPlugin(Plugin::Fish)).unwrap();
        let fish_switch = buttons.iter().find(|rect| rect.action == PrefAction::ToggleFish).unwrap();
        assert_eq!(panel.action_at(fish_card.x + 10.0, fish_card.y + 10.0), Some(PrefAction::OpenPlugin(Plugin::Fish)));
        assert_eq!(panel.action_at(fish_switch.x + 10.0, fish_switch.y + 10.0), Some(PrefAction::ToggleFish));

        panel.plugin = Some(Plugin::Tgpt);
        let buttons = panel.button_rects();
        for action in [PrefAction::BackToPlugins, PrefAction::OpenPluginGithub(Plugin::Tgpt)] {
            let rect = buttons.iter().find(|rect| rect.action == action).unwrap();
            assert_eq!(panel.action_at(rect.x + 10.0, rect.y + 10.0), Some(action));
        }
        assert!(!buttons.iter().any(|rect| rect.action == PrefAction::OpenPlugin(Plugin::Fish)));
    }
}
use crate::terminal::Rgb;

pub const TEXT_SWATCHES: [Rgb; 8] = [
    Rgb::new(23, 26, 29), Rgb::new(242, 242, 242),
    Rgb::new(242, 226, 193), Rgb::new(103, 211, 237),
    Rgb::new(246, 189, 96), Rgb::new(145, 218, 142),
    Rgb::new(199, 167, 245), Rgb::new(244, 153, 166),
];
