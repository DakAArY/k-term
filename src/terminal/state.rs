use unicode_width::UnicodeWidthChar;
use vte::Perform;
use super::grid::{Cell, Grid};

fn color_256_to_rgb(color: u16) -> [u8; 3] {
    match color {
        0  => [0,   0,   0  ],
        1  => [205, 49,  49 ],
        2  => [13,  188, 121],
        3  => [229, 229, 16 ],
        4  => [36,  114, 200],
        5  => [188, 63,  188],
        6  => [17,  168, 205],
        7  => [229, 229, 229],
        8  => [102, 102, 102],
        9  => [241, 76,  76 ],
        10 => [35,  209, 139],
        11 => [245, 245, 67 ],
        12 => [59,  142, 234],
        13 => [214, 112, 214],
        14 => [41,  184, 219],
        15 => [255, 255, 255],
        16..=231 => {
            let n = color - 16;
            let r = if (n / 36) == 0 { 0 } else { (n / 36) * 40 + 55 };
            let g = if ((n % 36) / 6) == 0 { 0 } else { ((n % 36) / 6) * 40 + 55 };
            let b = if (n % 6) == 0 { 0 } else { (n % 6) * 40 + 55 };
            [r as u8, g as u8, b as u8]
        }
        232..=255 => {
            let gray = (color - 232) * 10 + 8;
            [gray as u8, gray as u8, gray as u8]
        }
        _ => [255, 255, 255],
    }
}

fn basic_color(code: u16) -> [u8; 3] {
    match code {
        30 | 40  => [0,   0,   0  ],
        31 | 41  => [205, 49,  49 ],
        32 | 42  => [13,  188, 121],
        33 | 43  => [229, 229, 16 ],
        34 | 44  => [36,  114, 200],
        35 | 45  => [188, 63,  188],
        36 | 46  => [17,  168, 205],
        37 | 47  => [229, 229, 229],
        90 | 100 => [102, 102, 102],
        91 | 101 => [241, 76,  76 ],
        92 | 102 => [35,  209, 139],
        93 | 103 => [245, 245, 67 ],
        94 | 104 => [59,  142, 234],
        95 | 105 => [214, 112, 214],
        96 | 106 => [41,  184, 219],
        97 | 107 => [255, 255, 255],
        _        => [255, 255, 255],
    }
}

fn brighten(color: [u8; 3]) -> [u8; 3] {
    match color {
        [0,   0,   0  ] => [102, 102, 102],
        [205, 49,  49 ] => [241, 76,  76 ],
        [13,  188, 121] => [35,  209, 139],
        [229, 229, 16 ] => [245, 245, 67 ],
        [36,  114, 200] => [59,  142, 234],
        [188, 63,  188] => [214, 112, 214],
        [17,  168, 205] => [41,  184, 219],
        [229, 229, 229] => [255, 255, 255],
        other           => other,
    }
}

pub struct Terminal {
    pub cols: usize,
    pub rows: usize,
    pub primary: Grid,
    pub alt: Grid,
    pub use_alt_screen: bool,
    pub dirty: bool,
    pub current_fg: [u8; 3],
    pub current_bg: [u8; 3],
    pub default_fg: [u8; 3],
    pub default_bg: [u8; 3],
    pub hide_cursor: bool,
    pub saved_cursor_x: usize,
    pub saved_cursor_y: usize,
    bold: bool,
    pending_wrap: bool,
}

