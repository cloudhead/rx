use crate::gfx::prelude::Rgba8;

#[derive(Default, Debug)]
pub struct Palette {
    pub colors: Vec<Rgba8>,
}

impl Palette {
    pub fn add(&mut self, color: Rgba8) {
        if !self.colors.contains(&color) {
            self.colors.push(color);
        }
    }

    pub fn gradient(&mut self, start: Rgba8, end: Rgba8, number: usize) {
        fn blend(start: u8, end: u8, coef: f32) -> u8 {
            (start as f32 * (1.0 - coef) + end as f32 * coef).round() as u8
        }

        let step: f32 = 1.0 / ((number - 1) as f32);
        for i in 0..number {
            let coef = i as f32 * step;
            let color: Rgba8 = Rgba8 {
                r: blend(start.r, end.r, coef),
                g: blend(start.g, end.g, coef),
                b: blend(start.b, end.b, coef),
                a: blend(start.a, end.a, coef),
            };

            self.colors.push(color);
        }
    }

    pub fn clear(&mut self) {
        self.colors.clear();
    }
}
