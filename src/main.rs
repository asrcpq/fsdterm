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
        slave: slave_fd.into(),
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
    pub fn new() -> Console {
        let size = (80, 24);
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

    pub fn put_char(&mut self, ch: u8) {
        if ch == b'\n' {
            self.cursor_newline();
            return;
        }
        self.buffer[(self.cursor.0 + self.cursor.1 * self.size.0) as usize] = ch;
        self.cursor_inc();
    }

    pub fn render(&mut self) {
        self.canvas.flush();
        for x in 0..self.size.0 {
            for y in 0..self.size.1 {
                let ch = self.buffer[(x + y * self.size.0) as usize];
                // println!("{}", ch);
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
    }
}

fn start(pty: &PTY) {
    match unistd::fork() {
        Ok(unistd::ForkResult::Parent { child, .. }) => {
            unistd::close(pty.slave).unwrap();

            let sdl_context = sdl2::init().unwrap();
            let video_subsystem = sdl_context.video().unwrap();
            let WINDOW_SIZE: (u32, u32) = (1200, 480);

            let window = video_subsystem
                .window("eyhv", WINDOW_SIZE.0 as u32, WINDOW_SIZE.1 as u32)
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
                    WINDOW_SIZE.0,
                    WINDOW_SIZE.1,
                )
                .unwrap();

            let mut event_pump = sdl_context.event_pump().unwrap();

            let mut console = Console::new();

            'main_loop: loop {
                let mut readable = nix::sys::select::FdSet::new();
                readable.insert(pty.master);

                println!("wait...");
                std::thread::sleep(std::time::Duration::new(0, 1_000_000_000u32 / 120));

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
                    println!("buf: {:?}", buf);
                    console.put_char(buf[0]);
                    console.render();

                    texture
                        .update(None, &console.canvas.data, WINDOW_SIZE.0 as usize * 3)
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
                            let ch = match code {
                                Some(Keycode::A) => b'a',
                                Some(Keycode::B) => b'b',
                                Some(Keycode::C) => b'c',
                                Some(Keycode::D) => b'd',
                                Some(Keycode::E) => b'e',
                                Some(Keycode::F) => b'f',
                                Some(Keycode::G) => b'g',
                                Some(Keycode::H) => b'h',
                                Some(Keycode::I) => b'i',
                                Some(Keycode::J) => b'j',
                                Some(Keycode::K) => b'k',
                                Some(Keycode::L) => b'l',
                                Some(Keycode::M) => b'm',
                                Some(Keycode::N) => b'n',
                                Some(Keycode::O) => b'o',
                                Some(Keycode::P) => b'p',
                                Some(Keycode::Q) => b'q',
                                Some(Keycode::R) => b'r',
                                Some(Keycode::S) => b's',
                                Some(Keycode::T) => b't',
                                Some(Keycode::U) => b'u',
                                Some(Keycode::V) => b'v',
                                Some(Keycode::W) => b'w',
                                Some(Keycode::X) => b'x',
                                Some(Keycode::Y) => b'y',
                                Some(Keycode::Z) => b'z',
                                Some(Keycode::Space) => b' ',
                                Some(Keycode::Return) => b'\n',
                                _ => b'?',
                            };
                            nix::unistd::write(pty.master, &mut [ch; 1]).unwrap();
                        }
                        _ => {}
                    }
                }
            }

            // nix::sys::wait::waitpid(child, None);
        }
        Ok(unistd::ForkResult::Child) => {
            unistd::close(pty.master);

            // create process group
            unistd::setsid();

            const TIOCSCTTY: usize = 0x540E;
            nix::ioctl_write_int_bad!(tiocsctty, TIOCSCTTY);
            unsafe { tiocsctty(pty.slave, 0) };

            unistd::dup2(pty.slave, 0); // stdin
            unistd::dup2(pty.slave, 1); // stdout
            unistd::dup2(pty.slave, 2); // stderr
            unistd::close(pty.slave);

            use std::ffi::CString;
            let path = CString::new("/bin/sh").unwrap();
            unistd::execve(&path, &[], &[]);
        }
        Err(_) => {}
    }
}

fn main() {
    let pty = openpty().unwrap();
    start(&pty);
}
