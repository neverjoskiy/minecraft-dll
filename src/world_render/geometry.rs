










use crate::{avatar::render::mesh::{Mesh, MeshVertex}, utils::geometry::Vec3d};



pub const LINE_WIDTH: f32 = 1.5;


#[derive(Clone, Copy, Debug)]
pub struct Box3 {
    pub min: Vec3d,
    pub max: Vec3d,
}

fn color_bytes(color: [f32; 4]) -> [u8; 4] {
    [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}


fn line_vertex(point: Vec3d, camera: Vec3d, rgba: [u8; 4]) -> MeshVertex {
    MeshVertex {
        x: (point.x - camera.x) as f32,
        y: (point.y - camera.y) as f32,
        z: (point.z - camera.z) as f32,
        u: 0.0,
        v: 0.0,
        nx: LINE_WIDTH,
        ny: 0.0,
        nz: 0.0,
        r: rgba[0],
        g: rgba[1],
        b: rgba[2],
        a: rgba[3],
    }
}

fn tri_vertex(point: Vec3d, camera: Vec3d, rgba: [u8; 4], uv: [f32; 2]) -> MeshVertex {
    MeshVertex {
        x: (point.x - camera.x) as f32,
        y: (point.y - camera.y) as f32,
        z: (point.z - camera.z) as f32,
        u: uv[0],
        v: uv[1],
        nx: 0.0,
        ny: 1.0,
        nz: 0.0,
        r: rgba[0],
        g: rgba[1],
        b: rgba[2],
        a: rgba[3],
    }
}




pub fn push_billboard_quad(
    mesh: &mut Mesh,
    camera: Vec3d,
    center: Vec3d,
    width: f64,
    height: f64,
    color: [f32; 4],
) {
    let rgba = color_bytes(color);
    
    let forward = Vec3d {
        x: center.x - camera.x,
        y: center.y - camera.y,
        z: center.z - camera.z,
    };
    let depth = (forward.x * forward.x + forward.y * forward.y + forward.z * forward.z).sqrt();
    if depth < 0.05 {
        return;
    }
    let forward = Vec3d {
        x: forward.x / depth,
        y: forward.y / depth,
        z: forward.z / depth,
    };
    let world_up = Vec3d {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    let mut right = Vec3d {
        x: forward.y * world_up.z - forward.z * world_up.y,
        y: forward.z * world_up.x - forward.x * world_up.z,
        z: forward.x * world_up.y - forward.y * world_up.x,
    };
    let right_len = (right.x * right.x + right.y * right.y + right.z * right.z).sqrt();
    if right_len < 1e-6 {
        right = Vec3d {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        };
    } else {
        right.x /= right_len;
        right.y /= right_len;
        right.z /= right_len;
    }
    let up = Vec3d {
        x: right.y * forward.z - right.z * forward.y,
        y: right.z * forward.x - right.x * forward.z,
        z: right.x * forward.y - right.y * forward.x,
    };
    let hw = width * 0.5;
    let hh = height * 0.5;
    let p0 = Vec3d {
        x: center.x - right.x * hw - up.x * hh,
        y: center.y - right.y * hw - up.y * hh,
        z: center.z - right.z * hw - up.z * hh,
    };
    let p1 = Vec3d {
        x: center.x + right.x * hw - up.x * hh,
        y: center.y + right.y * hw - up.y * hh,
        z: center.z + right.z * hw - up.z * hh,
    };
    let p2 = Vec3d {
        x: center.x + right.x * hw + up.x * hh,
        y: center.y + right.y * hw + up.y * hh,
        z: center.z + right.z * hw + up.z * hh,
    };
    let p3 = Vec3d {
        x: center.x - right.x * hw + up.x * hh,
        y: center.y - right.y * hw + up.y * hh,
        z: center.z - right.z * hw + up.z * hh,
    };
    for (point, uv) in [(p0, [0.0, 0.0]), (p1, [1.0, 0.0]), (p2, [1.0, 1.0]), (p3, [0.0, 1.0])] {
        mesh.vertices.push(tri_vertex(point, camera, rgba, uv));
    }
}


pub fn push_segment(mesh: &mut Mesh, camera: Vec3d, from: Vec3d, to: Vec3d, color: [f32; 4]) {
    push_segment_width(mesh, camera, from, to, color, LINE_WIDTH);
}


pub fn push_segment_width(
    mesh: &mut Mesh,
    camera: Vec3d,
    from: Vec3d,
    to: Vec3d,
    color: [f32; 4],
    width: f32,
) {
    let rgba = color_bytes(color);
    let mut a = line_vertex(from, camera, rgba);
    let mut b = line_vertex(to, camera, rgba);
    a.nx = width.max(1.01);
    b.nx = width.max(1.01);
    mesh.vertices.push(a);
    mesh.vertices.push(b);
}




pub fn push_quad(
    mesh: &mut Mesh,
    camera: Vec3d,
    corners: [Vec3d; 4],
    color: [f32; 4],
) {
    let rgba = color_bytes(color);
    for (index, point) in corners.iter().enumerate() {
        let uv = match index {
            0 => [0.0, 0.0],
            1 => [1.0, 0.0],
            2 => [1.0, 1.0],
            _ => [0.0, 1.0],
        };
        mesh.vertices.push(tri_vertex(*point, camera, rgba, uv));
    }
}


pub fn push_triangle(
    mesh: &mut Mesh,
    camera: Vec3d,
    a: Vec3d,
    b: Vec3d,
    c: Vec3d,
    color: [f32; 4],
) {
    let rgba = color_bytes(color);
    mesh.vertices.push(tri_vertex(a, camera, rgba, [0.5, 0.0]));
    mesh.vertices.push(tri_vertex(b, camera, rgba, [0.0, 1.0]));
    mesh.vertices.push(tri_vertex(c, camera, rgba, [1.0, 1.0]));
}


pub fn push_polyline(mesh: &mut Mesh, camera: Vec3d, points: &[Vec3d], color: [f32; 4]) {
    for pair in points.windows(2) {
        push_segment(mesh, camera, pair[0], pair[1], color);
    }
}


pub fn push_ring(mesh: &mut Mesh, camera: Vec3d, points: &[Vec3d], color: [f32; 4]) {
    if points.len() < 2 {
        return;
    }
    push_polyline(mesh, camera, points, color);
    push_segment(
        mesh,
        camera,
        points[points.len() - 1],
        points[0],
        color,
    );
}




pub fn push_ring_width(
    mesh: &mut Mesh,
    camera: Vec3d,
    points: &[Vec3d],
    color: [f32; 4],
    width: f32,
) {
    if points.len() < 2 {
        return;
    }
    let rgba = color_bytes(color);
    let segment = |mesh: &mut Mesh, from: Vec3d, to: Vec3d| {
        let mut a = line_vertex(from, camera, rgba);
        let mut b = line_vertex(to, camera, rgba);
        a.nx = width;
        b.nx = width;
        mesh.vertices.push(a);
        mesh.vertices.push(b);
    };
    for pair in points.windows(2) {
        segment(mesh, pair[0], pair[1]);
    }
    segment(mesh, points[points.len() - 1], points[0]);
}


pub fn circle_points(center: Vec3d, radius: f64, segments: usize) -> Vec<Vec3d> {
    let segments = segments.max(3);
    (0..segments)
        .map(|index| {
            let angle = index as f64 / segments as f64 * std::f64::consts::TAU;
            Vec3d {
                x: center.x + angle.cos() * radius,
                y: center.y,
                z: center.z + angle.sin() * radius,
            }
        })
        .collect()
}


pub fn push_box_outline(mesh: &mut Mesh, camera: Vec3d, boxed: Box3, color: [f32; 4]) {
    let Box3 { min, max } = boxed;
    let corner = |x: f64, y: f64, z: f64| Vec3d { x, y, z };
    let edges = [
        
        ((min.x, min.y, min.z), (max.x, min.y, min.z)),
        ((max.x, min.y, min.z), (max.x, min.y, max.z)),
        ((max.x, min.y, max.z), (min.x, min.y, max.z)),
        ((min.x, min.y, max.z), (min.x, min.y, min.z)),
        
        ((min.x, max.y, min.z), (max.x, max.y, min.z)),
        ((max.x, max.y, min.z), (max.x, max.y, max.z)),
        ((max.x, max.y, max.z), (min.x, max.y, max.z)),
        ((min.x, max.y, max.z), (min.x, max.y, min.z)),
        
        ((min.x, min.y, min.z), (min.x, max.y, min.z)),
        ((max.x, min.y, min.z), (max.x, max.y, min.z)),
        ((max.x, min.y, max.z), (max.x, max.y, max.z)),
        ((min.x, min.y, max.z), (min.x, max.y, max.z)),
    ];
    for ((ax, ay, az), (bx, by, bz)) in edges {
        push_segment(
            mesh,
            camera,
            corner(ax, ay, az),
            corner(bx, by, bz),
            color,
        );
    }
}




pub fn push_ground_quad(
    mesh: &mut Mesh,
    camera: Vec3d,
    center: Vec3d,
    radius: f64,
    rotation: f64,
    color: [f32; 4],
) {
    let rgba = color_bytes(color);
    let (sin, cos) = rotation.sin_cos();
    let corner = |dx: f64, dz: f64| Vec3d {
        x: center.x + dx * cos - dz * sin,
        y: center.y,
        z: center.z + dx * sin + dz * cos,
    };
    let a = corner(-radius, -radius);
    let b = corner(radius, -radius);
    let c = corner(radius, radius);
    let d = corner(-radius, radius);
    for (point, uv) in [
        (a, [0.0, 0.0]),
        (b, [1.0, 0.0]),
        (c, [1.0, 1.0]),
        (d, [0.0, 1.0]),
    ] {
        mesh.vertices.push(tri_vertex(point, camera, rgba, uv));
    }
}










pub const APEX_ALPHA_RATIO: f32 = 0.53;

pub fn push_cone(
    mesh: &mut Mesh,
    camera: Vec3d,
    apex: Vec3d,
    rim_center: Vec3d,
    radius: f64,
    segments: usize,
    color: [f32; 4],
) {
    let apex_alpha = (color[3] * APEX_ALPHA_RATIO).clamp(0.0, 1.0);
    let apex_rgba = color_bytes([color[0], color[1], color[2], apex_alpha]);
    let rim_rgba = color_bytes(color);
    let rim = circle_points(rim_center, radius, segments);
    for index in 0..rim.len() {
        let current = rim[index];
        let next = rim[(index + 1) % rim.len()];
        mesh.vertices.push(tri_vertex(apex, camera, apex_rgba, [0.5, 0.0]));
        mesh.vertices.push(tri_vertex(apex, camera, apex_rgba, [0.5, 0.0]));
        mesh.vertices
            .push(tri_vertex(current, camera, rim_rgba, [0.0, 1.0]));
        mesh.vertices.push(tri_vertex(next, camera, rim_rgba, [1.0, 1.0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Vec3d {
        Vec3d {
            x: 100.0,
            y: 64.0,
            z: -50.0,
        }
    }

    #[test]
    fn vertices_are_camera_relative() {
        let mut mesh = Mesh::default();
        push_segment(
            &mut mesh,
            camera(),
            Vec3d {
                x: 101.0,
                y: 65.0,
                z: -49.0,
            },
            Vec3d {
                x: 101.0,
                y: 66.0,
                z: -49.0,
            },
            [1.0, 1.0, 1.0, 1.0],
        );
        let first = mesh.vertices[0];
        assert_eq!((first.x, first.y, first.z), (1.0, 1.0, 1.0));
    }

    #[test]
    fn line_meshes_are_routed_to_the_line_emitter() {
        
        
        let mut mesh = Mesh::default();
        push_segment(
            &mut mesh,
            camera(),
            Vec3d::default(),
            Vec3d {
                x: 1.0,
                ..Default::default()
            },
            [1.0, 0.0, 0.0, 1.0],
        );
        let first = mesh.vertices[0];
        assert!(first.nx > 1.0, "line width must exceed 1.0");
        assert_eq!(first.ny, 0.0, "ny must stay 0 for the line path");
    }

    #[test]
    fn triangle_meshes_are_not_mistaken_for_lines() {
        let mut mesh = Mesh::default();
        push_ground_quad(
            &mut mesh,
            camera(),
            camera(),
            1.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
        );
        let first = mesh.vertices[0];
        assert!(
            !(first.nx > 1.0 && first.ny == 0.0),
            "filled geometry must not match the line-mesh signature"
        );
    }

    #[test]
    fn box_outline_emits_twelve_segments() {
        let mut mesh = Mesh::default();
        push_box_outline(
            &mut mesh,
            camera(),
            Box3 {
                min: Vec3d::default(),
                max: Vec3d {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
            },
            [1.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(mesh.vertices.len(), 24, "12 edges × 2 vertices");
    }

    
    
    #[test]
    fn cone_faces_are_quads_for_the_filled_batch_topology() {
        let mut mesh = Mesh::default();
        let segments = 32;
        push_cone(
            &mut mesh,
            camera(),
            Vec3d {
                y: 2.0,
                ..Default::default()
            },
            Vec3d::default(),
            1.0,
            segments,
            [1.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(
            mesh.vertices.len(),
            segments * 4,
            "one degenerate quad (apex duplicated) per rim segment"
        );
    }

    #[test]
    fn ring_closes_back_to_its_first_point() {
        let mut mesh = Mesh::default();
        let points = circle_points(Vec3d::default(), 2.0, 8);
        push_ring(&mut mesh, camera(), &points, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(mesh.vertices.len(), 16, "8 segments × 2 vertices");
        let last = mesh.vertices[mesh.vertices.len() - 1];
        let first = mesh.vertices[0];
        assert!((last.x - first.x).abs() < 1e-5 && (last.z - first.z).abs() < 1e-5);
    }
}
