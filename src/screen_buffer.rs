pub struct ScreenBuffer {
    size: (i32, i32),
    cursor: (i32, i32),
    pub buffer: Vec<u8>,
}

impl ScreenBuffer {
    pub fn new(size: (i32, i32)) -> ScreenBuffer {
        ScreenBuffer {
            size,
            cursor: (0, 0),
            buffer: vec![0; (size.0 * size.1) as usize],
        }
    }

    fn cursor_inc(&mut self) {
        if self.cursor.0 < self.size.0 - 1 {
            self.cursor.0 += 1;
        } else {
            self.cursor_newline();
        }
    }

    fn cursor_newline(&mut self) {
        self.cursor.0 = 0;
        if self.cursor.1 < self.size.1 - 1 {
            self.cursor.1 += 1;
        } else {
            self.scroll_up();
            self.clear_line();
        }
    }

    fn clear_line(&mut self) {
        for x in 0..self.size.0 {
            self.buffer[(x + self.cursor.1 * self.size.0) as usize] = 0;
        }
    }

    // does not move cursor
    fn scroll_up(&mut self) {
        for x in 0..self.size.0 {
            for y in 0..self.size.1 - 1 {
                self.buffer[(x + y * self.size.0) as usize] =
                    self.buffer[(x + (y + 1) * self.size.0) as usize];
            }
        }
    }

    // not set char
    fn backspace(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
        }
    }

    pub fn set_char(&mut self, ch: u8, cursor_inc: bool) {
        if ch == b'\n' {
            self.cursor_newline();
            return;
        }
        if ch == 7 {
            println!("beep!");
            return;
        }
        if ch == 8 {
            // override cursor_inc
            self.backspace();
            return;
        }
        self.buffer[(self.cursor.0 + self.cursor.1 * self.size.0) as usize] = ch;
        if cursor_inc {
            self.cursor_inc();
        }
    }

    pub fn move_cursor(&mut self, x: i32, y: i32, abs: bool) {
        if abs {
            self.cursor.0 = x;
            self.cursor.1 = y;
        } else {
            self.cursor.0 += x;
            self.cursor.1 += y;
        }
        self.cursor.0 = self.cursor.0.min(self.size.0 - 1).max(0);
        self.cursor.1 = self.cursor.1.min(self.size.1 - 1).max(0);
    }

    // match csi definition
    pub fn erase_display(&mut self, param: i32) {
        if param == 0 {
            for x in 0..self.size.0 {
                for y in self.cursor.1..self.size.1 {
                    self.buffer[(x + y * self.size.0) as usize] = b' ';
                }
            }
        } else if param == 1 {
            for x in 0..self.size.0 {
                for y in 0..=self.cursor.1 {
                    self.buffer[(x + y * self.size.0) as usize] = b' ';
                }
            }
        } else if param == 2 {
            for x in 0..self.size.0 {
                for y in 0..self.size.1 {
                    self.buffer[(x + y * self.size.0) as usize] = b' ';
                }
            }
        } else {
            println!("Unsupported EL Param!")
        }
    }

    // match csi definition
    pub fn erase_line(&mut self, param: i32) {
        if param == 0 {
            for i in self.cursor.0..self.size.0 {
                self.buffer[(i + self.cursor.1 * self.size.0) as usize] = b' ';
            }
        } else if param == 1 {
            for i in 0..=self.cursor.0 {
                self.buffer[(i + self.cursor.1 * self.size.0) as usize] = b' ';
            }
        } else if param == 2 {
            for i in 0..self.size.0 {
                self.buffer[(i + self.cursor.1 * self.size.0) as usize] = b' ';
            }
        } else {
            println!("Unsupported EL Param!")
        }
    }

    pub fn report_cursor(&self, param: i32) -> Option<Vec<u8>> {
        if param != 6 {
            println!("Error: only implemented report_cursor for final byte: n");
            return None;
        }
        let mut report = vec![27, b'['];
        report.extend(self.cursor.1.to_string().into_bytes());
        report.push(b';');
        report.extend(self.cursor.0.to_string().into_bytes());
        report.push(b'R');
        Some(report)
    }

    pub fn get_render_data(&mut self) -> (&Vec<u8>, (i32, i32)) {
        (&self.buffer, self.cursor)
    }
}
