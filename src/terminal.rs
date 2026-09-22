use std::collections::VecDeque;
use vte::{Params, Parser, Perform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CellStyle {
    pub fg: Rgb,
    pub bold: bool,
    pub uses_default_fg: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TerminalCell {
    pub ch: char,
    pub style: CellStyle,
}

pub struct TerminalGrid {
    cols: usize,
    rows: usize,
    cursor_x: usize,
    cursor_y: usize,
    cells: Vec<TerminalCell>,
    history: VecDeque<Vec<TerminalCell>>,
    max_history: usize,
    view_offset: usize,
    parser: Parser,
    dirty: bool,
    current_style: CellStyle,
    default_fg: Rgb,
    ansi: [Rgb; 16],
}

const DEFAULT_FG: Rgb = Rgb::new(245, 245, 245);

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: DEFAULT_FG,
            bold: false,
            uses_default_fg: true,
        }
    }
}

impl TerminalGrid {
    pub fn new_with_theme(
        cols: usize,
        rows: usize,
        default_fg: Rgb,
        ansi: [Rgb; 16],
        max_history: usize,
    ) -> Self {
        let style = CellStyle {
            fg: default_fg,
            bold: false,
            uses_default_fg: true,
        };
        Self {
            cols,
            rows,
            cursor_x: 0,
            cursor_y: 0,
            cells: vec![TerminalCell { ch: ' ', style }; cols * rows],
            history: VecDeque::new(),
            max_history,
            view_offset: 0,
            parser: Parser::new(),
            dirty: true,
            current_style: style,
            default_fg,
            ansi,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let mut parser = std::mem::replace(&mut self.parser, Parser::new());
        for byte in bytes {
            parser.advance(self, *byte);
        }
        self.parser = parser;
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    pub fn cursor(&self) -> (usize, usize) {
        (
            self.cursor_x.min(self.cols.saturating_sub(1)),
            self.cursor_y.min(self.rows.saturating_sub(1)),
        )
    }

    pub fn cursor_visible(&self) -> bool {
        self.view_offset == 0
    }

    pub fn select_all_visible(&self) -> ((usize, usize), (usize, usize)) {
        (
            (0, 0),
            (self.cols.saturating_sub(1), self.rows.saturating_sub(1)),
        )
    }

    pub fn clear_scrollback(&mut self) {
        self.history.clear();
        self.view_offset = 0;
        self.dirty = true;
    }

    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.max_history = limit.max(100);
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
        self.view_offset = self.view_offset.min(self.history.len());
        self.dirty = true;
    }

    pub fn set_default_foreground(&mut self, color: Rgb) {
        self.default_fg = color;
        if self.current_style.uses_default_fg {
            self.current_style.fg = color;
        }
        for cell in &mut self.cells {
            if cell.style.uses_default_fg {
                cell.style.fg = color;
            }
        }
        for row in &mut self.history {
            for cell in row {
                if cell.style.uses_default_fg {
                    cell.style.fg = color;
                }
            }
        }
        self.dirty = true;
    }

    pub fn scroll_view(&mut self, delta_rows: isize) {
        if delta_rows > 0 {
            self.view_offset = (self.view_offset + delta_rows as usize).min(self.history.len());
        } else if delta_rows < 0 {
            self.view_offset = self
                .view_offset
                .saturating_sub(delta_rows.unsigned_abs());
        }
        self.dirty = true;
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.view_offset != 0 {
            self.view_offset = 0;
            self.dirty = true;
        }
    }

    fn current_row(&self, y: usize) -> &[TerminalCell] {
        let start = y * self.cols;
        &self.cells[start..start + self.cols]
    }

    fn visible_rows(&self) -> Vec<Vec<TerminalCell>> {
        let total = self.history.len() + self.rows;
        let end = total.saturating_sub(self.view_offset);
        let start = end.saturating_sub(self.rows);
        let mut out = Vec::with_capacity(self.rows);

        for absolute in start..end {
            if absolute < self.history.len() {
                out.push(self.history[absolute].clone());
            } else {
                let y = absolute - self.history.len();
                if y < self.rows {
                    out.push(self.current_row(y).to_vec());
                }
            }
        }

        let blank = vec![self.blank_default(); self.cols];
        while out.len() < self.rows {
            out.insert(0, blank.clone());
        }
        out
    }

    pub fn styled_runs(&self) -> Vec<(String, CellStyle)> {
        let mut runs: Vec<(String, CellStyle)> = Vec::new();
        let visible = self.visible_rows();

        for (y, row) in visible.iter().enumerate() {
            for x in 0..self.cols {
                let cell = row[x];
                let ch = cell.ch;
                let style = cell.style;

                match runs.last_mut() {
                    Some((text, last_style))
                        if last_style.fg == style.fg && last_style.bold == style.bold =>
                    {
                        text.push(ch);
                    }
                    _ => runs.push((ch.to_string(), style)),
                }
            }

            if y + 1 < self.rows {
                if let Some((text, _)) = runs.last_mut() {
                    text.push('\n');
                } else {
                    runs.push(("\n".to_string(), CellStyle::default()));
                }
            }
        }

        runs
    }

    pub fn selected_text(
        &self,
        start: (usize, usize),
        end: (usize, usize),
    ) -> String {
        let visible = self.visible_rows();
        let (mut x1, mut y1) = start;
        let (mut x2, mut y2) = end;

        if (y2, x2) < (y1, x1) {
            std::mem::swap(&mut x1, &mut x2);
            std::mem::swap(&mut y1, &mut y2);
        }

        y1 = y1.min(self.rows.saturating_sub(1));
        y2 = y2.min(self.rows.saturating_sub(1));
        x1 = x1.min(self.cols.saturating_sub(1));
        x2 = x2.min(self.cols.saturating_sub(1));

        let mut out = String::new();

        for y in y1..=y2 {
            let start_x = if y == y1 { x1 } else { 0 };
            let end_x = if y == y2 { x2 } else { self.cols.saturating_sub(1) };

            let mut line: String = visible[y][start_x..=end_x]
                .iter()
                .map(|cell| cell.ch)
                .collect();

            while line.ends_with(' ') {
                line.pop();
            }

            out.push_str(&line);
            if y != y2 {
                out.push('\n');
            }
        }

        out
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);

        if cols == self.cols && rows == self.rows {
            return;
        }

        let old_cols = self.cols;
        let old_rows = self.rows;
        let old_cursor_y = self.cursor_y;

        let blank = self.blank_default();
        let mut new_cells = vec![blank; cols * rows];
        let copy_rows = rows.min(old_rows);
        let copy_cols = cols.min(old_cols);

        // A newly started terminal must remain anchored at the top-left.
        // Once terminal output has progressed beyond the first row (or there is
        // scrollback), preserve the existing bottom-anchored resize behaviour.
        let initial_screen = self.history.is_empty() && self.cursor_y == 0;
        let (old_first, new_first) = if initial_screen {
            (0, 0)
        } else {
            (
                old_rows.saturating_sub(copy_rows),
                rows.saturating_sub(copy_rows),
            )
        };

        for i in 0..copy_rows {
            let old_y = old_first + i;
            let new_y = new_first + i;
            let old_start = old_y * old_cols;
            let new_start = new_y * cols;

            new_cells[new_start..new_start + copy_cols]
                .copy_from_slice(&self.cells[old_start..old_start + copy_cols]);
        }

        for row in &mut self.history {
            row.resize(cols, blank);
            row.truncate(cols);
        }

        self.cols = cols;
        self.rows = rows;
        self.cells = new_cells;
        self.cursor_x = self.cursor_x.min(cols.saturating_sub(1));

        self.cursor_y = if old_cursor_y >= old_first {
            new_first + (old_cursor_y - old_first)
        } else {
            new_first
        }
        .min(rows.saturating_sub(1));

        self.view_offset = self.view_offset.min(self.history.len());
        self.dirty = true;
    }

