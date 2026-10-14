//! One run of the program: the listener, the panel's size and the last
//! frame, and what of that is remembered. The controls change it through
//! the methods here, and the view draws from it.

use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use ddpview::cli::Options;
use ddpview::ddp::Frame;
use ddpview::listener::{Listener, Status};
use ddpview::panel::PanelType;
use ddpview::settings::Settings;
use ddpview::size::{guess, Guess, Size};

/// How long without a frame before the stream counts as stopped: the time
/// after which WLED gives up on a sender and goes back to its presets.
const STOPPED_AFTER: Duration = Duration::from_millis(2500);
/// The stretch of time the frame rate is measured over.
const RATE_OVER: Duration = Duration::from_secs(2);

/// Whether frames are coming in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arriving {
    /// The socket could not be bound.
    NotListening,
    /// Nothing since the socket was bound.
    Never,
    /// Frames came, the last this long ago.
    Stopped(Duration),
    /// Frames are coming, this many a second.
    Yes(f32),
}

#[derive(Resource)]
pub struct Session {
    listener: Option<Listener>,
    /// Why there is no listener.
    pub bind_error: Option<String>,
    pub bind: IpAddr,
    pub port: u16,
    /// Receivers every packet is sent on to, from `--forward`.
    pub forward: Vec<SocketAddr>,
    pub size: Size,
    /// Whether the size follows a guess from the frames.
    pub auto: bool,
    /// What the pixels look like.
    pub panel: PanelType,
    /// For LED panel types: a halo around each LED, and brighter LEDs.
    pub led_halo: bool,
    pub led_brighter: bool,
    /// The guess in force, and the pixel count it was made for.
    pub guess: Option<(usize, Guess)>,
    /// The last frame shown.
    pub frame: Option<Frame>,
    /// Set when the frame or the size changed, for the view to clear.
    pub redraw: bool,
    pub status: Status,
    /// The frame count at moments over the last `RATE_OVER`.
    samples: VecDeque<(Instant, u64)>,
    /// What the settings file holds, which `--size` and `--port` do not
    /// change.
    remembered: Settings,
    path: Option<PathBuf>,
    /// Wakes the window, which otherwise sleeps until something happens to
    /// it; the listener calls this for each frame.
    wake: Wake,
}

pub type Wake = Arc<dyn Fn() + Send + Sync>;

impl Session {
    pub fn new(options: Options, settings: Settings, path: Option<PathBuf>, wake: Wake) -> Self {
        let mut session = Self {
            listener: None,
            bind_error: None,
            bind: options.bind,
            port: options.port.unwrap_or(settings.port),
            forward: options.forward,
            size: options.size.unwrap_or(settings.size()),
            auto: options.size.is_none() && settings.auto_size,
            panel: settings.panel,
            led_halo: settings.led_halo,
            led_brighter: settings.led_brighter,
            guess: None,
            frame: None,
            redraw: true,
            status: Status::default(),
            samples: VecDeque::new(),
            remembered: settings,
            path,
            wake,
        };
        session.listen();
        session
    }

    /// Binds the socket, letting go of the old one first: it may hold the
    /// very port asked for. What was shown came from the old one, and goes
    /// with it.
    fn listen(&mut self) {
        self.listener = None;
        self.status = Status::default();
        self.samples.clear();
        self.frame = None;
        self.guess = None;
        self.redraw = true;
        let wake = Arc::clone(&self.wake);
        let address = SocketAddr::new(self.bind, self.port);
        match Listener::bind(address, &self.forward, move || wake()) {
            Ok(listener) => {
                self.listener = Some(listener);
                self.bind_error = None;
            }
            Err(e) => {
                eprintln!("ddpview: cannot listen on {address}: {e}");
                self.bind_error = Some(e.to_string());
            }
        }
    }

    /// Listens on another port, and remembers it if that worked.
    pub fn set_port(&mut self, port: u16) {
        if port == self.port && self.listener.is_some() {
            return;
        }
        self.port = port;
        self.listen();
        if self.listener.is_some() {
            self.remembered.port = port;
            self.save();
        }
    }

