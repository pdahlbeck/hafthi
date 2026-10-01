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
            Self::Plugins => "Integrations",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plugin {
    Fish,
    Starship,
    Tgpt,
    Sampler,
    Yazi,
    Micro,
    Yay,
}

impl Plugin {
    pub const ALL: [Self; 7] = [Self::Fish, Self::Starship, Self::Tgpt, Self::Sampler, Self::Yazi, Self::Micro, Self::Yay];

    pub fn title(self) -> &'static str {
        match self {
            Self::Fish => "Fish",
            Self::Starship => "Starship",
            Self::Tgpt => "tgpt",
            Self::Sampler => "Sampler",
            Self::Yazi => "Yazi",
            Self::Micro => "Micro",
            Self::Yay => "Yay",
        }
    }

    pub fn github_url(self) -> &'static str {
        match self {
            Self::Fish => "https://github.com/fish-shell/fish-shell",
            Self::Starship => "https://github.com/starship/starship",
            Self::Tgpt => "https://github.com/aandrew-me/tgpt",
            Self::Sampler => "https://github.com/sqshq/sampler",
            Self::Yazi => "https://github.com/sxyazi/yazi",
            Self::Micro => "https://github.com/micro-editor/micro",
            Self::Yay => "https://github.com/Jguer/yay",
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
    ToggleSampler,
    ToggleYazi,
    ToggleMicro,
    OpenSampler,
    OpenYazi,
    OpenMicro,
    InstallSampler,
    InstallYazi,
    InstallMicro,
    InstallYay,
    YayStable,
    YayDevelopment,
    EditSamplerConfig,
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
    pub plugin_error: String,
    pub yay_version: crate::packages::YayVersion,
    integration_scroll: f32,
    integration_drag_grab: Option<f32>,
}

impl PreferencesPanel {
    pub const WIDTH: f32 = 736.0;
    pub const HEIGHT: f32 = 476.0;
    pub const SIDEBAR_WIDTH: f32 = 192.0;
    pub const LIST_TOP: f32 = 130.0;
    pub const LIST_BOTTOM: f32 = 404.0;
    pub const CARD_HEIGHT: f32 = 56.0;
    pub const CARD_STEP: f32 = 66.0;

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
            plugin_error: String::new(),
            yay_version: crate::packages::YayVersion::Stable,
            integration_scroll: 0.0,
            integration_drag_grab: None,
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
        self.integration_drag_grab = None;
        self.hovered = None;
        self.hovered_page = None;
        self.color_editing = false;
        self.color_error = false;
        self.question_editing = false;
        self.plugin_error.clear();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.integration_drag_grab = None;
        self.hovered = None;
        self.hovered_page = None;
        self.color_editing = false;
        self.question_editing = false;
        self.plugin_error.clear();
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

    pub fn integration_list_visible(&self) -> bool {
        self.visible && self.page == PrefPage::Plugins && self.plugin.is_none()
    }

    pub fn integration_row_y(&self, index: usize) -> f32 {
        Self::LIST_TOP + index as f32 * Self::CARD_STEP - self.integration_scroll
    }

    fn integration_scroll_max(&self) -> f32 {
        (Plugin::ALL.len() as f32 * Self::CARD_STEP
            - (Self::CARD_STEP - Self::CARD_HEIGHT)
            - (Self::LIST_BOTTOM - Self::LIST_TOP)).max(0.0)
    }

    pub fn integration_viewport(&self) -> (f32, f32, f32, f32) {
        let (x, y) = self.pos(208.0, Self::LIST_TOP);
        (x, y, 500.0 * self.scale, (Self::LIST_BOTTOM - Self::LIST_TOP) * self.scale)
    }

