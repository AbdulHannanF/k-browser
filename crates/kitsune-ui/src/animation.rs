// ARCHITECTURE: Lerp-based animations stored per-widget in egui::Context::data().
// State persists between frames via egui's temp data store.
// Only call ctx.request_repaint() when an animation is still running.

use eframe::egui;

/// Lerp-animate a f32 toward `target` at `speed` units/second.
/// Stores and reads state via `id` in egui's temp data. Requests repaint while unsettled.
pub fn lerp_anim(ctx: &egui::Context, id: egui::Id, target: f32, speed: f32) -> f32 {
    let dt   = ctx.input(|i| i.unstable_dt).min(0.1);
    let cur  = ctx.data(|d| d.get_temp::<f32>(id).unwrap_or(target));
    let diff = target - cur;
    let next = if diff.abs() < 0.002 {
        target
    } else {
        cur + diff * (speed * dt).min(1.0)
    };
    ctx.data_mut(|d| d.insert_temp(id, next));
    if (next - target).abs() > 0.002 {
        ctx.request_repaint();
    }
    next
}

/// Oscillate [0.0..1.0] at `hz` Hz. Only requests repaint when `active`.
/// Returns 0.5 when idle to avoid stuck appearances.
pub fn pulse_anim(ctx: &egui::Context, id: egui::Id, hz: f32, active: bool) -> f32 {
    if !active {
        return 0.5;
    }
    let dt    = ctx.input(|i| i.unstable_dt).min(0.1);
    let phase = ctx.data(|d| d.get_temp::<f32>(id).unwrap_or(0.0));
    let next  = (phase + dt * hz) % 1.0;
    ctx.data_mut(|d| d.insert_temp(id, next));
    ctx.request_repaint();
    (next * std::f32::consts::TAU).sin() * 0.5 + 0.5
}

/// Cycling spinner text character (four-step rotation, ~1 Hz).
pub fn spinner_char(ctx: &egui::Context) -> &'static str {
    let t = ctx.input(|i| i.time);
    ["◐", "◓", "◑", "◒"][(t * 4.0) as usize % 4]
}