    fn blank_default(&self) -> TerminalCell {
        TerminalCell {
            ch: ' ',
            style: CellStyle {
                fg: self.default_fg,
                bold: false,
                uses_default_fg: true,
            },
        }
    }

    fn blank_cell(&self) -> TerminalCell {
        TerminalCell {
            ch: ' ',
            style: self.current_style,
        }
    }

    fn push_history_row(&mut self, row: Vec<TerminalCell>) {
        self.history.push_back(row);

        if self.view_offset > 0 {
            self.view_offset = (self.view_offset + 1).min(self.history.len());
        }

        while self.history.len() > self.max_history {
            self.history.pop_front();
            self.view_offset = self.view_offset.min(self.history.len());
        }
    }

    fn newline(&mut self) {
        self.cursor_x = 0;

        if self.cursor_y + 1 < self.rows {
            self.cursor_y += 1;
        } else {
            let first_row = self.current_row(0).to_vec();
            self.push_history_row(first_row);

            let row = self.cols;
            self.cells.copy_within(row.., 0);
            let start = self.cells.len() - row;
            let blank = self.blank_cell();
            for cell in &mut self.cells[start..] {
                *cell = blank;
            }
        }

        self.dirty = true;
    }

    fn erase_line_from_cursor(&mut self) {
        let start = self.cursor_y * self.cols + self.cursor_x.min(self.cols);
        let end = ((self.cursor_y + 1) * self.cols).min(self.cells.len());
        let blank = self.blank_cell();
        for cell in &mut self.cells[start..end] {
            *cell = blank;
        }
        self.dirty = true;
    }

