use crate::screen_buffer::ScreenBuffer;
use mray::algebra::Point2f;
use mray::canvas::Canvas;

pub struct Console {
    size: (i32, i32),
    font_size: (i32, i32),
    scaler: f32,
    pub canvas: Canvas,
    csi_buf: Vec<u8>,
    screen: Vec<ScreenBuffer>,
    sid: usize,
}

impl Console {
    pub fn new(size: (i32, i32)) -> Console {
        let font_size = (15, 20);
        Console {
            size,
            font_size,
            scaler: 20.,
            canvas: Canvas::new((size.0 * font_size.0, size.1 * font_size.1)),
            csi_buf: Vec::new(),
            screen: vec![ScreenBuffer::new(size), ScreenBuffer::new(size)],
            sid: 0,
        }
    }

    // for set env
    pub fn get_size(&self) -> (i32, i32) {
        self.size
    }

    fn proc_csi(&mut self) -> Option<Vec<u8>> {
        println!("{:?}", String::from_utf8(self.csi_buf.clone()).unwrap());
        if self.csi_buf.is_empty() {
            return None;
        }
        if self.csi_buf[0] != 27 {
            println!("csi_buf error");
            return None;
        }
        let mut param = Vec::new();
        let mut final_byte = None;
        for ch in self.csi_buf[1..].iter() {
            match ch {
                0x30..=0x3F => {
                    param.push(*ch);
                }
                0x40..=0x7F => {
                    final_byte = Some(ch);
                }
                _ => {}
            }
        }
        let mut report = None;
        match final_byte {
            Some(b'D') => {
                self.screen[self.sid].move_cursor(
                    -String::from_utf8(param)
                        .unwrap()
                        .parse::<i32>()
                        .unwrap_or(1),
                    0,
                    false,
                );
            }
            Some(b'C') => {
                self.screen[self.sid].move_cursor(
                    String::from_utf8(param)
                        .unwrap()
                        .parse::<i32>()
                        .unwrap_or(1),
                    0,
                    false,
                );
            }
            Some(b'A') => {
                self.screen[self.sid].move_cursor(
                    0,
                    -String::from_utf8(param)
                        .unwrap()
                        .parse::<i32>()
                        .unwrap_or(1),
                    false,
                );
            }
            Some(b'B') => {
                self.screen[self.sid].move_cursor(
                    0,
                    String::from_utf8(param)
                        .unwrap()
                        .parse::<i32>()
                        .unwrap_or(1),
                    false,
                );
            }
            Some(b'H') => {
                // ansi coodinate is 1..=n, not 0..n
                let params = String::from_utf8(param)
                    .unwrap()
                    .split(";")
                    .map(|x| x.parse::<i32>().unwrap_or(1) - 1)
                    .collect::<Vec<i32>>();
                if params.len() == 1 {
                    self.screen[self.sid].move_cursor(1, 1, true);
                } else {
                    self.screen[self.sid].move_cursor(params[1], params[0], true);
                }
            }
            Some(b'J') => {
                self.screen[self.sid].erase_display(
                    String::from_utf8(param)
                        .unwrap()
                        .parse::<i32>()
                        .unwrap_or(0),
                );
            }
            Some(b'K') => {
                self.screen[self.sid].erase_line(
                    String::from_utf8(param)
                        .unwrap()
                        .parse::<i32>()
                        .unwrap_or(0),
                );
            }
            Some(b'm') => {
                // we do nothing here as we don't plan to support SGR
            }
            Some(b'n') => {
                report = self.screen[self.sid].report_cursor(
                    String::from_utf8(param)
                        .unwrap()
                        .parse::<i32>()
                        .unwrap_or(0),
                );
            }
            Some(b'h') => {
                if std::str::from_utf8(&self.csi_buf).unwrap() == "\x1b[?1049h" {
                    self.sid = 1;
                } else {
                    println!(
                        "Unimplemented csi sequence {:?}",
                        String::from_utf8(self.csi_buf.clone()).unwrap()
                    );
                }
            }
            Some(b'l') => {
                if std::str::from_utf8(&self.csi_buf).unwrap() == "\x1b[?1049l" {
                    self.sid = 0;
                } else {
                    println!(
                        "Unimplemented csi sequence {:?}",
                        String::from_utf8(self.csi_buf.clone()).unwrap()
                    );
                }
            }
            Some(_) => {
                println!(
                    "Unimplemented final byte {:?}",
                    String::from_utf8(self.csi_buf.clone()).unwrap()
                );
            }
            _ => {}
        }
        self.csi_buf.clear();
        report
    }

    pub fn put_char(&mut self, ch: u8) -> Option<Vec<u8>> {
        if ch == 27 {
            //self.proc_csi();
            self.csi_buf = vec![27];
            return None;
        }

        if !self.csi_buf.is_empty() {
            if self.csi_buf.len() == 1 && ch == b'[' {
                self.csi_buf.push(ch);
                return None;
            }
            if ch >= 0x40 && ch < 0x80 {
                self.csi_buf.push(ch);
                return self.proc_csi();
            }
            self.csi_buf.push(ch);
            return None;
        }

        if ch == 13 {
            self.screen[self.sid].set_char(ch, false);
            return None;
        }
        self.screen[self.sid].set_char(ch, true);
        None
    }

    pub fn render(&mut self) {
        let (buffer, cursor) = self.screen[self.sid].get_render_data();
        self.canvas.flush();
        for x in 0..self.size.0 {
            for y in 0..self.size.1 {
                let ch = buffer[(x + y * self.size.0) as usize];
                for graphic_object in mray::fsd::fsd(char::from(ch))
                    .shift(Point2f::from_floats(-0.5, -0.5))
                    .shear(-0.25)
                    .shift(Point2f::from_floats(0.5, 0.5))
                    .zoom(self.scaler as f32)
                    .shift(Point2f::from_floats(
                        (self.font_size.0 * x) as f32,
                        (self.font_size.1 * y) as f32,
                    ))
                    .into_iter()
                {
                    graphic_object.render(&mut self.canvas);
                }
            }
        }
        // cursor render
        let ch = b'|';
        for graphic_object in mray::fsd::fsd(char::from(ch))
            .zoom(self.scaler as f32)
            .shift(Point2f::from_floats(
                (self.font_size.0 * cursor.0) as f32,
                (self.font_size.1 * cursor.1) as f32,
            ))
            .into_iter()
        {
            graphic_object.render(&mut self.canvas);
        }
    }
}
