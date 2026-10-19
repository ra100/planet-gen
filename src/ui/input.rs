//! Input batching fix for slow frames.
//!
//! When a frame blocks on CPU work (terrain regeneration while a slider is
//! dragged), winit queues every event that arrives in the meantime and egui
//! receives them as one batch. egui resolves the drag-release using the
//! pointer position at the *end* of the batch, so a slider jumps to wherever
//! the cursor went after the button was let go, often clamping to 0 or 1.
//!
//! [`split_at_pointer_release`] ends the batch at the first button release
//! and carries the remaining events into the next frame, so egui sees the
//! release at the position where it actually happened.

use egui::Event;

/// Keep `events` up to and including the first pointer-button release that
/// is followed by further pointer activity; move everything after it into
/// `deferred`, preserving order. Returns `true` when events were deferred.
///
/// Call with `deferred` already drained into the front of `events`.
pub fn split_at_pointer_release(events: &mut Vec<Event>, deferred: &mut Vec<Event>) -> bool {
    let Some(release) = events
        .iter()
        .position(|e| matches!(e, Event::PointerButton { pressed: false, .. }))
    else {
        return false;
    };
    let pointer_after = events[release + 1..].iter().any(|e| {
        matches!(
            e,
            Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone
        )
    });
    if !pointer_after {
        return false;
    }
    deferred.extend(events.drain(release + 1..));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    /// Drives one slider through egui with the given per-frame event batches
    /// and returns its value after each frame.
    fn run_slider(frames: Vec<Vec<Event>>, split: bool) -> Vec<f32> {
        let ctx = egui::Context::default();
        let mut value = 0.5_f32;
        let mut deferred = Vec::new();
        let mut values = Vec::new();
        let mut queue = std::collections::VecDeque::from(frames);
        let mut frame = 0;
        while !queue.is_empty() || !deferred.is_empty() {
            let mut events: Vec<Event> = std::mem::take(&mut deferred);
            events.extend(queue.pop_front().unwrap_or_default());
            if split {
                split_at_pointer_release(&mut events, &mut deferred);
            }
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                time: Some(frame as f64 * 0.5),
                events,
                ..Default::default()
            };
            frame += 1;
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.spacing_mut().slider_width = 200.0;
                    ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
                });
            });
            values.push(value);
        }
        values
    }

    /// Slider rail spans roughly x = 8..208 at y ≈ 18 in an 800×600 panel.
    /// Press near the middle, drag, then — in one slow-frame batch — release
    /// mid-rail and keep moving the cursor far right.
    fn slow_release_frames() -> Vec<Vec<Event>> {
        let y = 18.0;
        vec![
            vec![Event::PointerMoved(pos2(108.0, y))],
            vec![button(pos2(108.0, y), true)],
            vec![Event::PointerMoved(pos2(130.0, y))],
            vec![
                Event::PointerMoved(pos2(150.0, y)),
                button(pos2(150.0, y), false),
                Event::PointerMoved(pos2(400.0, y)),
                Event::PointerMoved(pos2(700.0, y)),
            ],
            vec![Event::PointerMoved(pos2(750.0, y))],
        ]
    }

    #[test]
    fn unsplit_batch_snaps_slider_to_cursor_after_release() {
        let values = run_slider(slow_release_frames(), false);
        assert_eq!(*values.last().unwrap(), 1.0, "values: {values:?}");
    }

    #[test]
    fn split_batch_keeps_slider_at_release_position() {
        let values = run_slider(slow_release_frames(), true);
        let last = *values.last().unwrap();
        assert!(last > 0.6 && last < 0.8, "values: {values:?}");
    }

    #[test]
    fn no_split_without_pointer_activity_after_release() {
        let mut events = vec![
            Event::PointerMoved(pos2(1.0, 1.0)),
            button(pos2(1.0, 1.0), false),
            Event::Text("a".into()),
        ];
        let mut deferred = Vec::new();
        assert!(!split_at_pointer_release(&mut events, &mut deferred));
        assert_eq!(events.len(), 3);
        assert!(deferred.is_empty());
    }

    #[test]
    fn split_preserves_order_of_deferred_events() {
        let mut events = vec![
            button(pos2(1.0, 1.0), false),
            Event::Text("a".into()),
            Event::PointerMoved(pos2(2.0, 2.0)),
        ];
        let mut deferred = Vec::new();
        assert!(split_at_pointer_release(&mut events, &mut deferred));
        assert_eq!(events.len(), 1);
        assert_eq!(
            deferred,
            vec![Event::Text("a".into()), Event::PointerMoved(pos2(2.0, 2.0))]
        );
    }
}
