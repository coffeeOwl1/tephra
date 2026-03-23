use iced::mouse;
use iced::widget::canvas::{self, Cache, Canvas, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Rectangle, Renderer, Size, Theme};

use crate::message::Message;
use crate::node::history::RingBuffer;
use crate::theme::colors;

/// Configuration for a line chart.
#[derive(Clone)]
pub struct LineChartConfig {
    pub color: Color,
    pub label: &'static str,
    pub unit: &'static str,
    pub peak: Option<f64>,
    pub threshold: Option<f64>,
    /// Fixed Y-axis bounds. Auto-range is used for None.
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
}

const CHART_HEIGHT: f32 = 160.0;

/// Display a line chart using a Canvas widget with a persistent cache.
pub fn line_chart<'a>(
    data: &'a RingBuffer<f64>,
    config: &LineChartConfig,
    cache: &'a Cache,
) -> Element<'a, Message> {
    Canvas::new(LineChartProgram {
        data,
        config: config.clone(),
        cache,
    })
    .width(Length::Fill)
    .height(CHART_HEIGHT)
    .into()
}

struct LineChartProgram<'a> {
    data: &'a RingBuffer<f64>,
    config: LineChartConfig,
    cache: &'a Cache,
}

impl<'a> canvas::Program<Message> for LineChartProgram<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geom = self.cache.draw(renderer, bounds.size(), |frame| {
            let w = frame.width();
            let h = frame.height();
            if w <= 0.0 || h <= 0.0 {
                return;
            }

            let samples: Vec<f64> = self.data.iter().copied().collect();
            let n = samples.len();
            if n < 2 {
                return;
            }

            // Auto-range Y axis
            let (y_min, y_max) = auto_range(&samples, &self.config);
            let y_range = y_max - y_min;
            if y_range <= 0.0 {
                return;
            }

            let margin_left = 45.0_f32;
            let margin_top = 8.0_f32;
            let margin_bottom = 20.0_f32;
            let margin_right = 8.0_f32;
            let chart_w = w - margin_left - margin_right;
            let chart_h = h - margin_top - margin_bottom;

            if chart_w <= 0.0 || chart_h <= 0.0 {
                return;
            }

            let to_x = |i: usize| -> f32 {
                margin_left + (i as f32 / (n - 1) as f32) * chart_w
            };
            let to_y = |v: f64| -> f32 {
                let norm = ((v - y_min) / y_range).clamp(0.0, 1.0) as f32;
                margin_top + chart_h * (1.0 - norm)
            };

            // -- Chart background --
            let bg = Path::rectangle(
                iced::Point::new(margin_left, margin_top),
                Size::new(chart_w, chart_h),
            );
            frame.fill(&bg, colors::with_alpha(colors::OBSIDIAN, 0.5));

            // -- Grid lines + Y labels --
            let grid_step = nice_step(y_range, 4);
            if grid_step > 0.0 && grid_step.is_finite() {
                let grid_stroke = Stroke {
                    style: canvas::Style::Solid(colors::with_alpha(colors::SCORIA, 0.4)),
                    width: 0.5,
                    ..Default::default()
                };
                let mut val = (y_min / grid_step).ceil() * grid_step;
                while val <= y_max {
                    let y = to_y(val);
                    let line = Path::line(
                        iced::Point::new(margin_left, y),
                        iced::Point::new(w - margin_right, y),
                    );
                    frame.stroke(&line, grid_stroke.clone());

                    // Y label
                    frame.fill_text(canvas::Text {
                        content: format!("{:.0}", val),
                        position: iced::Point::new(margin_left - 6.0, y),
                        color: colors::TEPHRA,
                        size: 10.0.into(),
                        align_x: iced::alignment::Horizontal::Right.into(),
                        align_y: iced::alignment::Vertical::Center,
                        ..Default::default()
                    });
                    val += grid_step;
                }
            }

            // -- Threshold line --
            if let Some(thresh) = self.config.threshold {
                if thresh >= y_min && thresh <= y_max {
                    let ty = to_y(thresh);
                    let line = Path::line(
                        iced::Point::new(margin_left, ty),
                        iced::Point::new(margin_left + chart_w, ty),
                    );
                    frame.stroke(
                        &line,
                        canvas::Stroke::default()
                            .with_color(colors::with_alpha(colors::MAGMA, 0.3))
                            .with_width(1.0),
                    );
                }
            }

            // -- Area fill --
            let mut area = canvas::path::Builder::new();
            area.move_to(iced::Point::new(to_x(0), margin_top + chart_h));
            area.line_to(iced::Point::new(to_x(0), to_y(samples[0])));
            for i in 1..n {
                area.line_to(iced::Point::new(to_x(i), to_y(samples[i])));
            }
            area.line_to(iced::Point::new(to_x(n - 1), margin_top + chart_h));
            area.close();
            frame.fill(&area.build(), colors::with_alpha(self.config.color, 0.15));

            // -- Line --
            let mut line = canvas::path::Builder::new();
            line.move_to(iced::Point::new(to_x(0), to_y(samples[0])));
            for i in 1..n {
                line.line_to(iced::Point::new(to_x(i), to_y(samples[i])));
            }
            frame.stroke(
                &line.build(),
                Stroke::default()
                    .with_color(self.config.color)
                    .with_width(2.0),
            );

            // -- Peak line --
            if let Some(peak) = self.config.peak {
                if peak >= y_min && peak <= y_max {
                    let py = to_y(peak);
                    let peak_line = Path::line(
                        iced::Point::new(margin_left, py),
                        iced::Point::new(w - margin_right, py),
                    );
                    frame.stroke(
                        &peak_line,
                        Stroke {
                            style: canvas::Style::Solid(colors::with_alpha(self.config.color, 0.35)),
                            width: 1.0,
                            ..Default::default()
                        },
                    );
                }
            }

            // -- Current value dot --
            let last = samples[n - 1];
            let cx = to_x(n - 1);
            let cy = to_y(last);
            frame.fill(
                &Path::circle(iced::Point::new(cx, cy), 6.0),
                colors::with_alpha(self.config.color, 0.25),
            );
            frame.fill(
                &Path::circle(iced::Point::new(cx, cy), 3.0),
                self.config.color,
            );

            // -- Current value text --
            frame.fill_text(canvas::Text {
                content: format!("{:.1}{}", last, self.config.unit),
                position: iced::Point::new(cx - 8.0, cy - 12.0),
                color: self.config.color,
                size: 11.0.into(),
                align_x: iced::alignment::Horizontal::Right.into(),
                ..Default::default()
            });

            // -- Chart title --
            frame.fill_text(canvas::Text {
                content: self.config.label.to_string(),
                position: iced::Point::new(margin_left + 4.0, margin_top + 4.0),
                color: colors::with_alpha(self.config.color, 0.7),
                size: 12.0.into(),
                ..Default::default()
            });
        });

        vec![geom]
    }
}

