

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Pos2, Ui};

pub const FAMILY: &str = "xz_icons";

pub const LOGO: char = '\u{E000}';
pub const COMBAT: char = '\u{E001}';
pub const MOVEMENT: char = '\u{E002}';
pub const RENDER: char = '\u{E003}';
pub const SKINS: char = '\u{E004}';
pub const MISC: char = '\u{E005}';
pub const FOLDER: char = '\u{E006}';
pub const FRIENDS: char = '\u{E007}';

const FONT_BYTES: &[u8] = include_bytes!("../../resources/menu_icons.ttf");

pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        FAMILY.to_owned(),
        std::sync::Arc::new(FontData::from_static(FONT_BYTES)),
    );
    fonts
        .families
        .entry(FontFamily::Name(FAMILY.into()))
        .or_default()
        .insert(0, FAMILY.to_owned());
    
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push(FAMILY.to_owned());
    ctx.set_fonts(fonts);
}

pub fn font_id(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY.into()))
}

pub fn paint(ui: &mut Ui, glyph: char, center: Pos2, size: f32, color: Color32) {
    let galley = ui.fonts(|f| f.layout_no_wrap(glyph.to_string(), font_id(size), color));
    let pos = Pos2::new(
        center.x - galley.size().x * 0.5,
        center.y - galley.size().y * 0.5,
    );
    ui.painter().galley(pos, galley, color);
}

pub fn measure(ui: &Ui, glyph: char, size: f32) -> egui::Vec2 {
    ui.fonts(|f| {
        f.layout_no_wrap(glyph.to_string(), font_id(size), Color32::WHITE)
            .size()
    })
}
