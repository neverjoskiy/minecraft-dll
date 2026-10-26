use crate::avatar::animation::{Animation, Interpolation, PlayState, TransformType};
use crate::avatar::model::ModelPart;


pub fn tick_animation(anim: &mut Animation, root: &mut ModelPart, mut limit: i32) -> i32 {
    if anim.play_state == PlayState::Stopped {
        return limit;
    }
    if anim.play_state != PlayState::Paused {
        
    }

    for part_entry in &anim.parts {
        if limit <= 0 {
            break;
        }
        let Some(part) = root.find_mut(&part_entry.part_name) else {
            continue;
        };
        if part.last_animation_priority > anim.priority {
            continue;
        }
        let merge = part.last_animation_priority == anim.priority;
        part.last_animation_priority = anim.priority;
        part.animated = true;

        for channel in &part_entry.channels {
            if limit <= 0 {
                return limit;
            }
            let frames = &channel.keyframes;
            if frames.is_empty() {
                continue;
            }
            let current_index = binary_search_keyframe(frames, anim.frame_time);
            let next_index = (current_index + 1).min(frames.len() - 1);
            let current = &frames[current_index];
            let next = &frames[next_index];
            let delta = if current.time == next.time {
                0.0
            } else {
                ((anim.frame_time - current.time) / (next.time - current.time)).clamp(0.0, 1.0)
            };
            let interp = if next.interpolation == Interpolation::Bezier {
                Interpolation::Bezier
            } else {
                current.interpolation
            };
            let transform = interp.generate(
                frames,
                current_index,
                next_index,
                anim.blend,
                delta,
                channel.transform,
            );
            channel.transform.apply(part, transform, merge);

            match channel.transform {
                TransformType::Rotation | TransformType::GlobalRot => {
                    if anim.override_rot() {
                        part.animation_override |= 1;
                    } else if !merge {
                        part.animation_override &= 6;
                    }
                }
                TransformType::Position => {
                    if anim.override_pos() {
                        part.animation_override |= 2;
                    } else if !merge {
                        part.animation_override &= 5;
                    }
                }
                TransformType::Scale => {
                    if anim.override_scale() {
                        part.animation_override |= 4;
                    } else if !merge {
                        part.animation_override &= 3;
                    }
                }
            }
            limit -= 1;
        }
    }
    limit
}

fn binary_search_keyframe(frames: &[crate::avatar::animation::Keyframe], time: f32) -> usize {
    let mut lo = 0usize;
    let mut hi = frames.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if time <= frames[mid].time {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo.saturating_sub(1)
}

pub fn clear_animation_priority(root: &mut ModelPart) {
    root.walk_mut(&mut |part| {
        part.last_animation_priority = i32::MIN;
        part.animated = false;
        part.animation_override = 0;
    });
}
