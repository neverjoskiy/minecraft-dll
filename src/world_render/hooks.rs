

use jni::{JNIEnv, objects::JObject};

use super::geometry;
use crate::{
    avatar::render::{
        mesh::Mesh,
        world_bridge::{self, WorldRenderType},
    },
    jni_bridge,
    native_jvmti::{self, HookSpec, LocalLoad, RENDER_SYMBOLS},
    utils::geometry::Vec3d,
};













pub fn world_hook_spec() -> HookSpec {
    HookSpec::entry(
        [RENDER_SYMBOLS.level_renderer],
        RENDER_SYMBOLS.submit_entities.name,
        RENDER_SYMBOLS.submit_entities.descriptor,
        "onSubmitWorld",
        "(Ljava/lang/Object;Ljava/lang/Object;)V",
        vec![LocalLoad::Reference(1), LocalLoad::Reference(3)],
    )
}

pub fn register_world_hooks() -> Result<(), String> {
    native_jvmti::register_hook(world_hook_spec())
}


pub struct WorldBatch {
    pub mesh: Mesh,
    pub render_type: WorldRenderType,
}

impl WorldBatch {
    pub fn lines() -> Self {
        Self {
            mesh: Mesh::default(),
            render_type: WorldRenderType::Lines,
        }
    }

    pub fn translucent() -> Self {
        Self {
            mesh: Mesh::default(),
            render_type: WorldRenderType::Translucent,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mesh.vertices.is_empty()
    }
}



pub fn on_submit_world(env: &mut JNIEnv<'_>, pose_stack: &JObject<'_>, collector: &JObject<'_>) {
    static HITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    crate::diag::log_every(&HITS, 600, "world: submitEntities hook fired");
    if !super::enabled() {
        crate::diag::log_once(
            "world_hook_disabled",
            "world: hook fired but enabled()==false (armed/unavailable mismatch)",
        );
        return;
    }
    if pose_stack.is_null() || collector.is_null() {
        crate::diag::log_once("world_null_args", "world: hook got null pose/collector");
        return;
    }
    
    let _ = env.with_local_frame(128, |env| {
        submit_all(env, pose_stack, collector);
        Ok::<(), jni::errors::Error>(())
    });
}

fn submit_all(env: &mut JNIEnv<'_>, pose_stack: &JObject<'_>, collector: &JObject<'_>) {
    let Some(camera) = world_bridge::camera_position(env) else {
        crate::diag::log_once("world_no_camera", "world: camera_position returned None");
        return;
    };
    
    super::set_local_player(crate::avatar::render::local_player_uuid(env));
    let batches = collect_batches(env, camera);
    static VERTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let total: usize = batches.iter().map(|batch| batch.mesh.vertices.len()).sum();
    crate::diag::log_every(
        &VERTS,
        600,
        format!(
            "world: batches={} verts={total} camera=({:.1},{:.1},{:.1})",
            batches.len(),
            camera.x,
            camera.y,
            camera.z
        ),
    );
    if total == 0 {
        crate::diag::log_once(
            "world_no_geometry",
            "world: hook alive but every module produced 0 vertices (modules off or all skipped)",
        );
    }
    for batch in batches {
        if batch.is_empty() {
            continue;
        }
        let kind = batch.render_type;
        let verts = batch.mesh.vertices.len();
        if let Err(error) =
            world_bridge::submit_world_mesh(env, pose_stack, collector, batch.mesh, batch.render_type)
        {
            crate::diag::log(format!(
                "world: submit failed kind={kind:?} verts={verts}: {error}"
            ));
            super::mark_unavailable(error);
            return;
        }
        static SUBMITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        crate::diag::log_every(
            &SUBMITS,
            600,
            format!("world: submitted kind={kind:?} verts={verts}"),
        );
    }
}


fn collect_batches(env: &mut JNIEnv<'_>, camera: Vec3d) -> Vec<WorldBatch> {
    let mut lines = WorldBatch::lines();
    let mut filled = WorldBatch::translucent();

    
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    let mut mark = |name: &'static str, lines: &WorldBatch, filled: &WorldBatch,
                    previous: &mut usize| {
        let now = lines.mesh.vertices.len() + filled.mesh.vertices.len();
        counts.push((name, now - *previous));
        *previous = now;
    };
    let mut seen = 0usize;

    crate::utils::block_esp::build_world_geometry(&mut lines.mesh, &mut filled.mesh, camera);
    mark("block_esp", &lines, &filled, &mut seen);
    crate::utils::jump_circle::build_world_geometry(&mut lines.mesh, &mut filled.mesh, camera);
    mark("jump_circle", &lines, &filled, &mut seen);
    crate::utils::trajectories::build_world_geometry(&mut lines.mesh, camera);
    mark("trajectories", &lines, &filled, &mut seen);
    
    
    if crate::utils::world_markers::build_world_geometry(
        env,
        &mut lines.mesh,
        &mut filled.mesh,
        camera,
    )
    .is_err()
    {
        jni_bridge::clear_pending_exception(env);
    }
    mark("markers", &lines, &filled, &mut seen);
    let _ = crate::utils::player_esp::build_world_geometry(env, &mut lines.mesh, camera);
    mark("player_esp", &lines, &filled, &mut seen);
    
    let _ = crate::utils::name_tags::world::build_world_geometry(env, &mut lines.mesh, &mut filled.mesh, camera);
    mark("name_tags_3d", &lines, &filled, &mut seen);

    static BREAKDOWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    crate::diag::log_every(
        &BREAKDOWN,
        600,
        format!(
            "world: per-module {} | lines={} filled={}",
            counts
                .iter()
                .map(|(name, count)| format!("{name}={count}"))
                .collect::<Vec<_>>()
                .join(" "),
            lines.mesh.vertices.len(),
            filled.mesh.vertices.len()
        ),
    );

    vec![lines, filled]
}


pub use geometry::{
    Box3, circle_points, push_box_outline, push_cone, push_ground_quad, push_polyline, push_ring,
    push_segment,
};
