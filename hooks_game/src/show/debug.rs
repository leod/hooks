use nalgebra::{Point2, Vector2};

use ggez;
use ggez::graphics::{self, Font, Text};

use common::debug;

const SUCC_MARGIN: f32 = 10.0;
const NAME_MARGIN: f32 = 30.0;

pub fn show(
    ctx: &mut ggez::Context,
    font: &Font,
    vars: &debug::Vars,
    pos: Point2<f32>,
) -> ggez::GameResult<Point2<f32>> {
    match vars {
        &debug::Vars::Leaf(ref string) => {
            let text = Text::new(ctx, &string, font)?;
            graphics::draw(ctx, &text, pos, 0.0)?;
            Ok(Point2::new(text.width() as f32, text.height() as f32))
        }
        &debug::Vars::Node(ref succs) => {
            let mut name_texts = Vec::new();
            let mut max_width: f32 = 0.0;
            for &(ref name, _) in succs {
                let text = Text::new(ctx, &name, font)?;
                max_width = max_width.max(text.width() as f32);
                //debug!("w: {}", text.width());
                name_texts.push(text);
            }

            let name_width = NAME_MARGIN + max_width;

            let mut cur_pos = pos;
            for (&(_, ref succ_vars), text) in succs.iter().zip(&name_texts) {
                let succ_pos = cur_pos + Vector2::new(name_width, 0.0);
                graphics::draw(ctx, text, cur_pos, 0.0)?;

                let succ_size = show(ctx, font, succ_vars, succ_pos)?;
                cur_pos.coords.y += succ_size.y + SUCC_MARGIN;
            }

            Ok(cur_pos)
        }
    }
}
