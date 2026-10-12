//! Performance overlay (FS-040 `view.debugOverlay`, SPEC 2.5): frame CPU time and
//! the latency of frames that handled input ("key to frame submit").

use std::collections::VecDeque;

const KEEP: usize = 240;

/// Rolling frame statistics.
#[derive(Debug, Default)]
pub struct Perf {
    frames: VecDeque<f32>,
    input_frames: VecDeque<f32>,
    pending_input: bool,
    last_was_input: bool,
}

fn percentile(v: &VecDeque<f32>, p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s: Vec<f32> = v.iter().copied().collect();
    s.sort_by(f32::total_cmp);
    s[((s.len() as f32 * p).ceil() as usize).clamp(1, s.len()) - 1]
}

impl Perf {
    /// Marks the start of a frame and whether it handles input.
    pub fn begin(&mut self, had_input: bool) {
        self.last_was_input = self.pending_input;
        self.pending_input = had_input;
    }

    /// Records the CPU time of the previous frame (milliseconds).
    pub fn record(&mut self, ms: f32) {
        self.frames.push_back(ms);
        if self.frames.len() > KEEP {
            self.frames.pop_front();
        }
        if self.pending_input {
            self.input_frames.push_back(ms);
            if self.input_frames.len() > KEEP {
                self.input_frames.pop_front();
            }
        }
    }

    /// p95 of input frames.
    pub fn input_p95(&self) -> f32 {
        percentile(&self.input_frames, 0.95)
    }

    /// Draws the overlay.
    pub fn overlay(&self, ctx: &egui::Context, editor_ms: f32) {
        egui::Area::new(egui::Id::new("inkra-perf"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 40.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let last = self.frames.back().copied().unwrap_or(0.0);
                    ui.monospace(format!(
                        "frame  {last:6.2} ms   p95 {:6.2} ms",
                        percentile(&self.frames, 0.95)
                    ));
                    ui.monospace(format!(
                        "input  p50 {:6.2} ms   p95 {:6.2} ms  (n={})",
                        percentile(&self.input_frames, 0.5),
                        self.input_p95(),
                        self.input_frames.len()
                    ));
                    ui.monospace(format!("editor {editor_ms:6.2} ms"));
                });
            });
    }
}
