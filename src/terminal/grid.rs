use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

#[derive(Clone)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn new(cols: usize, def_fg: [u8; 3], def_bg: [u8; 3]) -> Self {
        Self {
            cells: vec![Cell { c: ' ', fg: def_fg, bg: def_bg }; cols],
        }
    }

    pub fn clear(&mut self, def_fg: [u8; 3], def_bg: [u8; 3]) {
        for cell in self.cells.iter_mut() {
            cell.c = ' ';
            cell.fg = def_fg;
            cell.bg = def_bg;
        }
    }
}

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub visible: VecDeque<Row>,
    pub scrollback: VecDeque<Row>,
    pub max_scrollback: usize,
    pub scroll_offset: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub scroll_top: usize,
    pub scroll_bot: usize,
}

impl Grid {
    pub fn new(cols: usize, rows: usize, def_fg: [u8; 3], def_bg: [u8; 3]) -> Self {
        let mut visible = VecDeque::with_capacity(rows);
        for _ in 0..rows {
            visible.push_back(Row::new(cols, def_fg, def_bg));
        }

        Self {
            cols,
            rows,
            visible,
            scrollback: VecDeque::with_capacity(10_000),
            max_scrollback: 10_000,
            scroll_offset: 0,
            cursor_x: 0,
            cursor_y: 0,
            scroll_top: 0,
            scroll_bot: rows.saturating_sub(1),
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize, def_fg: [u8; 3], def_bg: [u8; 3]) {
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }

        let mut new_visible = VecDeque::with_capacity(new_rows);
        let copy_rows = self.rows.min(new_rows);

        for y in 0..copy_rows {
            let mut new_row = Row::new(new_cols, def_fg, def_bg);
            let copy_cols = self.cols.min(new_cols);
            for x in 0..copy_cols {
                new_row.cells[x] = self.visible[y].cells[x];
            }
            new_visible.push_back(new_row);
        }

        while new_visible.len() < new_rows {
            new_visible.push_back(Row::new(new_cols, def_fg, def_bg));
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.visible = new_visible;

        self.cursor_x = self.cursor_x.min(new_cols.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(new_rows.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bot = new_rows.saturating_sub(1);
    }

    pub fn get_cell(&self, x: usize, y: usize) -> &Cell {
        let x = x.min(self.cols.saturating_sub(1));
        
        if self.scroll_offset > 0 && y < self.scroll_offset {
            let sb_idx = self.scrollback.len().saturating_sub(self.scroll_offset).saturating_add(y);
            if sb_idx < self.scrollback.len() {
                return &self.scrollback[sb_idx].cells[x];
            }
        }
        
        let grid_y = y.saturating_sub(self.scroll_offset).min(self.rows.saturating_sub(1));
        &self.visible[grid_y].cells[x]
    }

    pub fn get_cell_mut(&mut self, x: usize, y: usize) -> &mut Cell {
        let y = y.min(self.rows.saturating_sub(1));
        let x = x.min(self.cols.saturating_sub(1));
        &mut self.visible[y].cells[x]
    }

    pub fn scroll_up(&mut self, def_fg: [u8; 3], fill_bg: [u8; 3]) {
        let top = self.scroll_top;
        let bot = self.scroll_bot.min(self.rows.saturating_sub(1));

        if top == 0 && bot == self.rows.saturating_sub(1) {
            let mut top_row = self.visible.pop_front().unwrap();
            
            if self.scrollback.len() >= self.max_scrollback {
                self.scrollback.pop_front();
            }
            self.scrollback.push_back(top_row.clone());

            top_row.clear(def_fg, fill_bg);
            self.visible.push_back(top_row);
        } else {
            let mut top_row = self.visible.remove(top).unwrap();
            top_row.clear(def_fg, fill_bg);
            self.visible.insert(bot, top_row);
        }
    }

    pub fn scroll_down(&mut self, def_fg: [u8; 3], fill_bg: [u8; 3]) {
        let top = self.scroll_top;
        let bot = self.scroll_bot.min(self.rows.saturating_sub(1));

        if top == 0 && bot == self.rows.saturating_sub(1) {
            let mut bot_row = self.visible.pop_back().unwrap();
            bot_row.clear(def_fg, fill_bg);
            self.visible.push_front(bot_row);
        } else {
            let mut bot_row = self.visible.remove(bot).unwrap();
            bot_row.clear(def_fg, fill_bg);
            self.visible.insert(top, bot_row);
        }
    }
}
