use egui::{Color32, Painter, Rect, Stroke, pos2};

use super::player_esp::{PlayerEspConfig, PlayerEspEntry};

pub fn draw_entries(
    painter: &Painter,
    entries: &[PlayerEspEntry],
    config: &PlayerEspConfig,
    screen: Rect,
) {
    for entry in entries {
        let rect = Rect::from_min_size(
            pos2(entry.x, entry.y),
            egui::vec2(entry.width, entry.height),
        );
        if rect.max.x < 0.0
            || rect.min.x > screen.max.x
            || rect.max.y < 0.0
            || rect.min.y > screen.max.y
        {
            continue;
        }

        if config.show_box {
            draw_box(painter, rect, entry);
        }

        if config.show_health {
            draw_health(painter, rect, entry);
        }

        if config.show_name {
            draw_name(painter, rect, entry);
        }
    }
}

fn rgba(color: [f32; 4], alpha_mul: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[3].clamp(0.0, 1.0) * alpha_mul * 255.0).round() as u8,
    )
}

fn draw_box(painter: &Painter, rect: Rect, entry: &PlayerEspEntry) {
    let color = rgba(entry.box_color, 1.0);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(2.4, Color32::from_rgba_unmultiplied(0, 0, 0, 180)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.2, color),
        egui::StrokeKind::Outside,
    );
}

fn draw_health(painter: &Painter, rect: Rect, entry: &PlayerEspEntry) {
    let bar_w = (rect.width() * 0.08).clamp(2.0, 4.0);
    let gap = 2.0;
    let bar = Rect::from_min_max(
        pos2(rect.left() - gap - bar_w, rect.top()),
        pos2(rect.left() - gap, rect.bottom()),
    );
    painter.rect_filled(bar, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 170));
    let fill_h = bar.height() * entry.health_ratio;
    let fill = Rect::from_min_max(
        pos2(bar.left() + 0.5, bar.bottom() - fill_h),
        pos2(bar.right() - 0.5, bar.bottom()),
    );
    painter.rect_filled(
        fill,
        0.0,
        Color32::from_rgb(
            entry.health_color[0],
            entry.health_color[1],
            entry.health_color[2],
        ),
    );
    let hp_text = format!("{:.0}", entry.health);
    let font = egui::FontId::proportional((rect.width() * 0.18).clamp(9.0, 13.0));
    painter.text(
        pos2(bar.center().x, bar.top() - 2.0),
        egui::Align2::CENTER_BOTTOM,
        &hp_text,
        font,
        Color32::from_rgb(
            entry.health_color[0],
            entry.health_color[1],
            entry.health_color[2],
        ),
    );
}

fn draw_name(painter: &Painter, rect: Rect, entry: &PlayerEspEntry) {
    let font_size = (rect.width() * 0.28).clamp(10.0, 14.0);
    let font = egui::FontId::proportional(font_size);
    let text_pos = pos2(rect.center().x, rect.top() - 3.0);
    painter.text(
        text_pos + egui::vec2(1.0, 1.0),
        egui::Align2::CENTER_BOTTOM,
        &entry.name,
        font.clone(),
        Color32::from_rgba_unmultiplied(0, 0, 0, 200),
    );
    painter.text(
        text_pos,
        egui::Align2::CENTER_BOTTOM,
        &entry.name,
        font,
        Color32::from_rgb(
            entry.name_color[0],
            entry.name_color[1],
            entry.name_color[2],
        ),
    );
}
