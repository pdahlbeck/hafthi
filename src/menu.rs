#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Copy,
    Paste,
    SelectAll,
    NewWindow,
    IncreaseFont,
    DecreaseFont,
    ResetFont,
    ClearScrollback,
    Preferences,
    EditConfig,
    Quit,
}

#[derive(Debug, Clone)]
pub struct MenuEntry {
    pub label: &'static str,
    pub action: Option<MenuAction>,
}

pub struct ContextMenu {
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub row_height: f32,
    pub hovered: Option<usize>,
    pub entries: Vec<MenuEntry>,
}

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            visible: false,
            x: 0.0,
            y: 0.0,
            width: 340.0,
            row_height: 42.0,
            hovered: None,
            entries: vec![
                MenuEntry { label: "Copy", action: Some(MenuAction::Copy) },
                MenuEntry { label: "Paste", action: Some(MenuAction::Paste) },
                MenuEntry { label: "Select All", action: Some(MenuAction::SelectAll) },
                MenuEntry { label: "────────────", action: None },
                MenuEntry { label: "New Window", action: Some(MenuAction::NewWindow) },
                MenuEntry { label: "────────────", action: None },
                MenuEntry { label: "Increase Font", action: Some(MenuAction::IncreaseFont) },
                MenuEntry { label: "Decrease Font", action: Some(MenuAction::DecreaseFont) },
                MenuEntry { label: "Reset Font Size", action: Some(MenuAction::ResetFont) },
                MenuEntry { label: "Clear Scrollback", action: Some(MenuAction::ClearScrollback) },
                MenuEntry { label: "────────────", action: None },
                MenuEntry { label: "Preferences…", action: Some(MenuAction::Preferences) },
                MenuEntry { label: "Edit Hafthi Config", action: Some(MenuAction::EditConfig) },
                MenuEntry { label: "Quit", action: Some(MenuAction::Quit) },
            ],
        }
    }

    pub fn open(
        &mut self,
        x: f32,
        y: f32,
        surface_width: u32,
        surface_height: u32,
        scale: f32,
        row_height: f32,
    ) {
        self.width = 300.0 * scale;
        self.row_height = row_height.max(1.0);
        let height = self.height();

        let max_x = (surface_width as f32 - self.width - 4.0).max(0.0);
        let max_y = (surface_height as f32 - height - 4.0).max(0.0);

        self.x = x.min(max_x).max(0.0);
        self.y = y.min(max_y).max(0.0);
        self.visible = true;
        self.hovered = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.hovered = None;
    }

    pub fn height(&self) -> f32 {
        self.row_height * self.entries.len() as f32
    }

    pub fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let old = self.hovered;
        self.hovered = self.index_at(x, y).filter(|&i| self.entries[i].action.is_some());
        old != self.hovered
    }

    pub fn action_at(&self, x: f32, y: f32) -> Option<MenuAction> {
        self.index_at(x, y)
            .and_then(|i| self.entries.get(i))
            .and_then(|entry| entry.action)
    }

    fn index_at(&self, x: f32, y: f32) -> Option<usize> {
        if !self.visible
            || x < self.x
            || x > self.x + self.width
            || y < self.y
            || y > self.y + self.height()
        {
            return None;
        }

        let index = ((y - self.y) / self.row_height).floor() as usize;
        (index < self.entries.len()).then_some(index)
    }

    pub fn text(&self) -> String {
        self.entries
            .iter()
            .map(|entry| entry.label)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
