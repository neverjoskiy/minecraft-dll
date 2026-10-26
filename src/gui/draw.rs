use egui::{Color32, Context, Pos2, Rect, Stroke, pos2, vec2};
use crate::{
    utils::{
        aim_target::AimAssistOverlay,
        geometry::Vec3d,
        name_tags::{self, NameTagEntry, NameTagItemIconRequest, NameTagsConfig},
        world_markers::WorldEntityMarker,
    },
};
use super::ThemePalette;
pub(crate) fn draw_action_wheel(
    ctx: &Context,
    snap: &crate::avatar::action_wheel::OverlayRenderSnapshot,
    palette: ThemePalette,
) {
    use std::f32::consts::TAU;

    egui::Area::new(egui::Id::new("xz_action_wheel"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let desired = vec2(320.0, 320.0);
            let (rect, _response) = ui.allocate_exact_size(desired, egui::Sense::hover());
            let center = rect.center();
            let radius = 118.0_f32;
            let painter = ui.painter();

            painter.circle_filled(center, radius + 28.0, Color32::from_black_alpha(150));
            painter.circle_stroke(
                center,
                radius + 28.0,
                Stroke::new(1.5, palette.accent.gamma_multiply(0.85)),
            );
            painter.text(
                center + vec2(0.0, -radius - 8.0),
                egui::Align2::CENTER_BOTTOM,
                if snap.page_title.is_empty() {
                    "XZ Actions"
                } else {
                    snap.page_title.as_str()
                },
                egui::FontId::proportional(14.0),
                palette.text,
            );

            let slot_count = snap.slot_titles.len().max(1);
            for (index, title) in snap.slot_titles.iter().enumerate() {
                let angle =
                    -std::f32::consts::FRAC_PI_2 + (index as f32) * (TAU / slot_count as f32);
                let pos = center + vec2(angle.cos() * radius, angle.sin() * radius);
                let selected = index == snap.selected_index;
                let fill = if selected {
                    palette.accent
                } else if title.is_empty() {
                    palette.faint
                } else {
                    palette.panel
                };
                let slot_rect = Rect::from_center_size(pos, vec2(86.0, 28.0));
                let response = ui.interact(
                    slot_rect,
                    egui::Id::new(("xz_wheel_slot", index)),
                    egui::Sense::click(),
                );
                painter.rect_filled(slot_rect, 6.0, fill);
                painter.rect_stroke(
                    slot_rect,
                    6.0,
                    Stroke::new(
                        1.0,
                        if selected {
                            palette.accent_hover
                        } else {
                            palette.stroke
                        },
                    ),
                    egui::StrokeKind::Middle,
                );
                let label = if title.is_empty() {
                    "вЂ”"
                } else {
                    title.as_str()
                };
                painter.text(
                    slot_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(12.0),
                    if selected {
                        Color32::BLACK
                    } else {
                        palette.text
                    },
                );
                if response.hovered() {
                    crate::avatar::action_wheel::select_slot(index);
                }
                if response.clicked() {
                    crate::avatar::action_wheel::select_slot(index);
                    let _ = crate::avatar::action_wheel::click();
                }
            }

            let scroll = ui.input(|input| input.raw_scroll_delta.y);
            if scroll > 0.5 {
                crate::avatar::action_wheel::scroll(-1);
            } else if scroll < -0.5 {
                crate::avatar::action_wheel::scroll(1);
            }
        });
}

pub(crate) fn draw_dim_background(ctx: &Context, palette: ThemePalette, opacity: f32) {
    let rect = ctx.screen_rect();
    let alpha = (palette.dim_alpha as f32 * 0.28 * opacity.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 48.0) as u8;
    if alpha == 0 {
        return;
    }
    ctx.layer_painter(egui::LayerId::background()).rect_filled(
        rect,
        0.0,
        Color32::from_black_alpha(alpha),
    );
}

