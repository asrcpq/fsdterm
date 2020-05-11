extern crate mray;
extern crate nix;
extern crate sdl2;

use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::stat::Mode;
use nix::unistd;

use std::os::unix::io::RawFd;
use std::path::Path;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;

use mray::algebra::Point2f;
use mray::canvas::Canvas;
use mray::graphic_object::GraphicObjects;

fn set_shift(mut ch: u8, shift: bool) -> u8 {
    if !shift {
        return ch
    }
    ch = match ch {
        b'a'..=b'z' => ch - b'a' + b'A',
        b'1' => b'!',
        b'2' => b'@',
        b'3' => b'#',
        b'4' => b'$',
        b'5' => b'%',
        b'6' => b'^',
        b'7' => b'&',
        b'8' => b'*',
        b'9' => b'(',
        b'0' => b')',
        b'`' => b'~',
        b',' => b'<',
        b'.' => b'>',
        b'/' => b'?',
        b'\\' => b'|',
        _ => ch,
    };
    ch
}

fn find_sdl_gl_driver() -> Option<u32> {
    for (index, item) in sdl2::render::drivers().enumerate() {
        if item.name == "opengl" {
            return Some(index as u32);
        }
    }
    None
}

struct PTY {
    pub master: RawFd,
    pub slave: RawFd,
}

fn openpty() -> Result<PTY, String> {
    // Open a new PTY master
    let master_fd = posix_openpt(OFlag::O_RDWR).unwrap();

    grantpt(&master_fd).unwrap();
    unlockpt(&master_fd).unwrap();

    // Get the name of the slave
    let slave_name = unsafe { ptsname(&master_fd).unwrap() };

    // Try to open the slave
    let slave_fd = open(Path::new(&slave_name), OFlag::O_RDWR, Mode::empty()).unwrap();

    use std::os::unix::io::IntoRawFd;
    Ok(PTY {
        master: master_fd.into_raw_fd(),
        slave: slave_fd,
    })
}

pub struct Console {
    cursor: (i32, i32),
    size: (i32, i32),
    font_size: (i32, i32),
    scaler: f32,
    buffer: Vec<u8>,
    pub canvas: Canvas,
    csi_buf: Vec<u8>,
}

