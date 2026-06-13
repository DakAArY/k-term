use std::{cell, collections::VecDeque, ops::MulAssign};

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

#[derive(Clone)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub is_wrapped: bool,
}

impl Row {
    pub fn new(cols: usize, def_fg: [u8; 3], def_bg: [u8; 3]) -> Self {
        Self {
            cells: vec![Cell { c: ' ', fg: def_fg, bg: def_bg }; cols],
            is_wrapped: false,
        }
    }

    pub fn clear(&mut self, def_fg: [u8; 3], def_bg: [u8; 3]) {
        for cell in self.cells.iter_mut() {
            cell.c = ' ';
            cell.fg = def_fg;
            cell.bg = def_bg;
        }
        self.is_wrapped = false;
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

    pub fn reflow(&mut self, new_cols: usize, new_rows: usize, def_fg: [u8; 3], def_bg: [u8; 3]) {
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }

        let mut logical_lines: Vec<Vec<Cell>> = Vec::new();
        let mut current_line = Vec::new();

        for row in self.scrollback.iter().chain(self.visible.iter()) {
            let mut actual_len = row.cells.len();
            if !row.is_wrapped {
                while actual_len > 0 {
                    let cell = &row.cells[actual_len - 1];
                    if cell.c != ' ' || cell.bg != def_bg || cell.fg != def_fg {
                        break;
                    }
                    actual_len -= 1;
                }
            }

            current_line.extend_from_slice(&row.cells[..actual_len]);

            if !row.is_wrapped {
                logical_lines.push(current_line);
                current_line = Vec::new();
            }
        }
        if !current_line.is_empty() {
            logical_lines.push(current_line);
        }

        let mut new_all_rows:  Vec<Row> = Vec::with_capacity(logical_lines.len());

        for line in logical_lines {
            if line.is_empty() {
                new_all_rows.push(Row::new(new_cols, def_fg, def_bg));
                continue;
            }

            let chunks: Vec<&[Cell]> = line.chunks(new_cols).collect();
            let chunk_count = chunks.len();

            for (i, chunk) in chunks.into_iter().enumerate() {
                let mut new_row = Row::new(new_cols, def_fg, def_bg);
                for (x, cell) in chunk.iter().enumerate() {
                    new_row.cells[x] = *cell; 
                }
                new_row.is_wrapped = i < chunk_count - 1;
                new_all_rows.push(new_row);
            }
        }

        let total_rows = new_all_rows.len();
        let visible_count = total_rows.min(new_rows);
        let scrollback_count = total_rows.saturating_sub(new_rows).min(self.max_scrollback);

        let mut new_visible = std::collections::VecDeque::with_capacity(new_rows);
        let mut new_scrollback = std::collections::VecDeque::with_capacity(self.max_scrollback);

        let scrollback_start = total_rows.saturating_sub(visible_count).saturating_sub(scrollback_count);
        for row in new_all_rows.into_iter().skip(scrollback_start) {
            if new_visible.len() < visible_count && new_scrollback.len() == scrollback_count {
                new_visible.push_back(row);
            } else if new_scrollback.len() < scrollback_count {
                new_scrollback.push_back(row);
            } else {
                new_visible.push_back(row);
            }
        }

        while new_visible.len() < new_rows {
            new_visible.push_back(Row::new(new_cols, def_fg, def_bg));
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.visible = new_visible;
        self.scrollback = new_scrollback;

        self.cursor_x = self.cursor_x.min(new_cols.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(new_rows.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bot = new_rows.saturating_sub(1);
    }
}
