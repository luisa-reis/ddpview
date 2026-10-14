//! The controls: an egui panel down the right of the window, which Tab
//! hides, and the notice over the picture when nothing is arriving.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use ddpview::panel::PanelType;
use ddpview::size::{Size, MAX_SIDE, PRESETS};
use egui::{Align2, Color32, RichText};

use crate::session::{Arriving, Session};
use crate::view::{Placed, Room};

/// The colour of something the user should look at.
const WARNING: Color32 = Color32::from_rgb(240, 180, 60);

#[derive(Resource)]
struct Controls {
    shown: bool,
    /// The port as typed, which is applied when the field is left.
    port: String,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup)
        .add_systems(Update, toggle)
        .add_systems(EguiPrimaryContextPass, draw);
}

fn setup(mut commands: Commands, session: Res<Session>) {
    commands.insert_resource(Controls {
        shown: true,
        port: session.port.to_string(),
    });
}

/// Tab hides and shows the panel, unless a field is being typed in.
fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mut controls: ResMut<Controls>,
    mut contexts: EguiContexts,
) {
    let typing = contexts
        .ctx_mut()
        .is_ok_and(|ctx| ctx.egui_wants_keyboard_input());
    if keys.just_pressed(KeyCode::Tab) && !typing {
        controls.shown = !controls.shown;
    }
}

fn draw(
    mut contexts: EguiContexts,
    mut controls: ResMut<Controls>,
    mut session: ResMut<Session>,
    mut room: ResMut<Room>,
    placed: Res<Placed>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut free = ctx.viewport_rect();
    if controls.shown {
        let mut whole = egui::Ui::new(
            ctx.clone(),
            "window".into(),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(free),
        );
        let panel = egui::Panel::right("controls")
            .resizable(false)
            .exact_size(230.0)
            .show(&mut whole, |ui| {
                panel(ui, &mut controls, &mut session, &placed)
            });
        free.max.x = panel.response.rect.min.x;
    }
    let free = free.shrink(12.0);
    room.min = Vec2::new(free.min.x, free.min.y);
    room.max = Vec2::new(free.max.x, free.max.y);
    notice(ctx, free, &session, controls.shown);
    Ok(())
}

fn panel(ui: &mut egui::Ui, controls: &mut Controls, session: &mut Session, placed: &Placed) {
    ui.add_space(6.0);
    ui.heading("Panel");
    panel_type(ui, session);
    size(ui, session);
    ui.label(RichText::new(scale_text(placed.scale)).weak());

    ui.add_space(10.0);
    ui.separator();
    ui.heading("Listening");
    listening(ui, controls, session);

    if !session.forward.is_empty() {
        ui.add_space(10.0);
        ui.separator();
        ui.heading("Forwarding");
        forwarding(ui, session);
    }

    ui.add_space(10.0);
    ui.separator();
    ui.heading("Arriving");
    arriving(ui, session);

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.add_space(6.0);
        ui.label(RichText::new("Tab hides these controls").weak());
    });
}

/// What the pixels look like.
fn panel_type(ui: &mut egui::Ui, session: &mut Session) {
    let mut picked = session.panel;
    egui::ComboBox::from_id_salt("panel type")
        .selected_text(picked.to_string())
        .width(160.0)
        .show_ui(ui, |ui| {
            for panel in PanelType::ALL {
                ui.selectable_value(&mut picked, panel, panel.to_string());
            }
        });
    session.set_panel(picked);
    if let Some(pitch) = picked.pitch_mm() {
        let (w, h) = (session.size.width, session.size.height);
        let text = format!(
            "{pitch} mm apart: {:.0} × {:.0} mm",
            w as f32 * pitch,
            h as f32 * pitch
        );
        ui.label(RichText::new(text).weak());
    }
}

/// The size: typed, picked from the presets, or guessed.
fn size(ui: &mut egui::Ui, session: &mut Session) {
    let (mut width, mut height) = (session.size.width, session.size.height);
    ui.horizontal(|ui| {
        let side = |value| {
            egui::DragValue::new(value)
                .range(1..=MAX_SIDE)
                .update_while_editing(false)
        };
        ui.add(side(&mut width));
        ui.label("×");
        ui.add(side(&mut height));
        ui.label("pixels");
    });
    let mut picked = Size::new(width, height);
    let known = PRESETS.contains(&session.size);
    egui::ComboBox::from_id_salt("presets")
        .selected_text(if known {
            session.size.to_string()
        } else {
            "Presets".into()
        })
        .height(400.0)
        .show_ui(ui, |ui| {
            for preset in PRESETS {
                ui.selectable_value(&mut picked, preset, preset.to_string());
            }
        });
    if picked != session.size {
        session.set_size(picked);
    }

    let mut auto = session.auto;
    ui.checkbox(&mut auto, "Guess from the frames");
    if auto != session.auto {
        session.set_auto(auto);
    }
    let pixels = session.frame.as_ref().map(|frame| frame.pixels());
    match (session.guess, pixels) {
        (Some((of, guess)), _) if guess.unused > 0 => {
            let text = format!(
                "{of} pixels are not a tidy panel: rounded up, the last {} are unused. Set the size.",
                guess.unused
            );
            ui.label(RichText::new(text).color(WARNING));
        }
        (Some((of, _)), _) => {
            ui.label(RichText::new(format!("Guessed from {of} pixels")).weak());
        }
        (None, _) if session.auto => {
            ui.label(RichText::new("Guessed when the first frame arrives").weak());
        }
        (None, Some(pixels)) if pixels != session.size.pixels() => {
            let text = format!(
                "Frames have {pixels} pixels, the panel {}",
                session.size.pixels()
            );
            ui.label(RichText::new(text).color(WARNING));
        }
        _ => {}
    }
}

