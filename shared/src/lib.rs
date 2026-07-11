use csscolorparser::Color;
use sha2::{Digest, Sha256};

pub mod account_data;
pub mod api;
pub mod commands;
pub mod parsing;
pub mod profile;
pub mod settings;
pub mod sidebar;
pub mod synth;
pub mod timeline;

pub trait ColorExt {
    fn set_lightness(&mut self, lightness: f32);
    fn set_alpha(&mut self, alpha: f32);
}

impl ColorExt for Color {
    fn set_lightness(&mut self, lightness: f32) {
        let [h, s, _, a] = self.to_hsla();

        *self = Color::from_hsla(h, s, lightness, a);
    }

    fn set_alpha(&mut self, alpha: f32) {
        let [h, s, l, _] = self.to_hsla();

        *self = Color::from_hsla(h, s, l, alpha);
    }
}

pub fn get_color(string: &str) -> Color {
    let hash = Sha256::digest(string.as_bytes());
    let h = hash[0] as f32 / 255.0 * 360.0;
    Color::from_hsla(h, 0.9, 0.7, 1.0)
}

pub fn unknown_color() -> Color {
    Color::from_hsla(0.0, 1.0, 0.7, 1.0)
}
