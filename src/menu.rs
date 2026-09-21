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
    pub icon: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: MenuAction,
    pub separator_after: bool,
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
            width: 500.0,
            row_height: 52.0,
            hovered: None,
            entries: vec![
                MenuEntry { icon: "⧉", label: "Copy", shortcut: "Ctrl+Shift+C", action: MenuAction::Copy, separator_after: false },
                MenuEntry { icon: "▣", label: "Paste", shortcut: "Ctrl+Shift+V", action: MenuAction::Paste, separator_after: false },
                MenuEntry { icon: "☑", label: "Select All", shortcut: "", action: MenuAction::SelectAll, separator_after: true },
                MenuEntry { icon: "＋", label: "New Window", shortcut: "", action: MenuAction::NewWindow, separator_after: true },
                MenuEntry { icon: "A+", label: "Increase Font", shortcut: "", action: MenuAction::IncreaseFont, separator_after: false },
                MenuEntry { icon: "A−", label: "Decrease Font", shortcut: "", action: MenuAction::DecreaseFont, separator_after: false },
                MenuEntry { icon: "A", label: "Reset Font Size", shortcut: "", action: MenuAction::ResetFont, separator_after: false },
                MenuEntry { icon: "↺", label: "Clear Scrollback", shortcut: "", action: MenuAction::ClearScrollback, separator_after: true },
                MenuEntry { icon: "⚙", label: "Preferences…", shortcut: "", action: MenuAction::Preferences, separator_after: false },
                MenuEntry { icon: "✎", label: "Edit Hafþi Config", shortcut: "", action: MenuAction::EditConfig, separator_after: true },
                MenuEntry { icon: "×", label: "Quit", shortcut: "", action: MenuAction::Quit, separator_after: false },
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
        self.width = 500.0 * scale;
        self.row_height = row_height.max(1.0);
        let height = self.height();

        let margin = 8.0 * scale;
        let max_x = (surface_width as f32 - self.width - margin).max(margin);
        let max_y = (surface_height as f32 - height - margin).max(margin);

        self.x = x.min(max_x).max(margin);
        self.y = y.min(max_y).max(margin);
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
        self.hovered = self.index_at(x, y);
        old != self.hovered
    }

    pub fn action_at(&self, x: f32, y: f32) -> Option<MenuAction> {
        self.index_at(x, y)
            .and_then(|i| self.entries.get(i))
            .map(|entry| entry.action)
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

    pub fn shortcuts(&self) -> String {
        self.entries
            .iter()
            .map(|entry| entry.shortcut)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn icons(&self) -> String {
        self.entries
            .iter()
            .map(|entry| entry.icon)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