fn scale_text(scale: f32) -> String {
    if scale >= 1.0 {
        format!("Shown at {scale:.0}×")
    } else {
        format!("Shown at {:.0}%: too big for the window", scale * 100.0)
    }
}

/// The port, which can be changed, and the address, which is `--bind`'s.
fn listening(ui: &mut egui::Ui, controls: &mut Controls, session: &mut Session) {
    ui.horizontal(|ui| {
        ui.label("UDP port");
        let field = ui.add(egui::TextEdit::singleline(&mut controls.port).desired_width(60.0));
        if field.lost_focus() {
            match controls.port.trim().parse::<u16>() {
                Ok(port) if port != 0 => session.set_port(port),
                _ => {}
            }
            controls.port = session.port.to_string();
        }
    });
    let reach = if session.bind.is_loopback() {
        "this computer only"
    } else if session.bind.is_unspecified() {
        "every network this computer is on"
    } else {
        "that network"
    };
    match &session.bind_error {
        Some(error) => {
            let text = format!("Cannot listen on {}: {error}", session.address());
            ui.label(RichText::new(text).color(WARNING));
        }
        None => {
            ui.label(format!("On {}", session.address()));
            ui.label(RichText::new(format!("Heard by {reach}")).weak());
        }
    }
    if session.bind.is_loopback() {
        ui.label(RichText::new("--bind 0.0.0.0 listens on the network").weak());
    }
}

/// Where packets are passed on to, from `--forward`, and how that goes.
fn forwarding(ui: &mut egui::Ui, session: &Session) {
    for target in &session.forward {
        ui.label(format!("To {target}"));
    }
    let status = &session.status;
    if status.forwarded > 0 {
        ui.label(RichText::new(format!("{} packets passed on", status.forwarded)).weak());
    }
    if let Some(error) = &status.forward_error {
        let text = format!(
            "{} of them not to every receiver. The last failure: {error}",
            status.forward_failed
        );
        ui.label(RichText::new(text).color(WARNING));
    }
}

/// The frame rate coming in and who is sending.
fn arriving(ui: &mut egui::Ui, session: &Session) {
    match session.arriving() {
        Arriving::NotListening => {
            ui.label("Not listening");
            return;
        }
        Arriving::Never => {
            ui.label(RichText::new("Nothing yet").color(WARNING));
        }
        Arriving::Stopped(since) => {
            ui.label(RichText::new(format!("Nothing for {}", ago(since.as_secs()))).color(WARNING));
        }
        Arriving::Yes(rate) => {
            ui.label(RichText::new(format!("{rate:.1} frames a second")).strong());
        }
    }
    if let Some(sender) = session.status.sender {
        ui.label(format!("From {sender}"));
    }
    if let Some(frame) = &session.frame {
        ui.label(RichText::new(format!("{} pixels a frame", frame.pixels())).weak());
    }
    if session.status.frames > 0 {
        ui.label(RichText::new(format!("{} frames so far", session.status.frames)).weak());
    }
    if session.status.rejected > 0 {
        let text = format!("{} packets that were not DDP", session.status.rejected);
        ui.label(RichText::new(text).color(WARNING));
    }
}

fn ago(seconds: u64) -> String {
    match seconds {
        0..120 => format!("{seconds} seconds"),
        120..7200 => format!("{} minutes", seconds / 60),
        _ => format!("{} hours", seconds / 3600),
    }
}

/// Says so over the picture when nothing is arriving: in the middle before
/// the first frame, along the top once there is a last frame to leave in
/// view.
fn notice(ctx: &egui::Context, free: egui::Rect, session: &Session, controls_shown: bool) {
    let address = session.address();
    let (anchor, at, title, detail) = match session.arriving() {
        Arriving::Yes(_) => {
            if !controls_shown {
                hint(ctx, free);
            }
            return;
        }
        Arriving::NotListening => {
            let error = session.bind_error.clone().unwrap_or_default();
            (
                Align2::CENTER_CENTER,
                free.center(),
                format!("Cannot listen on {address}"),
                error,
            )
        }
        Arriving::Never => (
            Align2::CENTER_CENTER,
            free.center(),
            "Nothing is arriving".to_string(),
            format!("Listening for DDP on UDP {address}"),
        ),
        Arriving::Stopped(since) => (
            Align2::CENTER_TOP,
            free.center_top() + egui::vec2(0.0, 8.0),
            format!("Nothing has arrived for {}", ago(since.as_secs())),
            format!("This is the last frame. Listening for DDP on UDP {address}"),
        ),
    };
    egui::Area::new("notice".into())
        .pivot(anchor)
        .fixed_pos(at)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                // An area is as narrow as it can be unless told otherwise.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(title).size(20.0).strong().color(WARNING));
                    ui.label(detail);
                    if !controls_shown {
                        ui.label(RichText::new("Tab shows the controls").weak());
                    }
                });
            });
        });
}

/// With the controls hidden, how to get them back.
fn hint(ctx: &egui::Context, free: egui::Rect) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let color = Color32::from_white_alpha(70);
    painter.text(
        free.right_bottom(),
        Align2::RIGHT_BOTTOM,
        "Tab: controls",
        egui::FontId::proportional(11.0),
        color,
    );
}
