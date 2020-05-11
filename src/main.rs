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

    fn backspace(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
        }
        self.set_char(0, false);
    }

    pub fn set_char(&mut self, ch: u8, cursor_inc: bool) {
        if ch == b'\n' {
            self.cursor_newline();
            return;
        }
        if ch == 8 {
            // override cursor_inc
            println!("back");
            self.backspace();
            return;
        }
        self.buffer[(self.cursor.0 + self.cursor.1 * self.size.0) as usize] = ch;
        if cursor_inc {
            self.cursor_inc();
        }
    }

    pub fn put_char(&mut self, ch: u8) {
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
                    (self.font_size.1 * self.cursor.1   ) as f32,
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
                                Some(Keycode::A) => Some(b'a'),
                                Some(Keycode::B) => Some(b'b'),
                                Some(Keycode::C) => Some(b'c'),
                                Some(Keycode::D) => Some(b'd'),
                                Some(Keycode::E) => Some(b'e'),
                                Some(Keycode::F) => Some(b'f'),
                                Some(Keycode::G) => Some(b'g'),
                                Some(Keycode::H) => Some(b'h'),
                                Some(Keycode::I) => Some(b'i'),
                                Some(Keycode::J) => Some(b'j'),
                                Some(Keycode::K) => Some(b'k'),
                                Some(Keycode::L) => Some(b'l'),
                                Some(Keycode::M) => Some(b'm'),
                                Some(Keycode::N) => Some(b'n'),
                                Some(Keycode::O) => Some(b'o'),
                                Some(Keycode::P) => Some(b'p'),
                                Some(Keycode::Q) => Some(b'q'),
                                Some(Keycode::R) => Some(b'r'),
                                Some(Keycode::S) => Some(b's'),
                                Some(Keycode::T) => Some(b't'),
                                Some(Keycode::U) => Some(b'u'),
                                Some(Keycode::V) => Some(b'v'),
                                Some(Keycode::W) => Some(b'w'),
                                Some(Keycode::X) => Some(b'x'),
                                Some(Keycode::Y) => Some(b'y'),
                                Some(Keycode::Z) => Some(b'z'),
                                Some(Keycode::Quote) => Some(b'\''),
                                Some(Keycode::Comma) => Some(b','),
                                Some(Keycode::Minus) => Some(b'-'),
                                Some(Keycode::Period) => Some(b'.'),
                                Some(Keycode::Slash) => Some(b'/'),
                                Some(Keycode::Num0) => Some(b'0'),
                                Some(Keycode::Num1) => Some(b'1'),
                                Some(Keycode::Num2) => Some(b'2'),
                                Some(Keycode::Num3) => Some(b'3'),
                                Some(Keycode::Num4) => Some(b'4'),
                                Some(Keycode::Num5) => Some(b'5'),
                                Some(Keycode::Num6) => Some(b'6'),
                                Some(Keycode::Num7) => Some(b'7'),
                                Some(Keycode::Num8) => Some(b'8'),
                                Some(Keycode::Num9) => Some(b'9'),
                                Some(Keycode::Semicolon) => Some(b';'),
                                Some(Keycode::Equals) => Some(b'='),
                                Some(Keycode::Backslash) => Some(b'\\'),
                                Some(Keycode::Backspace) => Some(8),
                                Some(Keycode::Space) => Some(b' '),
                                Some(Keycode::LShift) | Some(Keycode::RShift) => {
                                    shift = true;
                                    None
                                }
                                Some(Keycode::LCtrl) | Some(Keycode::RCtrl) => {
                                    ctrl = true;
                                    None
                                }
                                Some(Keycode::Return) => Some(b'\n'),
                                _ => None,
                            };
                            
                            if ctrl {
                                ch = match ch {
                                    Some(b'c') => Some(3),
                                    _ => None,
                                }
                            }
                            if let Some(ch) = ch {
                                nix::unistd::write(pty.master, &[set_shift(ch, shift); 1]).unwrap();
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
