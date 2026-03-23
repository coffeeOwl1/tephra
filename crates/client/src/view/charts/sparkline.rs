use iced::mouse;
use iced::widget::canvas::{self, Cache, Canvas, Geometry, Stroke};
use iced::{Color, Element, Length, Rectangle, Renderer, Theme};

use crate::message::Message;
use crate::node::history::RingBuffer;

/// A compact sparkline using Canvas.
pub fn sparkline<'a>(
    data: &'a RingBuffer<f64>,
    color: Color,
    cache: &'a Cache,
) -> Element<'a, Message> {
    Canvas::new(SparklineProgram { data, color, cache })
        .width(Length::Fill)
        .height(32)
        .into()
}

struct SparklineProgram<'a> {
    data: &'a RingBuffer<f64>,
    color: Color,
    cache: &'a Cache,
}

impl<'a> canvas::Program<Message> for SparklineProgram<'a> {
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
            let pad = 2.0;
            let cw = w - pad * 2.0;
            let ch = h - pad * 2.0;

            let samples: Vec<f64> = self.data.iter().copied().collect();
            if samples.len() < 2 || cw <= 0.0 || ch <= 0.0 {
                return;
            }

            let y_min = samples.iter().copied().fold(f64::INFINITY, f64::min);
            let y_max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let range = (y_max - y_min).max(1.0);
            let n = samples.len();

            let mut builder = canvas::path::Builder::new();
            for (i, &val) in samples.iter().enumerate() {
                let x = pad + (i as f32 / (n - 1) as f32) * cw;
                let norm = ((val - y_min) / range).clamp(0.0, 1.0) as f32;
                let y = pad + ch * (1.0 - norm);
                if i == 0 {
                    builder.move_to(iced::Point::new(x, y));
                } else {
                    builder.line_to(iced::Point::new(x, y));
                }
            }

            frame.stroke(
                &builder.build(),
                Stroke::default().with_color(self.color).with_width(1.5),
            );
        });

        vec![geom]
    }
}