    pub fn scroll_integrations(&mut self, x: f32, y: f32, logical_delta: f32) -> bool {
        if !self.integration_list_visible() || !logical_delta.is_finite() { return false; }
        let (vx, vy, vw, vh) = self.integration_viewport();
        if x < vx || x > vx + vw + 16.0 * self.scale || y < vy || y > vy + vh { return false; }
        let old = self.integration_scroll;
        self.integration_scroll = (old + logical_delta).clamp(0.0, self.integration_scroll_max());
        self.update_hover(x, y);
        old != self.integration_scroll
    }

    // Scrollbar geometry uses the same logical coordinates as the cards.
    pub fn integration_scrollbar(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.integration_list_visible() || self.integration_scroll_max() == 0.0 { return None; }
        let height = Self::LIST_BOTTOM - Self::LIST_TOP;
        let thumb_height = (height * height / (height + self.integration_scroll_max())).max(30.0);
        let y = Self::LIST_TOP + (height - thumb_height) * self.integration_scroll / self.integration_scroll_max();
        Some((714.0, y, 6.0, thumb_height))
    }

    pub fn begin_integration_scrollbar_drag(&mut self, x: f32, y: f32) -> bool {
        let Some((sx, sy, sw, sh)) = self.integration_scrollbar() else { return false; };
        let (x, y) = ((x - self.x) / self.scale, (y - self.y) / self.scale);
        if x < sx - 4.0 || x > sx + sw + 4.0 || y < Self::LIST_TOP || y > Self::LIST_BOTTOM { return false; }
        self.integration_drag_grab = Some(if y >= sy && y <= sy + sh { y - sy } else { sh / 2.0 });
        self.drag_integration_scrollbar(self.y + y * self.scale);
        true
    }

    pub fn drag_integration_scrollbar(&mut self, y: f32) -> bool {
        if !self.integration_list_visible() { self.integration_drag_grab = None; return false; }
        let Some(grab) = self.integration_drag_grab else { return false; };
        let Some((_, _, _, sh)) = self.integration_scrollbar() else { return false; };
        let travel = Self::LIST_BOTTOM - Self::LIST_TOP - sh;
        let old = self.integration_scroll;
        self.integration_scroll = (((y - self.y) / self.scale - Self::LIST_TOP - grab) / travel)
            .clamp(0.0, 1.0) * self.integration_scroll_max();
        old != self.integration_scroll
    }

    pub fn end_integration_scrollbar_drag(&mut self) {
        self.integration_drag_grab = None;
    }