    /// Holds the panel at a size.
    pub fn set_size(&mut self, size: Size) {
        if !size.fits() || (size == self.size && !self.auto) {
            return;
        }
        self.auto = false;
        self.guess = None;
        self.resize(size);
    }

    /// Guesses the size from the frames, or stops and keeps the size it has.
    pub fn set_auto(&mut self, auto: bool) {
        if auto == self.auto {
            return;
        }
        self.auto = auto;
        self.guess = None;
        self.follow_guess();
        self.resize(self.size);
    }

    /// Shows the panel as another type, and remembers it.
    pub fn set_panel(&mut self, panel: PanelType) {
        if panel == self.panel {
            return;
        }
        self.panel = panel;
        self.redraw = true;
        self.remembered.panel = panel;
        self.save();
    }

    /// Turns the LEDs' halo on or off, and remembers it.
    pub fn set_led_halo(&mut self, on: bool) {
        if on == self.led_halo {
            return;
        }
        self.led_halo = on;
        self.redraw = true;
        self.remembered.led_halo = on;
        self.save();
    }

    /// Makes the LEDs brighter than a screen's white allows, or not, and
    /// remembers it.
    pub fn set_led_brighter(&mut self, on: bool) {
        if on == self.led_brighter {
            return;
        }
        self.led_brighter = on;
        self.redraw = true;
        self.remembered.led_brighter = on;
        self.save();
    }

    /// In auto, takes the size guessed for the last frame's pixel count,
    /// when that count is not the one already guessed for.
    fn follow_guess(&mut self) {
        let Some(pixels) = self.frame.as_ref().map(Frame::pixels) else {
            return;
        };
        if !self.auto || self.guess.is_some_and(|(of, _)| of == pixels) {
            return;
        }
        if let Some(guess) = guess(pixels) {
            self.guess = Some((pixels, guess));
            self.resize(guess.size);
        }
    }

    fn resize(&mut self, size: Size) {
        self.size = size;
        self.redraw = true;
        let remembered = Settings {
            auto_size: self.auto,
            width: size.width,
            height: size.height,
            ..self.remembered
        };
        if remembered != self.remembered {
            self.remembered = remembered;
            self.save();
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Err(e) = self.remembered.save(path) {
            eprintln!("ddpview: cannot save {}: {e}", path.display());
        }
    }

    /// The address listened on, as a sender would be given it.
    pub fn address(&self) -> SocketAddr {
        match &self.listener {
            Some(listener) => listener.local(),
            None => SocketAddr::new(self.bind, self.port),
        }
    }

    pub fn arriving(&self) -> Arriving {
        if self.listener.is_none() {
            return Arriving::NotListening;
        }
        let Some(last) = self.status.last_frame else {
            return Arriving::Never;
        };
        let since = last.elapsed();
        if since > STOPPED_AFTER {
            return Arriving::Stopped(since);
        }
        Arriving::Yes(self.rate())
    }

    /// Frames a second, over the samples kept.
    fn rate(&self) -> f32 {
        let (Some(&(then, before)), Some(&(now, after))) =
            (self.samples.front(), self.samples.back())
        else {
            return 0.0;
        };
        let seconds = now.duration_since(then).as_secs_f32();
        if seconds <= 0.0 {
            return 0.0;
        }
        (after - before) as f32 / seconds
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, collect);
}

/// Each time round, takes what the listener has: its counts, and the
/// newest frame if there is one. This only ever takes a lock held for the
/// time it takes to copy a frame, so the socket cannot hold it up.
fn collect(mut session: ResMut<Session>) {
    let Some(listener) = &session.listener else {
        return;
    };
    let (status, frame) = (listener.status(), listener.take_frame());
    let now = Instant::now();
    // Not through `ResMut` unless something changed: change detection is
    // how the rest finds out.
    let session = session.bypass_change_detection();
    session.samples.push_back((now, status.frames));
    session.status = status;
    while session.samples.len() > 2 && now.duration_since(session.samples[1].0) >= RATE_OVER {
        session.samples.pop_front();
    }
    if let Some(frame) = frame {
        session.frame = Some(frame);
        session.redraw = true;
        session.follow_guess();
    }
}