fn auto_range(samples: &[f64], config: &LineChartConfig) -> (f64, f64) {
    // If both fixed bounds are set, use them directly
    if let (Some(fixed_lo), Some(fixed_hi)) = (config.y_min, config.y_max) {
        return (fixed_lo, fixed_hi);
    }

    let threshold = config.threshold;
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in samples {
        if v < lo { lo = v; }
        if v > hi { hi = v; }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 100.0);
    }
    if let Some(t) = threshold {
        if t > hi && t < hi * 1.5 + 10.0 {
            hi = t;
        }
    }
    let range = hi - lo;
    let pad = if range < 1.0 { 2.0 } else { range * 0.15 };
    let mut y_lo = (lo - pad).max(0.0);
    let mut y_hi = hi + pad;

    // Apply single-sided fixed bounds
    if let Some(fixed_lo) = config.y_min {
        y_lo = fixed_lo;
    }
    if let Some(fixed_hi) = config.y_max {
        y_hi = fixed_hi;
    }

    if y_hi <= y_lo { (y_lo, y_lo + 10.0) } else { (y_lo, y_hi) }
}

fn nice_step(range: f64, target: u32) -> f64 {
    if range <= 0.0 || target == 0 { return 1.0; }
    let rough = range / target as f64;
    if rough <= 0.0 || !rough.is_finite() { return 1.0; }
    let mag = 10f64.powf(rough.log10().floor());
    if mag <= 0.0 || !mag.is_finite() { return 1.0; }
    let n = rough / mag;
    let nice = if n <= 1.5 { 1.0 } else if n <= 3.5 { 2.0 } else if n <= 7.5 { 5.0 } else { 10.0 };
    nice * mag
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> LineChartConfig {
        LineChartConfig {
            color: Color::WHITE,
            label: "Test",
            unit: "",
            peak: None,
            threshold: None,
            y_min: None,
            y_max: None,
        }
    }

    #[test]
    fn auto_range_normal() {
        let (lo, hi) = auto_range(&[45.0, 50.0, 55.0], &default_config());
        assert!(lo < 45.0);
        assert!(hi > 55.0);
    }

    #[test]
    fn auto_range_flat() {
        let (lo, hi) = auto_range(&[50.0, 50.0, 50.0], &default_config());
        assert!(lo < 50.0);
        assert!(hi > 50.0);
    }

    #[test]
    fn auto_range_fixed_bounds() {
        let mut cfg = default_config();
        cfg.y_min = Some(25.0);
        cfg.y_max = Some(100.0);
        let (lo, hi) = auto_range(&[50.0, 55.0], &cfg);
        assert_eq!(lo, 25.0);
        assert_eq!(hi, 100.0);
    }

    #[test]
    fn nice_step_sane() {
        assert!(nice_step(100.0, 4) > 0.0);
        assert_eq!(nice_step(0.0, 4), 1.0);
        assert_eq!(nice_step(-1.0, 4), 1.0);
    }
}