impl Console {
    pub fn new(size: (i32, i32)) -> Console {
        let font_size = (15, 20);
        Console {
            cursor: (0, 0),
            size,
            font_size,
            scaler: 20.,
            buffer: vec![0; (size.0 * size.1) as usize],
            canvas: Canvas::new((size.0 * font_size.0, size.1 * font_size.1)),
            csi_buf: Vec::new(),
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
                self.buffer[(x + y * self.size.0) as usize]
                    = self.buffer[(x + (y + 1) * self.size.0) as usize];
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

    fn move_cursor(&mut self, x: i32, y: i32, abs: bool) {
        if abs {
            self.cursor.0 = x;
            self.cursor.1 = y;
        } else {
            self.cursor.0 += x;
            self.cursor.1 += y;
            self.cursor.0 = self.cursor.0.min(self.size.0 - 1).max(0);
            self.cursor.1 = self.cursor.1.min(self.size.1 - 1).max(0);
        }
    }

    // match csi definition
    fn erase_line(&mut self, param: i32) {
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

    fn proc_csi(&mut self) {
        println!("{:?}", String::from_utf8(self.csi_buf.clone()).unwrap());
        if self.csi_buf.is_empty() {
            return;
        }
        if self.csi_buf[0] != 27 {
            println!("csi_buf error");
            return;
        }
        let mut param = Vec::new();
        let mut final_byte = None;
        for ch in self.csi_buf[1..].iter() {
            match ch {
                0x30..=0x3F => {
                    param.push(*ch);
                },
                0x40..=0x7F => {
                    final_byte = Some(ch);
                }
                _ => {}
            }
        }
        match final_byte {
            Some(b'D') => {
                self.move_cursor(-String::from_utf8(param).unwrap().parse::<i32>().unwrap_or(1), 0, false);
            },
            Some(b'C') => {
                self.move_cursor(String::from_utf8(param).unwrap().parse::<i32>().unwrap_or(1), 0, false);
            },
            Some(b'A') => {
                self.move_cursor(0, -String::from_utf8(param).unwrap().parse::<i32>().unwrap_or(1), false);
            },
            Some(b'B') => {
                self.move_cursor(0, String::from_utf8(param).unwrap().parse::<i32>().unwrap_or(1), false);
            },
            Some(b'H') => {
                // ansi coodinate is 1..=n, not 0..n
                let params = String::from_utf8(param).unwrap().split(";").map(|x| x.parse::<i32>().unwrap_or(1) - 1).collect::<Vec<i32>>();
                self.move_cursor(params[0], params[1], true);
            }
            Some(b'K') => {
                self.erase_line(String::from_utf8(param).unwrap().parse::<i32>().unwrap_or(0));
            }
            Some(x) => {
                println!("Unimplemented final byte {}", x)
            }
            _ => {},
        }
        self.csi_buf.clear();
    }

    pub fn put_char(&mut self, ch: u8) {
        if ch == 27 {
            //self.proc_csi();
            self.csi_buf = vec![27];
            return;
        }

        if !self.csi_buf.is_empty() {
            if self.csi_buf.len() == 1 && ch == b'[' {
                self.csi_buf.push(ch);
                return;
            }
            if ch >= 0x40 && ch < 0x80 {
                self.csi_buf.push(ch);
                self.proc_csi();
                return;
            }
            self.csi_buf.push(ch);
            return;
        }

        self.set_char(ch, true);
    }

    pub fn render(&mut self) {
        self.canvas.flush();
        for x in 0..self.size.0 {
            for y in 0..self.size.1 {
                let ch = self.buffer[(x + y * self.size.0) as usize];
                for graphic_object in
                    GraphicObjects::fsd(char::from(ch))
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
        for graphic_object in
            GraphicObjects::fsd(char::from(ch))
                .zoom(self.scaler as f32)
                .shift(Point2f::from_floats(
                    (self.font_size.0 * self.cursor.0) as f32,
                    (self.font_size.1 * self.cursor.1) as f32,
                ))
                .into_iter() {
            graphic_object.render(&mut self.canvas);
        }
    }
}

fn start(pty: &PTY) {
    match unistd::fork() {
        Ok(unistd::ForkResult::Parent { child: _, .. }) => {
            unistd::close(pty.slave).unwrap();

            let sdl_context = sdl2::init().unwrap();
            let video_subsystem = sdl_context.video().unwrap();
            let window_size: (u32, u32) = (1200, 480);

            let mut shift: bool = false;
            let mut ctrl: bool = false;

            let window = video_subsystem
                .window("fsdterm", window_size.0 as u32, window_size.1 as u32)
                .opengl()
                .position_centered()
                .build()
                .unwrap();

            let mut canvas = window
                .into_canvas()
                .index(find_sdl_gl_driver().unwrap())
                .build()
                .unwrap();

            let texture_creator = canvas.texture_creator();
            let mut texture = texture_creator
                .create_texture_static(
                    Some(sdl2::pixels::PixelFormatEnum::RGB24),
                    window_size.0,
                    window_size.1,
                )
                .unwrap();

            let mut event_pump = sdl_context.event_pump().unwrap();

            let mut console = Console::new((80, 24));

            'main_loop: loop {
                let mut readable = nix::sys::select::FdSet::new();
                readable.insert(pty.master);

                // println!("wait...");
                std::thread::sleep(std::time::Duration::new(0, 1_000_000_000u32 / 500));

                use nix::sys::time::TimeValLike;
                nix::sys::select::select(
                    None,
                    Some(&mut readable),                        // read
                    None,                                       // write
                    None,                                       // error
                    Some(&mut nix::sys::time::TimeVal::zero()), // polling
                )
                .unwrap();

                if readable.contains(pty.master) {
                    let mut buf = [0];
                    if let Err(e) = nix::unistd::read(pty.master, &mut buf) {
                        eprintln!("Nothing to read from child: {}", e);
                        break;
                    }
                    console.put_char(buf[0]);
                    console.render();

                    texture
                        .update(None, &console.canvas.data, window_size.0 as usize * 3)
                        .unwrap();

                    canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
                    canvas.clear();
                    canvas.copy(&texture, None, None).unwrap();
                    canvas.present();
                }

                // read input
                for event in event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. } => break 'main_loop,
                        Event::KeyDown { keycode: code, .. } => {
                            let mut ch = match code {
                                Some(Keycode::A) => Some(vec![b'a']),
                                Some(Keycode::B) => Some(vec![b'b']),
                                Some(Keycode::C) => Some(vec![b'c']),
                                Some(Keycode::D) => Some(vec![b'd']),
                                Some(Keycode::E) => Some(vec![b'e']),
                                Some(Keycode::F) => Some(vec![b'f']),
                                Some(Keycode::G) => Some(vec![b'g']),
                                Some(Keycode::H) => Some(vec![b'h']),
                                Some(Keycode::I) => Some(vec![b'i']),
                                Some(Keycode::J) => Some(vec![b'j']),
                                Some(Keycode::K) => Some(vec![b'k']),
                                Some(Keycode::L) => Some(vec![b'l']),
                                Some(Keycode::M) => Some(vec![b'm']),
                                Some(Keycode::N) => Some(vec![b'n']),
                                Some(Keycode::O) => Some(vec![b'o']),
                                Some(Keycode::P) => Some(vec![b'p']),
                                Some(Keycode::Q) => Some(vec![b'q']),
                                Some(Keycode::R) => Some(vec![b'r']),
                                Some(Keycode::S) => Some(vec![b's']),
                                Some(Keycode::T) => Some(vec![b't']),
                                Some(Keycode::U) => Some(vec![b'u']),
                                Some(Keycode::V) => Some(vec![b'v']),
                                Some(Keycode::W) => Some(vec![b'w']),
                                Some(Keycode::X) => Some(vec![b'x']),
                                Some(Keycode::Y) => Some(vec![b'y']),
                                Some(Keycode::Z) => Some(vec![b'z']),
                                Some(Keycode::Quote) => Some(vec![b'\'']),
                                Some(Keycode::Comma) => Some(vec![b',']),
                                Some(Keycode::Minus) => Some(vec![b'-']),
                                Some(Keycode::Period) => Some(vec![b'.']),
                                Some(Keycode::Slash) => Some(vec![b'/']),
                                Some(Keycode::Num0) => Some(vec![b'0']),
                                Some(Keycode::Num1) => Some(vec![b'1']),
                                Some(Keycode::Num2) => Some(vec![b'2']),
                                Some(Keycode::Num3) => Some(vec![b'3']),
                                Some(Keycode::Num4) => Some(vec![b'4']),
                                Some(Keycode::Num5) => Some(vec![b'5']),
                                Some(Keycode::Num6) => Some(vec![b'6']),
                                Some(Keycode::Num7) => Some(vec![b'7']),
                                Some(Keycode::Num8) => Some(vec![b'8']),
                                Some(Keycode::Num9) => Some(vec![b'9']),
                                Some(Keycode::Semicolon) => Some(vec![b';']),
                                Some(Keycode::Equals) => Some(vec![b'=']),
                                Some(Keycode::Backslash) => Some(vec![b'\\']),
                                Some(Keycode::Backspace) => Some(vec![8, b' ', 8]),
                                Some(Keycode::Space) => Some(vec![b' ']),
                                Some(Keycode::LShift) | Some(Keycode::RShift) => {
                                    shift = true;
                                    None
                                }
                                Some(Keycode::LCtrl) | Some(Keycode::RCtrl) => {
                                    ctrl = true;
                                    None
                                }
                                Some(Keycode::Return) => Some(vec![b'\n']),
                                Some(Keycode::Left) => {
                                    Some(vec![27, b'O', b'D'])
                                },
                                Some(Keycode::Right) => {
                                    Some(vec![27, b'O', b'C'])
                                },
                                Some(Keycode::Down) => {
                                    Some(vec![27, b'O', b'B'])
                                },
                                Some(Keycode::Up) => {
                                    Some(vec![27, b'O', b'A'])
                                },
                                _ => None,
                            };
                            
                            if ctrl {
                                if let Some(c) = ch.clone() {
                                    if c[0] == b'c' {
                                        ch = Some(vec![3]);
                                    }
                                }
                            }
                            if let Some(ch) = ch {
                                for c in ch.iter() {
                                    nix::unistd::write(pty.master, &[set_shift(*c, shift); 1]).unwrap();
                                }
                            }
                        }
                        Event::KeyUp { keycode: code, .. } => {
                            match code {
                                Some(Keycode::LShift) | Some(Keycode::RShift) => 
                                    shift = false,
                                Some(Keycode::LCtrl) | Some(Keycode::RCtrl) =>
                                    ctrl = false,
                                _ => {},
                            };
                        }
                        _ => {}
                    }
                }
            }

            // nix::sys::wait::waitpid(child, None);
        }
        Ok(unistd::ForkResult::Child) => {
            unistd::close(pty.master).unwrap();

            // create process group
            unistd::setsid().unwrap();

            const TIOCSCTTY: usize = 0x540E;
            nix::ioctl_write_int_bad!(tiocsctty, TIOCSCTTY);
            unsafe { tiocsctty(pty.slave, 0).unwrap() };

            unistd::dup2(pty.slave, 0).unwrap(); // stdin
            unistd::dup2(pty.slave, 1).unwrap(); // stdout
            unistd::dup2(pty.slave, 2).unwrap(); // stderr
            unistd::close(pty.slave).unwrap();

            use std::ffi::CString;
            let path = CString::new("/bin/bash").unwrap();
            unistd::execve(&path, &[], &[]).unwrap();
        }
        Err(_) => {}
    }
}

fn main() {
    let pty = openpty().unwrap();
    start(&pty);
}