pub(crate) fn draw_aim_fov(ctx: &Context, overlay: AimAssistOverlay, palette: ThemePalette) {
    let rect = ctx.screen_rect();
    let radius = aim_fov_radius(
        rect.width(),
        rect.height(),
        overlay.aim_fov_degrees,
        overlay.vanilla_fov_degrees,
    );
    if radius < 1.0 {
        return;
    }

    let color = if overlay.target_active {
        palette.accent_hover
    } else {
        palette.muted
    };
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("xz_aim_assist_fov"),
    ))
    .circle_stroke(
        rect.center(),
        radius,
        egui::Stroke::new(1.5, alpha_color(color, 0.78)),
    );
}

pub(crate) fn draw_neuro_progress(
    ctx: &Context,
    progress: crate::utils::neuro::NeuroProgressOverlay,
    draggable: bool,
) {
    use crate::utils::neuro::NeuroProgressPhase;

    let text = if progress.phase == NeuroProgressPhase::Training {
        if progress.detail.is_empty() {
            "Training model...".to_string()
        } else {
            format!("Training model... ({})", progress.detail)
        }
    } else {
        let max_samples = progress.max_samples.max(1);
        let percent = ((progress.samples as f64 / max_samples as f64) * 100.0)
            .clamp(0.0, 100.0)
            .round() as u32;
        format!(
            "Recording {}/{} ({}%)",
            progress.samples, progress.max_samples, percent
        )
    };
    let text = if !progress.detail.is_empty() && progress.phase != NeuroProgressPhase::Training {
        format!("{text} | {}", progress.detail)
    } else {
        text
    };

    egui::Area::new(egui::Id::new("xz_neuro_progress"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 56.0))
        .interactable(draggable)
        .movable(draggable)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(15.0)
                    .color(egui::Color32::from_rgb(235, 240, 245))
                    .strong(),
            );
        });
}

pub(crate) fn aim_fov_radius(width: f32, height: f32, aim_fov: f32, vanilla_fov: f32) -> f32 {
    let max_radius = (width.min(height) * 0.5 - 2.0).max(0.0);
    if max_radius <= 0.0 {
        return 0.0;
    }

    let vanilla_fov = vanilla_fov.clamp(30.0, 170.0);
    if aim_fov >= vanilla_fov {
        return max_radius;
    }

    let aim_half = (aim_fov * 0.5).clamp(0.1, 89.9).to_radians();
    let vanilla_half = (vanilla_fov * 0.5).clamp(1.0, 89.9).to_radians();
    let radius = aim_half.tan() / vanilla_half.tan() * (width.min(height) * 0.5);
    radius.clamp(0.0, max_radius)
}

pub(crate) fn draw_target_ghosts_world(
    painter: &egui::Painter,
    viewport: [f32; 2],
    camera: &name_tags::CameraView,
    target: WorldEntityMarker,
    phase: f32,
    color: [f32; 4],
) {
    let height = (target.max_y - target.min_y).max(0.8);
    let orbit = target.radius * 1.12;
    for i in 0..5 {
        let t = i as f64 / 5.0;
        let angle = phase as f64 * 2.6 + t * std::f64::consts::TAU;
        let y = target.min_y
            + height
                * (0.18
                    + 0.72 * ((phase as f64 * 0.7 + t * std::f64::consts::TAU).sin() * 0.5 + 0.5));
        let point = Vec3d {
            x: target.center.x + angle.cos() * orbit,
            y,
            z: target.center.z + angle.sin() * orbit,
        };
        if let Some(center) = project_world(point, camera, viewport) {
            let size =
                (42.0 / point.subtract(camera.position).length().max(1.0) as f32).clamp(4.0, 12.0);
            painter.circle_filled(center, size * 1.8, color32_from_rgba(color, 0.16));
            painter.circle_filled(center, size, color32_from_rgba(color, 0.62));
            painter.circle_stroke(
                center,
                size * 1.25,
                Stroke::new(1.0, Color32::from_white_alpha(110)),
            );
        }
    }
}

