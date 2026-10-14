//! The window: a Bevy app that shows what the listener receives. The
//! receiving itself is the library's (`src/lib.rs`), which has no Bevy in
//! it; here are the session that ties it to the settings (`session`), the
//! picture (`view`) and the controls (`controls`).

mod controls;
mod session;
mod view;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent};
use bevy_egui::EguiPlugin;
use ddpview::cli::{self, Command};
use ddpview::settings::Settings;

use session::Session;

/// How often the window redraws when nothing makes it: often enough for
/// "nothing for 5 seconds" to count up.
const IDLE: Duration = Duration::from_millis(250);

/// The threads Bevy shares its systems between.
const THREADS: usize = 3;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match cli::parse(&args) {
        Ok(Command::Run(options)) => options,
        Ok(Command::Help) => {
            println!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("ddpview {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("ddpview: {e:#}");
            return ExitCode::from(2);
        }
    };
    let path = Settings::path();
    let (settings, warning) = match &path {
        Some(path) => Settings::load(path),
        None => (Settings::default(), None),
    };
    if let Some(warning) = warning {
        eprintln!("ddpview: using the default settings ({warning})");
    }

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "ddpview".into(),
                    resolution: (920, 680).into(),
                    ..default()
                }),
                ..default()
            })
            // A handful of systems, a sprite and a panel: a thread for
            // every core only spends processor time waking them all up
            // for each frame.
            .set(TaskPoolPlugin {
                task_pool_options: TaskPoolOptions::with_num_threads(THREADS),
            })
            // Nearest-neighbour sampling: a panel's pixels are squares,
            // never blended into their neighbours.
            .set(ImagePlugin::default_nearest()),
    )
    .add_plugins(EguiPlugin::default())
    .insert_resource(ClearColor(Color::srgb_u8(24, 24, 27)))
    // The window is redrawn when a frame arrives, when it is touched
    // and a few times a second besides, focused or not. Bevy's
    // default, as fast as the screen refreshes, costs a third of a
    // processor core to show a picture that has not changed.
    .insert_resource(WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(IDLE),
        unfocused_mode: UpdateMode::reactive_low_power(IDLE),
    });
    // The listener's thread wakes the window through winit's event loop.
    let proxy = (**app.world().resource::<EventLoopProxyWrapper>()).clone();
    let wake = Arc::new(move || {
        // It fails only once the window has gone, with nothing to wake.
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    app.insert_resource(Session::new(options, settings, path, wake))
        .add_plugins((session::plugin, view::plugin, controls::plugin))
        .run();
    ExitCode::SUCCESS
}
