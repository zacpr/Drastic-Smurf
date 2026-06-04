use egui::{Color32, Context, Pos2, Rect, Vec2};

use crate::core::config::{BackgroundEffect, VfxSettings};
use crate::ui::theme::Theme;

/// Paint the background visual effect behind the main UI.
/// Call this before rendering the central panel.
pub fn paint_background(ctx: &Context, settings: &VfxSettings, rect: Rect) {
    match settings.background_effect {
        BackgroundEffect::None => {}
        BackgroundEffect::Gradient => {
            paint_gradient(ctx, rect, settings);
            if !settings.reduce_motion {
                ctx.request_repaint();
            }
        }
        BackgroundEffect::Mesh => {
            paint_mesh(ctx, rect, settings);
            if !settings.reduce_motion {
                ctx.request_repaint();
            }
        }
        BackgroundEffect::Particles => {
            paint_particles(ctx, rect, settings);
            if !settings.reduce_motion {
                ctx.request_repaint();
            }
        }
    }
}

/// Paint a glowing spotlight backlight that follows the cursor under the widgets.
pub fn paint_cursor_glow(ctx: &Context, settings: &VfxSettings, _rect: Rect) {
    if !settings.cursor_glow || settings.reduce_motion {
        return;
    }

    if let Some(mouse_pos) = ctx.input(|i| i.pointer.latest_pos()) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        let accent = Theme::accent();

        let glow_radius = 120.0;
        let steps = 12;
        for i in 0..steps {
            let f = i as f32 / steps as f32;
            let radius = glow_radius * (1.0 - f * 0.85);
            let alpha = ((1.0 - f) * settings.background_intensity * 25.0) as u8;
            let color = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha);
            painter.circle_filled(mouse_pos, radius, color);
        }

        // Request repaint so the cursor glow updates smoothly in real time
        ctx.request_repaint();
    }
}

fn paint_gradient(ctx: &Context, rect: Rect, settings: &VfxSettings) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let intensity = settings.background_intensity;
    if intensity <= 0.0 {
        return;
    }

    let time = if settings.reduce_motion {
        0.0
    } else {
        ctx.input(|i| i.time as f32) * settings.animation_speed * 0.1
    };

    let accent = Theme::accent();

    // Mouse parallax offset calculation
    let parallax_offset = if settings.reduce_motion || settings.parallax_amount <= 0.0 {
        Vec2::ZERO
    } else {
        ctx.input(|i| {
            if let Some(mouse_pos) = i.pointer.latest_pos() {
                let center = rect.center();
                let delta = mouse_pos - center;
                delta * settings.parallax_amount * 0.05
            } else {
                Vec2::ZERO
            }
        })
    };

    // Create a soft radial-ish gradient by painting a large quad with color variation
    let center = rect.center();
    let t1 = (time.sin() + 1.0) / 2.0;
    let t2 = ((time + 2.0).sin() + 1.0) / 2.0;

    let offset1 = Vec2::new(
        (t1 * 2.0 - 1.0) * rect.width() * 0.3,
        (t2 * 2.0 - 1.0) * rect.height() * 0.3,
    );

    let glow_center = Pos2::new(
        center.x + offset1.x - parallax_offset.x,
        center.y + offset1.y - parallax_offset.y,
    );

    let glow_radius = rect.width().min(rect.height()) * 0.6;

    // Paint a soft circle gradient by drawing concentric circles with decreasing alpha
    let steps = 20;
    for i in 0..steps {
        let f = i as f32 / steps as f32;
        let radius = glow_radius * (1.0 - f * 0.8);
        let alpha = ((1.0 - f) * intensity * 30.0) as u8;
        let color = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha);
        painter.circle_filled(glow_center, radius, color);
    }

    // Second, smaller accent spot
    let offset2 = Vec2::new(
        ((time + std::f32::consts::PI).sin() + 1.0) / 2.0 * 2.0 - 1.0,
        ((time + 1.57).cos() + 1.0) / 2.0 * 2.0 - 1.0,
    ) * rect.width()
        * 0.25;

    let glow_center2 = Pos2::new(
        center.x + offset2.x - parallax_offset.x,
        center.y + offset2.y - parallax_offset.y,
    );
    for i in 0..steps {
        let f = i as f32 / steps as f32;
        let radius = glow_radius * 0.5 * (1.0 - f * 0.8);
        let alpha = ((1.0 - f) * intensity * 20.0) as u8;
        let color = Color32::from_rgba_premultiplied(
            accent.r().saturating_add(40),
            accent.g().saturating_add(20),
            accent.b(),
            alpha,
        );
        painter.circle_filled(glow_center2, radius, color);
    }
}

