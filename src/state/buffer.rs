use vte::Perform;

#[derive(Clone, Copy)]
pub struct Cell {
    pub c: char,
}

pub struct TerminalState {
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub cols: usize,
    pub rows: usize,
    pub grid: Vec<Vec<Cell>>,
    pub dirty: bool,
}

impl TerminalState {
    pub fn new(cols: usize, rows: usize) -> Self {
        let empty_cell = Cell { c: ' ' };
        let grid = vec![vec![empty_cell; cols]; rows];

        Self { 
            cursor_x: 0,
            cursor_y: 0,
            cols,
            rows,
            grid,
            dirty: true,
        }
    }

    fn scroll_up(&mut self) {
        self.grid.remove(0);
        self.grid.push(vec![Cell { c: ' ' }; self.cols]);
    }
}

impl Perform for TerminalState {
    fn print(&mut self, c: char) {
        if self.cursor_y < self.rows && self.cursor_x < self.cols {
            self.grid[self.cursor_y][self.cursor_x].c = c;
            self.cursor_x += 1;
        }

        if self.cursor_x >= self.cols {
            self.cursor_x = 0;
            if self.cursor_y < self.rows - 1 {
                self.cursor_y += 1;
            } else {
                self.scroll_up();
            }
        }

        self.dirty = true;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            10 => {
                if self.cursor_y < self.rows - 1 {
                    self.cursor_y += 1;
                } else {
                    self.scroll_up();
                }
            }
            13 => {
                self.cursor_x = 0;
            }
            8 => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                }
            }
            7 => { /* ignore bell */ }
            127 => {/*ignorar del visual*/}
            _ => {
                println!("[VTE - Execute] Byte no manejado: {:#04x}", byte);
            }
        }

        self.dirty = true;
    }

    fn csi_dispatch(
            &mut self,
            params: &consts::Params,
            _intermediates: &[u8],
            _ignore: bool,
            action: char,
        ) {
        let mut args = params.iter().map(|param| param[0]);

        match action {
            'K' => {
                // Erase in Line (Borrar en la línea actual)
                let mode = args.next().unwrap_or(0);
                match mode {
                    0 => {
                        // Borrar desde el cursor hasta el final
                        for x in self.cursor_x..self.cols {
                            self.grid[self.cursor_y][x].c = ' ';
                        }
                    }
                    1 => {
                        // Borrar desde el inicio hasta el cursor
                        for x in 0..=self.cursor_x {
                            self.grid[self.cursor_y][x].c = ' ';
                        }
                    }
                    2 => {
                        // Borrar toda la línea
                        for x in 0..self.cols {
                            self.grid[self.cursor_y][x].c = ' ';
                        }
                    }
                    _ => {}
                }
            }
            'J' => {
                // Erase in Display (Limpiar pantalla, ej. al usar comando 'clear')
                let mode = args.next().unwrap_or(0);
                if mode == 2 || mode == 3 {
                    for y in 0..self.rows {
                        for x in 0..self.cols {
                            self.grid[y][x].c = ' ';
                        }
                    }
                    self.cursor_x = 0;
                    self.cursor_y = 0;
                }
            }
            'C' => {
                // Cursor Forward (Flecha derecha)
                let n = args.next().unwrap_or(1).max(1) as usize;
                self.cursor_x = (self.cursor_x + n).min(self.cols - 1);
            }
            'D' => {
                // Cursor Backward (Flecha izquierda)
                let n = args.next().unwrap_or(1).max(1) as usize;
                self.cursor_x = self.cursor_x.saturating_sub(n);
            }
            'H' | 'f' => {
                // Cursor Position (Mover a coordenadas exactas)
                let row = args.next().unwrap_or(1).max(1) as usize;
                let col = args.next().unwrap_or(1).max(1) as usize;
                self.cursor_y = (row - 1).min(self.rows - 1);
                self.cursor_x = (col - 1).min(self.cols - 1);
            }
            _ => {
                // Comandos de color ('m') y otros llegarán aquí después
            }
        }
        self.dirty = true; // Avisamos al motor gráfico que repinte
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
