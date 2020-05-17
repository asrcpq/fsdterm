mod console;
mod screen_buffer;

extern crate mray;
extern crate nix;
extern crate sdl2;

use console::Console;

use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::stat::Mode;
use nix::unistd;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;

use std::os::unix::io::RawFd;
use std::path::Path;

fn set_shift(mut ch: u8, shift: bool) -> u8 {
    if !shift {
        return ch;
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
        b'-' => b'_',
        b'=' => b'+',
        b'`' => b'~',
        b',' => b'<',
        b'.' => b'>',
        b'/' => b'?',
        b'[' => b'{',
        b']' => b'}',
        b'\\' => b'|',
        b';' => b':',
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

fn start(pty: &PTY) {
    // console is created before creating process
    let mut console = Console::new((80, 24));

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

            'main_loop: loop {
                // println!("wait...");
                std::thread::sleep(std::time::Duration::new(0, 10_000_000u32));
                'readable_pts: loop {
                    let mut readable = nix::sys::select::FdSet::new();
                    readable.insert(pty.master);

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
                            break 'main_loop;
                        }
                        if let Some(report) = console.put_char(buf[0]) {
                            for c in report.iter() {
                                nix::unistd::write(pty.master, &[set_shift(*c, shift); 1]).unwrap();
                            }
                        }
                    } else {
                        break 'readable_pts;
                    }
                }
                console.render();

                texture
                    .update(None, &console.canvas.data, window_size.0 as usize * 3)
                    .unwrap();

                canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
                canvas.clear();
                canvas.copy(&texture, None, None).unwrap();
                canvas.present();

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
                                Some(Keycode::LeftBracket) => Some(vec![b'[']),
                                Some(Keycode::RightBracket) => Some(vec![b']']),
                                Some(Keycode::Backslash) => Some(vec![b'\\']),
                                Some(Keycode::Backquote) => Some(vec![b'`']),
                                Some(Keycode::Backspace) => Some(vec![8, b' ', 8]),
                                Some(Keycode::Escape) => Some(vec![27]),
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
                                Some(Keycode::Left) => Some(vec![27, b'O', b'D']),
                                Some(Keycode::Right) => Some(vec![27, b'O', b'C']),
                                Some(Keycode::Down) => Some(vec![27, b'O', b'B']),
                                Some(Keycode::Up) => Some(vec![27, b'O', b'A']),
                                _ => None,
                            };

                            ch = match ch {
                                None => None,
                                Some(mut ch) => {
                                    Some(ch
                                        .iter_mut()
                                        .map(|x| set_shift(*x, shift))
                                        .collect()
                                    )
                                },
                            };

                            if ctrl {
                                if let Some(c) = ch.clone() {
                                    ch = match c[0] {
                                        b'a'..=b'z' => Some(vec![c[0] - b'a' + 1]),
                                        b'[' => Some(vec![27]),
                                        b'\\' => Some(vec![28]),
                                        b']' => Some(vec![29]),
                                        b'^' => Some(vec![30]),
                                        b'_' => Some(vec![31]),
                                        _ => Some(vec![c[0]]),
                                    }
                                }
                            }
                            if let Some(ch) = ch {
                                    nix::unistd::write(pty.master, &ch)
                                        .unwrap();
                            }
                        }
                        Event::KeyUp { keycode: code, .. } => {
                            match code {
                                Some(Keycode::LShift) | Some(Keycode::RShift) => shift = false,
                                Some(Keycode::LCtrl) | Some(Keycode::RCtrl) => ctrl = false,
                                _ => {}
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
            std::env::set_var("TERM", "dumb");
            std::env::set_var("COLUMNS", &console.get_size().0.to_string());
            std::env::set_var("LINES", &console.get_size().1.to_string());

            unistd::execv(&path, &[]).unwrap();
        }
        Err(_) => {}
    }
}

fn main() {
    let pty = openpty().unwrap();
    start(&pty);
}