fn paint_mesh(ctx: &Context, rect: Rect, settings: &VfxSettings) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let intensity = settings.background_intensity;
    if intensity <= 0.0 {
        return;
    }

    let time = if settings.reduce_motion {
        0.0
    } else {
        ctx.input(|i| i.time as f32) * settings.animation_speed * 0.05
    };
    let accent = Theme::accent();

    // Mouse parallax offset calculation
    let parallax_offset = if settings.reduce_motion || settings.parallax_amount <= 0.0 {
        Vec2::ZERO
    } else {
        ctx.input(|i| {
            if let Some(mouse_pos) = i.pointer.latest_pos() {
                let center = rect.center();
                let delta = mouse_pos - center;
                delta * settings.parallax_amount * 0.05
            } else {
                Vec2::ZERO
            }
        })
    };

    // Draw a subtle grid/mesh of lines
    let spacing = 60.0;
    let alpha = (intensity * 25.0) as u8;
    let line_color = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha);

    // Vertical lines with wave offset and parallax shift
    let cols = (rect.width() / spacing).ceil() as i32 + 2;
    let wave_amp = 15.0 * intensity;

    for i in -1..cols {
        let base_x = rect.min.x + i as f32 * spacing - parallax_offset.x;
        let mut points = Vec::new();
        let rows = (rect.height() / 10.0).ceil() as i32;
        for j in 0..=rows {
            let y = rect.min.y + j as f32 * 10.0;
            let wave = (y * 0.02 + time + i as f32 * 0.5).sin() * wave_amp;
            points.push(Pos2::new(base_x + wave, y));
        }
        if points.len() >= 2 {
            for k in 0..points.len() - 1 {
                painter.line_segment(
                    [points[k], points[k + 1]],
                    egui::Stroke::new(0.5, line_color),
                );
            }
        }
    }

    // Horizontal lines with wave offset and parallax shift
    let rows = (rect.height() / spacing).ceil() as i32 + 2;
    for j in -1..rows {
        let base_y = rect.min.y + j as f32 * spacing - parallax_offset.y;
        let mut points = Vec::new();
        let cols = (rect.width() / 10.0).ceil() as i32;
        for i in 0..=cols {
            let x = rect.min.x + i as f32 * 10.0;
            let wave = (x * 0.02 + time + j as f32 * 0.5).cos() * wave_amp;
            points.push(Pos2::new(x, base_y + wave));
        }
        if points.len() >= 2 {
            for k in 0..points.len() - 1 {
                painter.line_segment(
                    [points[k], points[k + 1]],
                    egui::Stroke::new(0.5, line_color),
                );
            }
        }
    }
}

// --- Dynamic Interactive Neural Constellation Particle System ---

struct SimpleRng {
    state: u32,
}

impl SimpleRng {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        self.state
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }

    fn gen_range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

#[derive(Clone)]
struct Particle {
    pos: Pos2,
    vel: Vec2,
    size: f32,
    alpha_phase: f32,
    alpha_speed: f32,
}

std::thread_local! {
    static PARTICLES: std::cell::RefCell<Vec<Particle>> = const { std::cell::RefCell::new(Vec::new()) };
    static LAST_TIME: std::cell::RefCell<Option<f32>> = const { std::cell::RefCell::new(None) };
}