    fn set_ansi_index(&mut self, idx: usize) {
        if let Some(color) = self.ansi.get(idx).copied() {
            self.current_style.fg = color;
            self.current_style.uses_default_fg = false;
        }
    }

    fn ansi256(&self, index: u16) -> Rgb {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                let n = index - 16;
                let r = (n / 36) % 6;
                let g = (n / 6) % 6;
                let b = n % 6;
                let map = |v: u16| -> u8 {
                    if v == 0 { 0 } else { (55 + 40 * v) as u8 }
                };
                Rgb::new(map(r), map(g), map(b))
            }
            232..=255 => {
                let v = (8 + (index - 232) * 10) as u8;
                Rgb::new(v, v, v)
            }
            _ => self.default_fg,
        }
    }

    fn sgr(&mut self, params: &Params) {
        let values: Vec<u16> = params
            .iter()
            .flat_map(|sub| sub.iter().copied())
            .collect();

        let values = if values.is_empty() { vec![0] } else { values };
        let mut i = 0;

        while i < values.len() {
            match values[i] {
                0 => self.current_style = CellStyle {
                    fg: self.default_fg,
                    bold: false,
                    uses_default_fg: true,
                },
                1 => self.current_style.bold = true,
                22 => self.current_style.bold = false,
                30..=37 => self.set_ansi_index((values[i] - 30) as usize),
                39 => {
                    self.current_style.fg = self.default_fg;
                    self.current_style.uses_default_fg = true;
                }
                90..=97 => self.set_ansi_index((values[i] - 90 + 8) as usize),
                38 => {
                    if i + 2 < values.len() && values[i + 1] == 5 {
                        self.current_style.fg = self.ansi256(values[i + 2]);
                        self.current_style.uses_default_fg = false;
                        i += 2;
                    } else if i + 4 < values.len() && values[i + 1] == 2 {
                        self.current_style.fg = Rgb::new(
                            values[i + 2].min(255) as u8,
                            values[i + 3].min(255) as u8,
                            values[i + 4].min(255) as u8,
                        );
                        self.current_style.uses_default_fg = false;
                        i += 4;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        self.dirty = true;
    }
}

impl Perform for TerminalGrid {
    fn print(&mut self, c: char) {
        if self.cursor_x >= self.cols {
            self.newline();
        }

        let idx = self.cursor_y * self.cols + self.cursor_x;
        if idx < self.cells.len() {
            self.cells[idx] = TerminalCell {
                ch: c,
                style: self.current_style,
            };
            self.cursor_x += 1;
            self.dirty = true;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => {
                self.cursor_x = 0;
                self.dirty = true;
            }
            0x08 => {
                self.cursor_x = self.cursor_x.saturating_sub(1);
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let first = params
            .iter()
            .next()
            .and_then(|p| p.first())
            .copied()
            .unwrap_or(1) as usize;

        match action {
            'm' => self.sgr(params),
            'A' => self.cursor_y = self.cursor_y.saturating_sub(first.max(1)),
            'B' => self.cursor_y = (self.cursor_y + first.max(1)).min(self.rows.saturating_sub(1)),
            'C' => self.cursor_x = (self.cursor_x + first.max(1)).min(self.cols.saturating_sub(1)),
            'D' => self.cursor_x = self.cursor_x.saturating_sub(first.max(1)),
            'G' => self.cursor_x = first.saturating_sub(1).min(self.cols.saturating_sub(1)),
            'H' | 'f' => {
                let mut iter = params.iter();
                let row = iter
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                let col = iter
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                self.cursor_y = row.saturating_sub(1).min(self.rows.saturating_sub(1));
                self.cursor_x = col.saturating_sub(1).min(self.cols.saturating_sub(1));
            }
            'J' => {
                // CSI J defaults to 0: erase from the cursor onward.
                // Shells use this while repainting a prompt; clearing the
                // entire screen here loses command output above the cursor.
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(0);
                let cursor = self.cursor_y * self.cols + self.cursor_x;
                let blank = self.blank_cell();
                match mode {
                    0 => {
                        for cell in &mut self.cells[cursor.min(self.cells.len())..] {
                            *cell = blank;
                        }
                    }
                    1 => {
                        let end = cursor.saturating_add(1).min(self.cells.len());
                        for cell in &mut self.cells[..end] {
                            *cell = blank;
                        }
                    }
                    2 => self.cells.fill(blank),
                    3 => self.history.clear(),
                    _ => {}
                }
            }
            'K' => self.erase_line_from_cursor(),
            _ => return,
        }

        self.dirty = true;
    }
}

#[cfg(test)]
mod foreground_tests {
    use super::*;

    #[test]
    fn partial_screen_erase_preserves_previous_command_output() {
        let fg = Rgb::new(245, 245, 245);
        let mut grid = TerminalGrid::new_with_theme(10, 4, fg, [fg; 16], 100);
        grid.feed(b"old output\r\nnew prompt\x1b[J");
        assert_eq!(grid.current_row(0).iter().map(|cell| cell.ch).collect::<String>(), "old output");
        assert_eq!(grid.current_row(1).iter().map(|cell| cell.ch).collect::<String>(), "new prompt");
        assert_eq!(grid.cursor(), (0, 2));
    }

    #[test]
    fn screen_erase_modes_keep_cursor_and_erase_only_requested_cells() {
        let fg = Rgb::new(245, 245, 245);
        let mut grid = TerminalGrid::new_with_theme(5, 2, fg, [fg; 16], 100);
        grid.feed(b"abcde\r\nfghij\x1b[1;3H\x1b[0J");
        assert_eq!(grid.current_row(0).iter().map(|cell| cell.ch).collect::<String>(), "abcde");
        assert_eq!(grid.current_row(1).iter().map(|cell| cell.ch).collect::<String>(), "fg   ");
        assert_eq!(grid.cursor(), (2, 0));
        grid.feed(b"\x1b[1J");
        assert_eq!(grid.current_row(0).iter().map(|cell| cell.ch).collect::<String>(), "   de");
        grid.feed(b"\x1b[2J");
        assert!(grid.cells.iter().all(|cell| cell.ch == ' '));
        assert_eq!(grid.cursor(), (2, 0));
    }

    #[test]
    fn changing_default_color_keeps_explicit_ansi_colors() {
        let default = Rgb::new(10, 20, 30);
        let red = Rgb::new(200, 30, 40);
        let mut ansi = [default; 16];
        ansi[1] = red;
        let mut grid = TerminalGrid::new_with_theme(8, 2, default, ansi, 100);
        grid.feed(b"A\x1b[31mR\x1b[0mZ");
        let changed = Rgb::new(230, 220, 210);
        grid.set_default_foreground(changed);
        assert_eq!(grid.cells[0].style.fg, changed);
        assert_eq!(grid.cells[1].style.fg, red);
        assert_eq!(grid.cells[2].style.fg, changed);
        grid.feed(b"N");
        assert_eq!(grid.cells[3].style.fg, changed);
    }
}
