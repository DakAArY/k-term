use std::{isize, usize};

use vte::Perform;

#[derive(Clone, Copy)]
pub struct Cell {
    pub c: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

const DEFAULT_FG: [u8; 3] = [220, 220, 220];
const DEFAULT_BG: [u8; 3] = [18, 18, 18];

fn color_256_to_rgb(color: u16) -> [u8; 3] {
    match color {
        0..=15 => [200, 200, 200],
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

pub struct Screen {
    pub grid: Vec<Vec<Cell>>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub scrollback: Vec<Vec<Cell>>,
    pub scroll_offset: usize,
}

impl Screen {
    pub fn new(cols: usize, rows: usize, def_fg: [u8; 3], def_bg: [u8; 3]) -> Self {
        let empty_cell = Cell { c: ' ', fg: def_fg, bg: def_bg };
        Self {
            grid: vec![vec![empty_cell; cols]; rows],
            cursor_x: 0,
            cursor_y: 0,
            scrollback: Vec::with_capacity(10000),
            scroll_offset: 0,
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize, def_fg: [u8; 3], def_bg: [u8; 3]) {
        self.grid.resize(new_rows, vec![Cell { c: ' ', fg: def_fg, bg: def_bg }; new_cols]);

        for row in &mut self.grid {
            row.resize(new_cols, Cell { c: ' ', fg: def_fg, bg: def_bg });
        }

        self.cursor_x = self.cursor_x.min(new_cols.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(new_rows.saturating_sub(1));
    }

    fn scroll_up(&mut self, cols: usize, def_fg: [u8; 3], def_bg: [u8; 3]) {
        let removed_row = self.grid.remove(0);
        self.scrollback.push(removed_row);

        if self.scrollback.len() > 10000 {
            self.scrollback.remove(0);
        }

        self.grid.push(vec![Cell { c: ' ', fg: def_fg, bg: def_bg }; cols]);
    }
}

pub struct TerminalState {
    pub cols: usize, pub rows: usize,
    pub primary: Screen,  pub alt: Screen,    
    pub use_alt_screen: bool, pub dirty: bool,
    pub current_fg: [u8; 3], pub current_bg: [u8; 3],
    pub default_fg: [u8; 3], pub default_bg: [u8; 3],
}

impl TerminalState {
    pub fn new(cols: usize, rows: usize, def_fg: [u8; 3], def_bg: [u8; 3]) -> Self {
        Self { 
            cols, rows,
            primary: Screen::new(cols, rows, def_fg, def_bg),
            alt: Screen::new(cols, rows, def_fg, def_bg),
            use_alt_screen: false, dirty: true,
            current_fg: def_fg, current_bg: def_bg,
            default_fg: def_fg, default_bg: def_bg
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        self.cols = new_cols;
        self.rows = new_rows;
        self.primary.resize(new_cols, new_rows, self.default_fg, self.default_bg);
        self.alt.resize(new_cols, new_rows, self.default_fg, self.default_bg);
        self.dirty = true;
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
}

impl Perform for TerminalState {
    fn print(&mut self, c: char) {
        self.snap_to_bottom();

        let cols = self.cols;
        let rows = self.rows;
        let fg = self.current_fg;
        let bg = self.current_bg;
        
        let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };

        if screen.cursor_y < rows && screen.cursor_x < cols {
            screen.grid[screen.cursor_y][screen.cursor_x].c = c;
            screen.grid[screen.cursor_y][screen.cursor_x].fg = fg;
            screen.grid[screen.cursor_y][screen.cursor_x].bg = bg;
            screen.cursor_x += 1;
        }

        if screen.cursor_x >= cols {
            screen.cursor_x = 0;
            if screen.cursor_y < rows - 1 {
                screen.cursor_y += 1;
            } else {
                screen.scroll_up(cols, self. default_fg, self.default_bg);
            }
        }

        self.dirty = true;
    }

    fn execute(&mut self, byte: u8) {
        self.snap_to_bottom();

        let rows = self.rows;
        let cols = self.cols;
        let screen = if self.use_alt_screen { &mut self.alt } else { &mut self.primary };

        match byte {
            10 => { // \n
                if screen.cursor_y < rows - 1 {
                    screen.cursor_y += 1;
                } else {
                    screen.scroll_up(cols, self.default_fg, self.default_bg);
                }
            }
            13 => screen.cursor_x = 0, // \r
            8 => { // \b
                if screen.cursor_x > 0 {
                    screen.cursor_x -= 1;
                }
            }
            7 | 127 => { /* ignorar */ }
            _ => {}
        }
        self.dirty = true;
    }

    fn csi_dispatch(
            &mut self,
            params: &consts::Params,
            intermediates: &[u8],
            _ignore: bool,
            action: char,
        ) {
        let mut args = params.iter().map(|param| param[0]);
        let is_dec_private = intermediates.get(0) == Some(&b'?');

        if is_dec_private {
            match action {
                'h' => {
                    for mode in args {
                        if mode == 1049 {
                            self.use_alt_screen = true;
                            self.alt = Screen::new(self.cols, self.rows, self.default_fg, self.default_bg);
                        }
                    }
                    self.dirty = true;
                    return;
                }
                'l' => {
                    for mode in args {
                        if mode == 1049 {
                            self.use_alt_screen = false;
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
                let mode = args.next().unwrap_or(0);
                match mode {
                    0 => for x in screen.cursor_x..cols { screen.grid[screen.cursor_y][x].c = ' '; },
                    1 => for x in 0..=screen.cursor_x { screen.grid[screen.cursor_y][x].c = ' '; },
                    2 => for x in 0..cols { screen.grid[screen.cursor_y][x].c = ' '; },
                    _ => {}
                }
            }
            'J' => {
                let mode = args.next().unwrap_or(0);
                if mode == 2 || mode == 3 {
                    for y in 0..rows {
                        for x in 0..cols {
                            screen.grid[y][x].c = ' ';
                        }
                    }
                    screen.cursor_x = 0;
                    screen.cursor_y = 0;
                }
            }
            'C' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_x = (screen.cursor_x + n).min(cols - 1);
            }
            'D' => {
                let n = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_x = screen.cursor_x.saturating_sub(n);
            }
            'H' | 'f' => {
                let row = args.next().unwrap_or(1).max(1) as usize;
                let col = args.next().unwrap_or(1).max(1) as usize;
                screen.cursor_y = (row - 1).min(rows - 1);
                screen.cursor_x = (col - 1).min(cols - 1);
            }
            'm' => {
                let params_vec: Vec<u16> = args.collect();
                if params_vec.is_empty() {
                    self.current_fg = DEFAULT_FG;
                    self.current_bg = DEFAULT_BG;
                } else {
                    let mut i = 0;
                    while i < params_vec.len() {
                        let param = params_vec[i];
                        match param {
                            0 => {
                                self.current_fg = DEFAULT_FG;
                                self.current_bg = DEFAULT_BG;
                            }
                            30 => self.current_fg = [0, 0, 0],
                            31 => self.current_fg = [205, 49, 49],
                            32 => self.current_fg = [13, 188, 121],
                            33 => self.current_fg = [229, 229, 16],
                            34 => self.current_fg = [36, 114, 200],
                            35 => self.current_fg = [188, 63, 188],
                            36 => self.current_fg = [17, 168, 205],
                            37 => self.current_fg = [229, 229, 229],
                            90 => self.current_fg = [102, 102, 102],
                            91 => self.current_fg = [241, 76, 76],
                            92 => self.current_fg = [35, 209, 139],
                            93 => self.current_fg = [245, 245, 67],
                            94 => self.current_fg = [59, 142, 234],
                            95 => self.current_fg = [214, 112, 214],
                            96 => self.current_fg = [41, 184, 219],
                            97 => self.current_fg = [229, 229, 229],
                            40 => self.current_bg = [0, 0, 0],
                            41 => self.current_bg = [205, 49, 49],
                            42 => self.current_bg = [13, 188, 121],
                            43 => self.current_bg = [229, 229, 16],
                            44 => self.current_bg = [36, 114, 200],
                            45 => self.current_bg = [188, 63, 188],
                            46 => self.current_bg = [17, 168, 205],
                            47 => self.current_bg = [229, 229, 229],
                            100 => self.current_bg = [102, 102, 102],
                            101 => self.current_bg = [241, 76, 76],
                            102 => self.current_bg = [35, 209, 139],
                            103 => self.current_bg = [245, 245, 67],
                            104 => self.current_bg = [59, 142, 234],
                            105 => self.current_bg = [214, 112, 214],
                            106 => self.current_bg = [41, 184, 219],
                            107 => self.current_bg = [229, 229, 229],
                            38 => {
                                if i + 2 < params_vec.len() && params_vec[i+1] == 5 {
                                    self.current_fg = color_256_to_rgb(params_vec[i+2]);
                                    i += 2; 
                                } else if i + 4 < params_vec.len() && params_vec[i+1] == 2 {
                                    self.current_fg = [params_vec[i+2] as u8, params_vec[i+3] as u8, params_vec[i+4] as u8];
                                    i += 4;
                                }
                            }
                            48 => {
                                if i + 2 < params_vec.len() && params_vec[i+1] == 5 {
                                    self.current_bg = color_256_to_rgb(params_vec[i+2]);
                                    i += 2;
                                } else if i + 4 < params_vec.len() && params_vec[i+1] == 2 {
                                    self.current_bg = [params_vec[i+2] as u8, params_vec[i+3] as u8, params_vec[i+4] as u8];
                                    i += 4;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn hook(&mut self, _params: &consts::Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

mod consts {
    pub use vte::Params;
}
