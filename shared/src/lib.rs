use colorsys::Hsl;

pub mod account_data;
pub mod api;
pub mod commands;
pub mod parsing;
pub mod sidebar;
pub mod timeline;
pub mod user_profile;

pub fn get_color(string: &str) -> Hsl {
    let mut hash: u32 = 0;
    for c in string.chars() {
        hash = (c as u32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
    }

    let hue = hash % 360;

    Hsl::new(hue as f64, 90.0, 70.0, None)
}

pub fn unknown_color() -> Hsl {
    Hsl::new(0.0, 100.0, 70.0, None)
}