fn paint_particles(ctx: &Context, rect: Rect, settings: &VfxSettings) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let intensity = settings.background_intensity;
    if intensity <= 0.0 {
        return;
    }

    let current_time = ctx.input(|i| i.time as f32);

    // Calculate precise delta time to ensure smooth particle drift independent of frame drops
    let mut dt = LAST_TIME.with(|t| {
        let mut last = t.borrow_mut();
        if let Some(prev) = *last {
            let diff = current_time - prev;
            *last = Some(current_time);
            diff.clamp(0.0, 0.1) // prevent massive jumps on frame lag
        } else {
            *last = Some(current_time);
            0.0
        }
    });

    if settings.reduce_motion {
        dt = 0.0;
    } else {
        dt *= settings.animation_speed;
    }

    let accent = Theme::accent();
    let num_particles = 70;

    PARTICLES.with(|p| {
        let mut particles = p.borrow_mut();

        // Spawn particles inside the active view bounds on first execution
        if particles.is_empty() {
            let mut rng = SimpleRng::new(54321);
            for _ in 0..num_particles {
                particles.push(Particle {
                    pos: Pos2::new(
                        rng.gen_range(rect.min.x, rect.max.x),
                        rng.gen_range(rect.min.y, rect.max.y),
                    ),
                    vel: Vec2::new(rng.gen_range(-12.0, 12.0), rng.gen_range(-12.0, 12.0)),
                    size: rng.gen_range(1.2, 3.2),
                    alpha_phase: rng.gen_range(0.0, std::f32::consts::TAU),
                    alpha_speed: rng.gen_range(1.0, 2.5),
                });
            }
        }

        // Parallax offset and cursor repulsion calculations
        let mouse_pos = ctx.input(|i| i.pointer.latest_pos());
        let parallax_offset = if settings.reduce_motion || settings.parallax_amount <= 0.0 {
            Vec2::ZERO
        } else {
            ctx.input(|i| {
                if let Some(mpos) = i.pointer.latest_pos() {
                    let center = rect.center();
                    let delta = mpos - center;
                    delta * settings.parallax_amount * 0.02
                } else {
                    Vec2::ZERO
                }
            })
        };

        // Update positions, wrap boundaries, and calculate cursor field repulsion
        for particle in particles.iter_mut() {
            // Apply drift velocity
            particle.pos += particle.vel * dt;

            // Apply parallax shift relative to motion
            particle.pos -= parallax_offset * dt;

            // Gentle push away from cursor to create interactive sweeping ripples
            if let Some(mpos) = mouse_pos {
                let to_mouse = mpos - particle.pos;
                let dist = to_mouse.length();
                if dist < 120.0 && dist > 1.0 {
                    let force = (1.0 - dist / 120.0) * 35.0;
                    let dir = to_mouse.normalized();
                    particle.pos -= dir * force * dt;
                }
            }

            // Boundary wrapping
            if particle.pos.x < rect.min.x {
                particle.pos.x = rect.max.x;
            } else if particle.pos.x > rect.max.x {
                particle.pos.x = rect.min.x;
            }
            if particle.pos.y < rect.min.y {
                particle.pos.y = rect.max.y;
            } else if particle.pos.y > rect.max.y {
                particle.pos.y = rect.min.y;
            }
        }

        // 1. Draw neural constellation connection segments between close nodes
        let line_max_dist = 90.0;
        let len = particles.len();
        for i in 0..len {
            for j in (i + 1)..len {
                let p1 = &particles[i];
                let p2 = &particles[j];
                let dist = p1.pos.distance(p2.pos);
                if dist < line_max_dist {
                    let pct = 1.0 - (dist / line_max_dist);
                    let alpha = (pct * pct * intensity * 15.0) as u8;
                    if alpha > 0 {
                        let color = Color32::from_rgba_premultiplied(
                            accent.r(),
                            accent.g(),
                            accent.b(),
                            alpha,
                        );
                        painter.line_segment([p1.pos, p2.pos], egui::Stroke::new(0.5, color));
                    }
                }
            }
        }

        // 2. Draw the floating particle stars themselves with organic pulsing halos
        for particle in particles.iter() {
            let pulse =
                (particle.alpha_phase + current_time * particle.alpha_speed).sin() * 0.35 + 0.65;
            let alpha = (pulse * intensity * 75.0) as u8;
            if alpha > 0 {
                let color =
                    Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha);
                painter.circle_filled(particle.pos, particle.size, color);

                // Halo glow ring for larger particle stars
                if particle.size > 2.2 {
                    let halo_alpha = (alpha as f32 * 0.2) as u8;
                    let halo_color = Color32::from_rgba_premultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        halo_alpha,
                    );
                    painter.circle_filled(particle.pos, particle.size * 2.5, halo_color);
                }
            }
        }
    });
}