impl Terminal {
    pub fn new(cols: usize, rows: usize, def_fg: [u8; 3], def_bg: [u8; 3]) -> Self {
        Self {
            cols,
            rows,
            primary: Grid::new(cols, rows, def_fg, def_bg),
            alt: Grid::new(cols, rows, def_fg, def_bg),
            use_alt_screen: false,
            dirty: true,
            current_fg: def_fg,
            current_bg: def_bg,
            default_fg: def_fg,
            default_bg: def_bg,
            hide_cursor: false,
            saved_cursor_x: 0,
            saved_cursor_y: 0,
            bold: false,
            pending_wrap: false,
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        self.cols = new_cols;
        self.rows = new_rows;
        self.primary.reflow(new_cols, new_rows, self.default_fg, self.default_bg);
        self.alt.reflow(new_cols, new_rows, self.default_fg, self.default_bg);
        self.dirty = true;
    }

    pub fn is_cursor(&self, x: usize, y: usize) -> bool {
        let screen = if self.use_alt_screen { &self.alt } else { &self.primary };
        screen.scroll_offset == 0 && screen.cursor_x == x && screen.cursor_y == y
    }

    pub fn get_cell(&self, x: usize, y: usize) -> &Cell {
        let screen = if self.use_alt_screen { &self.alt } else { &self.primary };
        screen.get_cell(x, y)
    }

    pub fn scroll(&mut self, lines: isize) {
        if self.use_alt_screen { return; }
        let max_offset = self.primary.scrollback.len();
        let current = self.primary.scroll_offset as isize;
        let new_offset = (current + lines).clamp(0, max_offset as isize) as usize;
        if new_offset != self.primary.scroll_offset {
            self.primary.scroll_offset = new_offset;
            self.dirty = true;
        }
    }

    pub fn snap_to_bottom(&mut self) {
        if self.primary.scroll_offset > 0 {
            self.primary.scroll_offset = 0;
            self.dirty = true;
        }
    }

    fn effective_fg(&self) -> [u8; 3] {
        if self.bold {
            brighten(self.current_fg)
        } else {
            self.current_fg
        }
    }

    fn handle_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.current_fg = self.default_fg;
            self.current_bg = self.default_bg;
            self.bold = false;
            return;
        }
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => {
                    self.current_fg = self.default_fg;
                    self.current_bg = self.default_bg;
                    self.bold = false;
                }
                1 => { self.bold = true; }
                2 | 22 => { self.bold = false; }
                3 | 4 | 5 | 6 | 7 | 21 | 23 | 24 | 25 | 27 | 28 => {}
                39 => self.current_fg = self.default_fg,
                49 => self.current_bg = self.default_bg,
                30..=37 | 90..=97 => self.current_fg = basic_color(params[i]),
                40..=47 | 100..=107 => self.current_bg = basic_color(params[i]),
                38 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.current_fg = color_256_to_rgb(params[i + 2]);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        self.current_fg = [params[i + 2] as u8, params[i + 3] as u8, params[i + 4] as u8];
                        i += 4;
                    }
                }
                48 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.current_bg = color_256_to_rgb(params[i + 2]);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        self.current_bg = [params[i + 2] as u8, params[i + 3] as u8, params[i + 4] as u8];
                        i += 4;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        let cols = self.cols;
        let rows = self.rows;
        let fg = self.effective_fg();
        let bg = self.current_bg;
        let char_width = c.width().unwrap_or(1).max(1);
        let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };

        if self.pending_wrap {
            self.pending_wrap = false;
            screen.visible[screen.cursor_y].is_wrapped = true;
            screen.cursor_x = 0;
            if screen.cursor_y < rows - 1 {
                screen.cursor_y += 1;
            } else {
                screen.scroll_up(self.default_fg, bg);
            }
        }

        if screen.cursor_x + char_width > cols {
            screen.visible[screen.cursor_y].is_wrapped = true;
            screen.cursor_x = 0;
            if screen.cursor_y < rows - 1 {
                screen.cursor_y += 1;
            } else {
                screen.scroll_up(self.default_fg, bg);
            }
        }

        if screen.cursor_y < rows && screen.cursor_x < cols {
            let cell = screen.get_cell_mut(screen.cursor_x, screen.cursor_y);
            cell.c = c;
            cell.fg = fg;
            cell.bg = bg;

            if char_width == 2 && screen.cursor_x + 1 < cols {
                let dummy = screen.get_cell_mut(screen.cursor_x + 1, screen.cursor_y);
                dummy.c = '\0';
                dummy.fg = fg;
                dummy.bg = bg;
            }

            screen.cursor_x += char_width;

            if screen.cursor_x >= cols {
                screen.cursor_x = cols - 1;
                self.pending_wrap = true;
            }
        }
        self.dirty = true;
    }

    fn execute(&mut self, byte: u8) {
        self.pending_wrap = false;
        let rows = self.rows;
        let cols = self.cols;
        let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };

        match byte {
            9 => {
                let next_tab = ((screen.cursor_x / 8) + 1) * 8;
                screen.cursor_x = next_tab.min(cols.saturating_sub(1));
            }
            10 | 11 | 12 => {
                let bot = screen.scroll_bot.min(rows - 1);
                if screen.cursor_y < bot {
                    screen.cursor_y += 1;
                } else if screen.cursor_y == bot {
                    let def_fg = self.default_fg;
                    let fill_bg = self.current_bg;
                    screen.scroll_up(def_fg, fill_bg);
                } else {
                    if screen.cursor_y < rows - 1 {
                        screen.cursor_y += 1;
                    }
                }
            }
            13 => { screen.cursor_x = 0; }
            8 => {
                if screen.cursor_x > 0 {
                    screen.cursor_x -= 1;
                }
            }
            7 | 127 => {}
            _ => {}
        }
        self.dirty = true;
    }

    fn csi_dispatch(&mut self, params: &consts::Params, intermediates: &[u8], _ignore: bool, action: char) {
        let mut args = params.iter().map(|param| param[0]);
        let is_dec_private = intermediates.first() == Some(&b'?');
        self.pending_wrap = false;

        if is_dec_private {
            match action {
                'h' => {
                    for mode in args {
                        match mode {
                            25   => { self.hide_cursor = false; }
                            1049 => {
                                self.saved_cursor_x = self.primary.cursor_x;
                                self.saved_cursor_y = self.primary.cursor_y;
                                self.use_alt_screen = true;
                                self.alt = Grid::new(self.cols, self.rows, self.default_fg, self.default_bg);
                            }
                            1    => {}
                            7    => {}
                            _    => {}
                        }
                    }
                    self.dirty = true;
                    return;
                }
                'l' => {
                    for mode in args {
                        match mode {
                            25   => { self.hide_cursor = true; }
                            1049 => {
                                self.use_alt_screen = false;
                                self.primary.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                                self.primary.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
                            }
                            1    => {}
                            7    => {}
                            _    => {}
                        }
                    }
                    self.dirty = true;
                    return;
                }
                _ => {}
            }
        }

        let cols = self.cols;
        let rows = self.rows;
        let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };

        match action {
            'K' => {
                let cy = screen.cursor_y;
                let fill_fg = self.current_fg;
                let fill_bg = self.current_bg;
                match args.next().unwrap_or(0) {
                    0 => for x in screen.cursor_x..cols {
                        let cell = screen.get_cell_mut(x, cy);
                        cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                    },
                    1 => for x in 0..=screen.cursor_x {
                        let cell = screen.get_cell_mut(x, cy);
                        cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                    },
                    2 => for x in 0..cols {
                        let cell = screen.get_cell_mut(x, cy);
                        cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                    },
                    _ => {}
                }
            }
            'J' => {
                let cy = screen.cursor_y;
                let cx = screen.cursor_x;
                let fill_fg = self.current_fg;
                let fill_bg = self.current_bg;
                match args.next().unwrap_or(0) {
                    0 => {
                        for x in cx..cols {
                            let cell = screen.get_cell_mut(x, cy);
                            cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                        }
                        for y in (cy + 1)..rows {
                            for x in 0..cols {
                                let cell = screen.get_cell_mut(x, y);
                                cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                            }
                        }
                    }
                    1 => {
                        for y in 0..cy {
                            for x in 0..cols {
                                let cell = screen.get_cell_mut(x, y);
                                cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                            }
                        }
                        for x in 0..=cx {
                            let cell = screen.get_cell_mut(x, cy);
                            cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                        }
                    }
                    2 => {
                        for y in 0..rows {
                            for x in 0..cols {
                                let cell = screen.get_cell_mut(x, y);
                                cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                            }
                        }
                    }
                    3 => {
                        for y in 0..rows {
                            for x in 0..cols {
                                let cell = screen.get_cell_mut(x, y);
                                cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                            }
                        }
                        screen.scrollback.clear();
                        screen.cursor_x = 0;
                        screen.cursor_y = 0;
                    }
                    _ => {}
                }
            }
            'A' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_y = screen.cursor_y.saturating_sub(n);
            }
            'B' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_y = (screen.cursor_y + n).min(rows - 1);
            }
            'C' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_x = (screen.cursor_x + n).min(cols - 1);
            }
            'D' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_x = screen.cursor_x.saturating_sub(n);
            }
            'E' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_y = (screen.cursor_y + n).min(rows - 1);
                screen.cursor_x = 0;
            }
            'F' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_y = screen.cursor_y.saturating_sub(n);
                screen.cursor_x = 0;
            }
            'G' => {
                let col = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_x = (col - 1).min(cols - 1);
            }
            'H' | 'f' => {
                let row = args.next().unwrap_or(1).max(1) as usize;
                let col = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_y = (row - 1).min(rows - 1);
                screen.cursor_x = (col - 1).min(cols - 1);
            }
            'd' => {
                let row = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_y = (row - 1).min(rows - 1);
            }
            'L' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                let cy = screen.cursor_y;
                let bot = screen.scroll_bot.min(rows - 1);
                let fill_fg = self.current_fg;
                let fill_bg = self.current_bg;
                if cy <= bot {
                    for _ in 0..n {
                        for y in (cy..bot).rev() {
                            for x in 0..cols {
                                screen.visible[y + 1].cells[x] = screen.visible[y].cells[x];
                            }
                        }
                        for x in 0..cols {
                            let cell = screen.get_cell_mut(x, cy);
                            cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                        }
                    }
                }
                screen.cursor_x = 0;
            }
            'M' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                let cy = screen.cursor_y;
                let bot = screen.scroll_bot.min(rows - 1);
                let fill_fg = self.current_fg;
                let fill_bg = self.current_bg;
                if cy <= bot {
                    for _ in 0..n {
                        for y in cy..bot {
                            for x in 0..cols {
                                screen.visible[y].cells[x] = screen.visible[y + 1].cells[x];
                            }
                        }
                        for x in 0..cols {
                            let cell = screen.get_cell_mut(x, bot);
                            cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                        }
                    }
                }
                screen.cursor_x = 0;
            }
            'P' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                let cy = screen.cursor_y;
                let cx = screen.cursor_x;
                let fill_fg = self.current_fg;
                let fill_bg = self.current_bg;
                let end = cols.saturating_sub(n);
                for x in cx..end {
                    screen.visible[cy].cells[x] = screen.visible[cy].cells[x + n];
                }
                for x in end..cols {
                    let cell = screen.get_cell_mut(x, cy);
                    cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                }
            }
            'X' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                let cy = screen.cursor_y;
                let cx = screen.cursor_x;
                let fill_fg = self.current_fg;
                let fill_bg = self.current_bg;
                for x in cx..(cx + n).min(cols) {
                    let cell = screen.get_cell_mut(x, cy);
                    cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                }
            }
            '@' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                let cy = screen.cursor_y;
                let cx = screen.cursor_x;
                let fill_fg = self.current_fg;
                let fill_bg = self.current_bg;
                let shift_end = cols.saturating_sub(n);
                for x in (cx..shift_end).rev() {
                    screen.visible[cy].cells[x + n] = screen.visible[cy].cells[x];
                }
                for x in cx..(cx + n).min(cols) {
                    let cell = screen.get_cell_mut(x, cy);
                    cell.c = ' '; cell.fg = fill_fg; cell.bg = fill_bg;
                }
            }
            'S' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                let fill_bg = self.current_bg;
                let def_fg = self.default_fg;
                for _ in 0..n {
                    screen.scroll_up(def_fg, fill_bg);
                }
            }
            'T' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                let fill_bg = self.current_bg;
                let def_fg = self.default_fg;
                for _ in 0..n {
                    screen.scroll_down(def_fg, fill_bg);
                }
            }
            'r' => {
                let top = args.next().unwrap_or(1).max(1) as usize;
                let bot = args.next().unwrap_or(rows as u16).max(1) as usize;
                screen.scroll_top = (top - 1).min(rows - 1);
                screen.scroll_bot = (bot - 1).min(rows - 1);
                screen.cursor_x = 0;
                screen.cursor_y = 0;
            }
            'n' => {}
            'm' => {
                let params_vec: Vec<u16> = args.collect();
                self.handle_sgr(&params_vec);
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn hook(&mut self, _params: &consts::Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        self.pending_wrap = false;
        match byte {
            b'7' => {
                let screen = if self.use_alt_screen { &self.alt } else { &self.primary };
                self.saved_cursor_x = screen.cursor_x;
                self.saved_cursor_y = screen.cursor_y;
            }
            b'8' => {
                let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };
                screen.cursor_x = self.saved_cursor_x.min(self.cols.saturating_sub(1));
                screen.cursor_y = self.saved_cursor_y.min(self.rows.saturating_sub(1));
                self.dirty = true;
            }
            b'c' => {
                self.current_fg = self.default_fg;
                self.current_bg = self.default_bg;
                self.hide_cursor = false;
                self.use_alt_screen = false;
                self.bold = false;
                self.pending_wrap = false;
                self.primary = Grid::new(self.cols, self.rows, self.default_fg, self.default_bg);
                self.alt = Grid::new(self.cols, self.rows, self.default_fg, self.default_bg);
                self.dirty = true;
            }
            b'D' => {
                let rows = self.rows;
                let def_fg = self.default_fg;
                let fill_bg = self.current_bg;
                let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };
                let bot = screen.scroll_bot.min(rows - 1);
                if screen.cursor_y < bot {
                    screen.cursor_y += 1;
                } else if screen.cursor_y == bot {
                    screen.scroll_up(def_fg, fill_bg);
                } else if screen.cursor_y < rows - 1 {
                    screen.cursor_y += 1;
                }
                self.dirty = true;
            }
            b'E' => {
                let rows = self.rows;
                let def_fg = self.default_fg;
                let fill_bg = self.current_bg;
                let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };
                screen.cursor_x = 0;
                let bot = screen.scroll_bot.min(rows - 1);
                if screen.cursor_y < bot {
                    screen.cursor_y += 1;
                } else if screen.cursor_y == bot {
                    screen.scroll_up(def_fg, fill_bg);
                } else if screen.cursor_y < rows - 1 {
                    screen.cursor_y += 1;
                }
                self.dirty = true;
            }
            b'M' => {
                let def_fg = self.default_fg;
                let fill_bg = self.current_bg;
                let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };
                let top = screen.scroll_top;
                if screen.cursor_y > top {
                    screen.cursor_y -= 1;
                } else {
                    screen.scroll_down(def_fg, fill_bg);
                }
                self.dirty = true;
            }
            _ => {}
        }
    }
}

mod consts {
    pub use vte::Params;
}