pub(crate) fn draw_world_segment(
    painter: &egui::Painter,
    viewport: [f32; 2],
    camera: &name_tags::CameraView,
    from: Vec3d,
    to: Vec3d,
    stroke: Stroke,
) {
    if let (Some(a), Some(b)) = (
        project_world(from, camera, viewport),
        project_world(to, camera, viewport),
    ) {
        painter.line_segment([a, b], stroke);
    }
}

pub(crate) fn project_world(point: Vec3d, camera: &name_tags::CameraView, viewport: [f32; 2]) -> Option<Pos2> {
    name_tags::manual_project_to_screen(point, camera, viewport)
        .map(|screen| pos2(screen.x, screen.y))
}

pub(crate) fn color32_from_rgba(color: [f32; 4], alpha_mul: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[3].clamp(0.0, 1.0) * alpha_mul * 255.0).round() as u8,
    )
}
pub(crate) fn draw_name_tags(
    ctx: &Context,
    tags: &[NameTagEntry],
    config: &NameTagsConfig,
    palette: ThemePalette,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("xz_name_tags"),
    ));
    let opacity = config.alpha.clamp(0.0, 1.0);

    for tag in tags {
        let scale = tag.scale.clamp(0.45, 3.0);
        let layout = tag_label_layout(tag, scale);
        let prefix = if tag.prefix.is_empty() {
            String::new()
        } else {
            format!("{} ", tag.prefix)
        };
        let name_color = rgb_color(tag.name_color, opacity);
        let health_color = rgb_color(tag.health_color, opacity);
        let prefix_color = alpha_color(palette.accent_hover, opacity);
        let segments = if prefix.is_empty() {
            vec![
                (tag.name.as_str(), name_color),
                (tag.health_text.as_str(), health_color),
            ]
        } else {
            vec![
                (prefix.as_str(), prefix_color),
                (tag.name.as_str(), name_color),
                (tag.health_text.as_str(), health_color),
            ]
        };
        let rect = layout.rect;

        draw_tag_items(&painter, tag, rect.top(), scale, opacity, palette);

        painter.rect_filled(
            rect,
            4.0 * scale,
            Color32::from_rgba_unmultiplied(
                palette.panel.r(),
                palette.panel.g(),
                palette.panel.b(),
                scaled_alpha(185, opacity),
            ),
        );
        painter.rect_stroke(
            rect,
            4.0 * scale,
            egui::Stroke::new(1.0, alpha_color(palette.stroke, opacity)),
            egui::StrokeKind::Inside,
        );

        let mut x = rect.left() + layout.pad_x;
        for (index, (text, color)) in segments.iter().enumerate() {
            painter.text(
                pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                *text,
                egui::FontId::proportional(layout.font_size),
                *color,
            );
            x += estimate_text_width(text, layout.font_size);
            if index + 1 < segments.len() {
                x += layout.gap;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct TagLabelLayout {
    rect: Rect,
    font_size: f32,
    pad_x: f32,
    gap: f32,
}

#[derive(Clone, Copy)]
struct TagItemLayout {
    slot: name_tags::NameTagItemSlot,
    frame: Rect,
    icon: Rect,
}

pub(crate) fn tag_label_layout(tag: &NameTagEntry, scale: f32) -> TagLabelLayout {
    let font_size = (12.5 * scale).clamp(8.0, 24.0);
    let pad_x = 7.0 * scale;
    let gap = 5.0 * scale;
    let height = font_size + 9.0 * scale;
    let mut text_width = estimate_text_width(&tag.name, font_size)
        + estimate_text_width(&tag.health_text, font_size);
    let segment_count: usize = if tag.prefix.is_empty() { 2 } else { 3 };
    if !tag.prefix.is_empty() {
        text_width += estimate_text_width(&format!("{} ", tag.prefix), font_size);
    }
    text_width += gap * segment_count.saturating_sub(1) as f32;
    let width = (text_width + pad_x * 2.0).max(42.0 * scale);

    TagLabelLayout {
        rect: Rect::from_center_size(pos2(tag.screen_x, tag.screen_y), vec2(width, height)),
        font_size,
        pad_x,
        gap,
    }
}

pub(crate) fn tag_item_layouts(tag: &NameTagEntry, tag_top: f32, scale: f32) -> Vec<TagItemLayout> {
    if tag.items.is_empty() {
        return Vec::new();
    }

    let frame_size = (20.0 * scale).clamp(13.0, 32.0);
    let gap = (3.0 * scale).clamp(2.0, 6.0);
    let padding = (2.0 * scale).clamp(1.0, 4.0);
    let count = tag.items.len().min(6);
    let total_width = frame_size * count as f32 + gap * count.saturating_sub(1) as f32;
    let mut x = tag.screen_x - total_width * 0.5;
    let y = tag_top - frame_size - 4.0 * scale;
    let mut layouts = Vec::with_capacity(count);

    for item in tag.items.iter().take(6) {
        let frame = Rect::from_min_size(pos2(x, y), vec2(frame_size, frame_size));
        layouts.push(TagItemLayout {
            slot: item.slot,
            frame,
            icon: frame.shrink(padding),
        });
        x += frame_size + gap;
    }

    layouts
}

pub(crate) fn collect_name_tag_item_icon_requests(tags: &[NameTagEntry]) -> Vec<NameTagItemIconRequest> {
    let mut requests = Vec::new();
    for tag in tags {
        let scale = tag.scale.clamp(0.45, 3.0);
        let label = tag_label_layout(tag, scale);
        for item in tag_item_layouts(tag, label.rect.top(), scale) {
            requests.push(NameTagItemIconRequest {
                entity_id: tag.entity_id,
                slot: item.slot,
                inventory_slot: None,
                x: item.icon.left(),
                y: item.icon.top(),
                size: item.icon.width().min(item.icon.height()),
            });
        }
    }
    requests
}

pub(crate) fn draw_tag_items(
    painter: &egui::Painter,
    tag: &NameTagEntry,
    tag_top: f32,
    scale: f32,
    opacity: f32,
    palette: ThemePalette,
) {
    if tag.items.is_empty() {
        return;
    }

    for item in tag_item_layouts(tag, tag_top, scale) {
        painter.rect_filled(
            item.frame,
            3.0 * scale,
            Color32::from_rgba_unmultiplied(
                palette.extreme.r(),
                palette.extreme.g(),
                palette.extreme.b(),
                scaled_alpha(170, opacity),
            ),
        );
        painter.rect_stroke(
            item.frame,
            3.0 * scale,
            egui::Stroke::new(1.0, alpha_color(palette.stroke, opacity)),
            egui::StrokeKind::Inside,
        );
        painter.rect_filled(
            item.icon,
            2.0 * scale,
            Color32::from_rgba_unmultiplied(0, 0, 0, scaled_alpha(42, opacity)),
        );
    }
}

pub(crate) fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars()
        .map(|ch| {
            if ch == ' ' {
                0.32
            } else if ch.is_ascii_uppercase() || ch.is_ascii_digit() {
                0.62
            } else if ch.is_ascii() {
                0.54
            } else {
                0.74
            }
        })
        .sum::<f32>()
        * font_size
}

pub(crate) fn rgb_color(rgb: [u8; 3], opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], scaled_alpha(255, opacity))
}

pub(crate) fn alpha_color(color: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        scaled_alpha(color.a(), opacity),
    )
}

pub(crate) fn scaled_alpha(alpha: u8, opacity: f32) -> u8 {
    ((alpha as f32) * opacity.clamp(0.0, 1.0)).round() as u8
}