    fn list_button_at(&self, action: PrefAction, y: f32) -> bool {
        if !self.integration_list_visible() || matches!(action, PrefAction::Save | PrefAction::Cancel) { return true; }
        let (_, top, _, height) = self.integration_viewport();
        y >= top && y <= top + height
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
                        let y = self.integration_row_y(index);
                        add(209.0, y, if plugin == Plugin::Yay { 495.0 } else { 404.0 }, Self::CARD_HEIGHT, OpenPlugin(plugin));
                        let toggle = match plugin {
                            Plugin::Fish => ToggleFish,
                            Plugin::Starship => ToggleStarship,
                            Plugin::Tgpt => ToggleCommandHelp,
                            Plugin::Sampler => ToggleSampler,
                            Plugin::Yazi => ToggleYazi,
                            Plugin::Micro => ToggleMicro,
                            Plugin::Yay => continue,
                        };
                        add(620.0, y + 11.0, 84.0, 34.0, toggle);
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
                        Plugin::Sampler => {
                            add(620.0, 125.0, 84.0, 34.0, ToggleSampler);
                            add(226.0, 252.0, 142.0, 34.0, OpenSampler);
                            add(377.0, 252.0, 157.0, 34.0, InstallSampler);
                            add(226.0, 302.0, 196.0, 34.0, EditSamplerConfig);
                        }
                        Plugin::Yazi => {
                            add(620.0, 125.0, 84.0, 34.0, ToggleYazi);
                            add(226.0, 252.0, 142.0, 34.0, OpenYazi);
                            add(377.0, 252.0, 157.0, 34.0, InstallYazi);
                        }
                        Plugin::Yay => {
                            add(226.0, 221.0, 130.0, 34.0, YayStable);
                            add(365.0, 221.0, 225.0, 34.0, YayDevelopment);
                            if crate::packages::yay_install_available() && crate::pty::installed_program("yay").is_none() {
                                add(226.0, 283.0, 150.0, 34.0, InstallYay);
                            }
                        }
                        Plugin::Micro => {
                            add(620.0, 125.0, 84.0, 34.0, ToggleMicro);
                            add(226.0, 252.0, 142.0, 34.0, OpenMicro);
                            add(377.0, 252.0, 157.0, 34.0, InstallMicro);
                        }
                    }
                }
            }
        }
        add(526.0, 430.0, 86.0, 34.0, Cancel);
        add(620.0, 430.0, 84.0, 34.0, Save);
        drop(add);
        if self.integration_list_visible() {
            let (_, top, _, height) = self.integration_viewport();
            buttons.retain(|rect| matches!(rect.action, Save | Cancel)
                || (rect.y < top + height && rect.y + rect.h > top));
        }
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
            .find(|rect| rect.contains(x, y) && self.list_button_at(rect.action, y))
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
            .find(|rect| rect.contains(x, y) && self.list_button_at(rect.action, y))
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
    fn integration_scroll_clips_clicks_and_preserves_header_footer_and_details() {
        for (width, height) in [(620, 440), (1816, 2316)] {
            let mut panel = PreferencesPanel::new();
            panel.open(width, height, 1.0);
            panel.page = PrefPage::Plugins;
            let (x, y, w, h) = panel.integration_viewport();
            let save = panel.button_rects().into_iter().find(|b| b.action == PrefAction::Save).unwrap();
            assert!(panel.integration_scrollbar().is_some());
            assert!(!panel.scroll_integrations(panel.x + 10.0, y + 10.0, 100.0));
            assert!(!panel.scroll_integrations(x + 10.0, y - 5.0, 100.0));
            assert!(panel.scroll_integrations(x + 10.0, y + 10.0, 25.0));
            assert_eq!(panel.action_at(x + 20.0, y - 5.0), None);
            assert_eq!(panel.action_at(x + 20.0, y + 5.0), Some(PrefAction::OpenPlugin(Plugin::Fish)));
            assert!(panel.scroll_integrations(x + 10.0, y + 10.0, 10000.0));
            assert_eq!(panel.integration_scroll, panel.integration_scroll_max());
            assert!(!panel.scroll_integrations(x + 10.0, y + 10.0, 10000.0));
            let yay = panel.button_rects().into_iter().find(|b| b.action == PrefAction::OpenPlugin(Plugin::Yay)).unwrap();
            assert!(yay.y >= y && yay.y + yay.h <= y + h + 0.01);
            assert_eq!(panel.action_at(yay.x + 10.0, yay.y + 10.0), Some(yay.action));
            assert_eq!(panel.action_at(x + 20.0, y + h + 3.0), None);
            assert_eq!(panel.action_at(save.x + 10.0, save.y + 10.0), Some(PrefAction::Save));
            assert_eq!(panel.button_rects().into_iter().find(|b| b.action == PrefAction::Save).unwrap().y, save.y);
            assert!(!panel.scroll_integrations(x + w / 2.0, y + 10.0, f32::NAN));
            panel.plugin = Some(Plugin::Yay);
            assert!(panel.integration_scrollbar().is_none());
            assert!(!panel.scroll_integrations(x + 10.0, y + 10.0, -100.0));
            panel.plugin = None;
            assert_eq!(panel.integration_scroll, panel.integration_scroll_max());
        }
    }

    #[test]
    fn integration_scrollbar_drag_clamps_and_stops_on_release() {
        let mut panel = PreferencesPanel::new();
        panel.open(980, 640, 1.0);
        panel.page = PrefPage::Plugins;
        let (x, y, _, _) = panel.integration_scrollbar().unwrap();
        let (px, py) = panel.pos(x + 3.0, y + 5.0);
        assert!(panel.begin_integration_scrollbar_drag(px, py));
        assert!(panel.drag_integration_scrollbar(panel.y + 2000.0 * panel.scale));
        assert_eq!(panel.integration_scroll, panel.integration_scroll_max());
        assert!(panel.drag_integration_scrollbar(panel.y));
        assert_eq!(panel.integration_scroll, 0.0);
        panel.end_integration_scrollbar_drag();
        assert!(!panel.drag_integration_scrollbar(py + 100.0));
        let (px, py) = panel.pos(x + 3.0, PreferencesPanel::LIST_BOTTOM - 1.0);
        assert!(panel.begin_integration_scrollbar_drag(px, py));
        assert!(panel.integration_scroll > 0.0);
        panel.close();
        assert!(!panel.drag_integration_scrollbar(py));
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

        let (vx, vy, _, _) = panel.integration_viewport();
        assert!(panel.scroll_integrations(vx + 10.0, vy + 10.0, 10000.0));
        let buttons = panel.button_rects();

        let yazi_card = buttons.iter().find(|rect| rect.action == PrefAction::OpenPlugin(Plugin::Yazi)).unwrap();
        let yazi_switch = buttons.iter().find(|rect| rect.action == PrefAction::ToggleYazi).unwrap();
        assert!(yazi_card.y + yazi_card.h < panel.y + panel.height());
        assert_eq!(panel.action_at(yazi_card.x + 10.0, yazi_card.y + 10.0), Some(PrefAction::OpenPlugin(Plugin::Yazi)));
        assert_eq!(panel.action_at(yazi_switch.x + 10.0, yazi_switch.y + 10.0), Some(PrefAction::ToggleYazi));

        let micro_card = buttons.iter().find(|rect| rect.action == PrefAction::OpenPlugin(Plugin::Micro)).unwrap();
        let micro_switch = buttons.iter().find(|rect| rect.action == PrefAction::ToggleMicro).unwrap();
        assert!(micro_card.y + micro_card.h < panel.y + panel.height());
        assert_eq!(panel.action_at(micro_card.x + 10.0, micro_card.y + 10.0), Some(PrefAction::OpenPlugin(Plugin::Micro)));
        assert_eq!(panel.action_at(micro_switch.x + 10.0, micro_switch.y + 10.0), Some(PrefAction::ToggleMicro));

        let cards: Vec<_> = buttons.iter().filter(|rect| matches!(rect.action, PrefAction::OpenPlugin(_))).collect();
        let save = buttons.iter().find(|rect| rect.action == PrefAction::Save).unwrap();
        for pair in cards.windows(2) { assert!(pair[0].y + pair[0].h < pair[1].y); }
        let yay = cards.iter().find(|rect| rect.action == PrefAction::OpenPlugin(Plugin::Yay)).unwrap();
        assert!(yay.y + yay.h < save.y);
        assert_eq!(panel.action_at(yay.x + yay.w - 10.0, yay.y + 10.0), Some(PrefAction::OpenPlugin(Plugin::Yay)));
        panel.plugin = Some(Plugin::Yay);
        for action in [PrefAction::YayStable, PrefAction::YayDevelopment] {
            let rect = panel.button_rects().into_iter().find(|rect| rect.action == action).unwrap();
            assert_eq!(panel.action_at(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0), Some(action));
        }
        assert_eq!(panel.yay_version, crate::packages::YayVersion::Stable);
        let link = panel.button_rects().into_iter().find(|rect| rect.action == PrefAction::OpenPluginGithub(Plugin::Yay)).unwrap();
        assert_eq!(panel.action_at(link.x + 10.0, link.y + 10.0), Some(link.action));

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
