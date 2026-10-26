mod contexts;
mod hooks;
pub mod mesh;
pub(crate) mod textures;

pub use contexts::{
    LayerPolicy, RenderContext, allow_camera_offset, allow_nameplate_override, allow_particles,
    allow_sounds, layer_policy,
};
pub use hooks::{
    ensure_render_hooks_armed, install_render_hooks, on_avatar_body, on_avatar_scale,
    on_dispatcher_submit, on_first_person_hand, on_geometry, on_is_body_visible, on_living_submit,
    local_player_uuid, on_should_render_layers, register_avatar_hooks_if_active, replacement_enabled,
    set_replacement_enabled, store_geometry_renderer, store_mesh_emit_class,
};



pub mod world_bridge {
    pub use super::hooks::{WorldRenderType, camera_position, submit_world_mesh};
}

pub(crate) use hooks::clear_render_caches;
#[cfg(test)]
pub use hooks::readd_world_hooks_for_test;
pub use mesh::{Mesh, MeshVertex, TexturedMesh};

use uuid::Uuid;

use crate::avatar::format::{BbElement, BbModel, BbVec3};
use crate::avatar::runtime::{LoadedAvatar, RenderAcceptanceSnapshot};

#[derive(Clone, Debug, Default)]
pub struct AvatarRenderSnapshot {
    pub player: Option<Uuid>,
    pub avatar_id: String,
    pub display_name: String,
    pub elements: Vec<RenderElement>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct RenderElement {
    pub name: String,
    pub from: BbVec3,
    pub to: BbVec3,
}

pub fn snapshot_for_player(player: Uuid) -> AvatarRenderSnapshot {
    let acceptance = crate::avatar::runtime::render_snapshot(player);
    let fallback_id = acceptance.avatar_id.clone();
    let fallback_status = acceptance.status.clone();
    crate::avatar::runtime::active_avatar(player)
        .map(|avatar| snapshot_from_avatar(Some(player), avatar, acceptance))
        .unwrap_or_else(|| AvatarRenderSnapshot {
            avatar_id: fallback_id,
            status: fallback_status,
            ..Default::default()
        })
}

pub fn snapshot_from_avatar(
    player: Option<Uuid>,
    avatar: LoadedAvatar,
    acceptance: RenderAcceptanceSnapshot,
) -> AvatarRenderSnapshot {
    AvatarRenderSnapshot {
        player,
        avatar_id: avatar.id,
        display_name: avatar.display_name,
        elements: avatar
            .package
            .models
            .iter()
            .flat_map(collect_elements)
            .collect(),
        status: acceptance.status,
    }
}

fn collect_elements(model: &BbModel) -> impl Iterator<Item = RenderElement> + '_ {
    model.elements.iter().map(element_to_render)
}

fn element_to_render(element: &BbElement) -> RenderElement {
    RenderElement {
        name: element.name.clone(),
        from: element.from,
        to: element.to,
    }
}
