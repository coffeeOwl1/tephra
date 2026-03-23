use iced::mouse;
use iced::widget::canvas::{self, Cache, Canvas, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Rectangle, Renderer, Size, Theme};

use crate::message::Message;
use crate::node::history::RingBuffer;
use crate::theme::colors;

/// A single data series for the multi-line chart.
pub struct ChartSeries<'a> {
    pub data: &'a RingBuffer<f64>,
    pub color: Color,
    pub label: String,
}

/// Configuration for the multi-line overlay chart.
pub struct MultiLineConfig {
    pub title: &'static str,
    pub unit: &'static str,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub threshold: Option<f64>,
}

const CHART_HEIGHT: f32 = 200.0;

/// Display multiple data series overlaid on the same chart.
pub fn multi_line_chart<'a>(
    series: Vec<ChartSeries<'a>>,
    config: &MultiLineConfig,
    cache: &'a Cache,
) -> Element<'a, Message> {
    Canvas::new(MultiLineProgram {
        series,
        config_title: config.title,
        config_unit: config.unit,
        config_y_min: config.y_min,
        config_y_max: config.y_max,
        config_threshold: config.threshold,
        cache,
    })
    .width(Length::Fill)
    .height(CHART_HEIGHT)
    .into()
}

struct MultiLineProgram<'a> {
    series: Vec<ChartSeries<'a>>,
    config_title: &'static str,
    config_unit: &'static str,
    config_y_min: Option<f64>,
    config_y_max: Option<f64>,
    config_threshold: Option<f64>,
    cache: &'a Cache,
}

impl<'a> canvas::Program<Message> for MultiLineProgram<'a> {
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

            // Collect all samples to compute shared Y range
            let all_samples: Vec<Vec<f64>> = self
                .series
                .iter()
                .map(|s| s.data.iter().copied().collect())
                .collect();

            let max_len = all_samples.iter().map(|s| s.len()).max().unwrap_or(0);
            if max_len < 2 {
                return;
            }

            // Compute Y range across all series
            let (y_min, y_max) = {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for samples in &all_samples {
                    for &v in samples {
                        if v < lo { lo = v; }
                        if v > hi { hi = v; }
                    }
                }
                if !lo.is_finite() || !hi.is_finite() {
                    (0.0, 100.0)
                } else {
                    let range = hi - lo;
                    let pad = if range < 1.0 { 2.0 } else { range * 0.15 };
                    let mut y_lo = (lo - pad).max(0.0);
                    let mut y_hi = hi + pad;
                    if let Some(t) = self.config_threshold {
                        if t > y_hi - pad { y_hi = t + pad; }
                    }
                    if let Some(f) = self.config_y_min { y_lo = f; }
                    if let Some(f) = self.config_y_max { y_hi = f; }
                    if y_hi <= y_lo { (y_lo, y_lo + 10.0) } else { (y_lo, y_hi) }
                }
            };
            let y_range = y_max - y_min;
            if y_range <= 0.0 { return; }

            let margin_left = 45.0_f32;
            let margin_top = 8.0_f32;
            let margin_bottom = 20.0_f32;
            let margin_right = 8.0_f32;
            let chart_w = w - margin_left - margin_right;
            let chart_h = h - margin_top - margin_bottom;
            if chart_w <= 0.0 || chart_h <= 0.0 { return; }

            let to_y = |v: f64| -> f32 {
                let norm = ((v - y_min) / y_range).clamp(0.0, 1.0) as f32;
                margin_top + chart_h * (1.0 - norm)
            };

            // Background
            let bg = Path::rectangle(
                iced::Point::new(margin_left, margin_top),
                Size::new(chart_w, chart_h),
            );
            frame.fill(&bg, colors::with_alpha(colors::OBSIDIAN, 0.5));

            // Grid lines + Y labels
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

            // Threshold line
            if let Some(thresh) = self.config_threshold {
                if thresh >= y_min && thresh <= y_max {
                    let ty = to_y(thresh);
                    let line = Path::line(
                        iced::Point::new(margin_left, ty),
                        iced::Point::new(margin_left + chart_w, ty),
                    );
                    frame.stroke(
                        &line,
                        Stroke::default()
                            .with_color(colors::with_alpha(colors::MAGMA, 0.3))
                            .with_width(1.0),
                    );
                }
            }

            // Draw each series
            for (idx, (samples, series)) in
                all_samples.iter().zip(self.series.iter()).enumerate()
            {
                let n = samples.len();
                if n < 2 { continue; }

                let to_x = |i: usize| -> f32 {
                    margin_left + (i as f32 / (max_len - 1) as f32) * chart_w
                };
                // Offset shorter series to align at the right edge
                let offset = max_len - n;

                // Area fill (subtle)
                let mut area = canvas::path::Builder::new();
                area.move_to(iced::Point::new(to_x(offset), margin_top + chart_h));
                area.line_to(iced::Point::new(to_x(offset), to_y(samples[0])));
                for i in 1..n {
                    area.line_to(iced::Point::new(to_x(offset + i), to_y(samples[i])));
                }
                area.line_to(iced::Point::new(to_x(offset + n - 1), margin_top + chart_h));
                area.close();
                frame.fill(&area.build(), colors::with_alpha(series.color, 0.08));

                // Line
                let mut line = canvas::path::Builder::new();
                line.move_to(iced::Point::new(to_x(offset), to_y(samples[0])));
                for i in 1..n {
                    line.line_to(iced::Point::new(to_x(offset + i), to_y(samples[i])));
                }
                frame.stroke(
                    &line.build(),
                    Stroke::default()
                        .with_color(series.color)
                        .with_width(1.5),
                );

                // Current value dot + label
                let last = samples[n - 1];
                let cx = to_x(offset + n - 1);
                let cy = to_y(last);
                frame.fill(
                    &Path::circle(iced::Point::new(cx, cy), 4.0),
                    colors::with_alpha(series.color, 0.3),
                );
                frame.fill(
                    &Path::circle(iced::Point::new(cx, cy), 2.5),
                    series.color,
                );

                // Legend entry on the right side
                let legend_y = margin_top + 14.0 + (idx as f32 * 14.0);
                frame.fill_text(canvas::Text {
                    content: format!("{}: {:.1}{}", series.label, last, self.config_unit),
                    position: iced::Point::new(w - margin_right - 4.0, legend_y),
                    color: series.color,
                    size: 10.0.into(),
                    align_x: iced::alignment::Horizontal::Right.into(),
                    ..Default::default()
                });
            }

            // Chart title
            frame.fill_text(canvas::Text {
                content: self.config_title.to_string(),
                position: iced::Point::new(margin_left + 4.0, margin_top + 4.0),
                color: colors::with_alpha(colors::PUMICE, 0.7),
                size: 12.0.into(),
                ..Default::default()
            });
        });

        vec![geom]
    }
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
