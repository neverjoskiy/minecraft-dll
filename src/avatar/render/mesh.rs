

use crate::avatar::format::{BbElement, BbElementKind, BbFace, BbModel, TextureRef};

#[derive(Clone, Copy, Debug)]
pub struct MeshVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub nx: f32,
    pub ny: f32,
    pub nz: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<MeshVertex>,
}


#[derive(Clone, Debug)]
pub struct TexturedMesh {
    pub texture_index: Option<u32>,
    pub mesh: Mesh,
}

impl Mesh {
    pub fn demo_cube() -> Self {
        
        cube_mesh(
            [-8.0, 0.0, -8.0],
            [8.0, 16.0, 8.0],
            [0.45, 0.75, 1.0, 1.0],
            None,
        )
    }

    pub fn from_bb_cube(from: [f32; 3], to: [f32; 3], rgba: [f32; 4]) -> Self {
        cube_mesh(from, to, rgba, None)
    }
}


pub fn meshes_from_model(model: &BbModel) -> Vec<TexturedMesh> {
    meshes_from_elements(&model.elements, &model.textures)
}


pub fn meshes_from_elements(elements: &[BbElement], textures: &[TextureRef]) -> Vec<TexturedMesh> {
    let mut buckets: Vec<(Option<u32>, Mesh)> = Vec::new();
    for element in elements {
        match element.kind {
            BbElementKind::Cube => push_cube_faces(&mut buckets, element, textures),
            BbElementKind::Mesh => push_mesh_faces(&mut buckets, element, textures),
        }
    }
    buckets
        .into_iter()
        .filter(|(_, mesh)| !mesh.vertices.is_empty())
        .map(|(texture_index, mesh)| TexturedMesh {
            texture_index,
            mesh,
        })
        .collect()
}

fn push_cube_faces(
    buckets: &mut Vec<(Option<u32>, Mesh)>,
    element: &BbElement,
    textures: &[TextureRef],
) {
    let sx = 1.0 / 16.0;
    
    
    let fx = element.from.x;
    let fy = element.from.y;
    let fz = element.from.z;
    let dx = element.to.x - element.from.x;
    let dy = element.to.y - element.from.y;
    let dz = element.to.z - element.from.z;

    
    let specs: [(&str, [f32; 3], [[f32; 3]; 4]); 6] = [
        (
            "north",
            [0.0, 0.0, -1.0],
            [
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
        ),
        (
            "south",
            [0.0, 0.0, 1.0],
            [
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
        ),
        (
            "east",
            [1.0, 0.0, 0.0],
            [
                [1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
            ],
        ),
        (
            "west",
            [-1.0, 0.0, 0.0],
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
            ],
        ),
        (
            "up",
            [0.0, 1.0, 0.0],
            [
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
        ),
        (
            "down",
            [0.0, -1.0, 0.0],
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        ),
    ];
    
    const UV_CORNERS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    for (name, normal, corners) in specs {
        let Some(face) = element.faces.get(name) else {
            continue;
        };
        let tex_idx = face.texture;
        let (uw, uh) = tex_idx
            .and_then(|i| textures.get(i as usize))
            .map(|t| (t.uv_width.max(1.0), t.uv_height.max(1.0)))
            .unwrap_or((16.0, 16.0));
        let rot_steps = ((face.rotation / 90).rem_euclid(4)) as usize;
        let n = transform_normal(normal, element);
        let bucket = bucket_mut(buckets, tex_idx);

        for (i, mult) in corners.iter().enumerate() {
            let pos = [
                (fx + dx * mult[0]) * sx,
                (fy + dy * mult[1]) * sx,
                (fz + dz * mult[2]) * sx,
            ];
            let uv_n = UV_CORNERS[(i + rot_steps) % 4];
            let u = (face.uv[0] + (face.uv[2] - face.uv[0]) * uv_n[0]) / uw;
            let v = (face.uv[1] + (face.uv[3] - face.uv[1]) * uv_n[1]) / uh;
            let p = transform_point(pos, element);
            bucket.vertices.push(MeshVertex {
                x: p[0],
                y: p[1],
                z: p[2],
                u,
                v,
                nx: n[0],
                ny: n[1],
                nz: n[2],
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            });
        }
    }
}





fn push_mesh_faces(
    buckets: &mut Vec<(Option<u32>, Mesh)>,
    element: &BbElement,
    textures: &[TextureRef],
) {
    let sx = 1.0 / 16.0;
    for face in element.faces.values() {
        let n = face.vertex_ids.len();
        if !(3..=4).contains(&n) {
            continue;
        }
        let tex_idx = face.texture;
        let (uw, uh) = tex_idx
            .and_then(|i| textures.get(i as usize))
            .map(|t| (t.uv_width.max(1.0), t.uv_height.max(1.0)))
            .unwrap_or((16.0, 16.0));

        let mut locals: Vec<[f32; 3]> = Vec::with_capacity(n);
        let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(n);
        for id in &face.vertex_ids {
            let Some(v) = element.vertices.get(id) else {
                locals.clear();
                break;
            };
            
            locals.push([v.x * sx, v.y * sx, v.z * sx]);
            let uv = face.vertex_uvs.get(id).copied().unwrap_or([0.0, 0.0]);
            uvs.push([uv[0] / uw, uv[1] / uh]);
        }
        if locals.len() != n {
            continue;
        }
        let normal = face_normal(&locals);
        let nrm = transform_mesh_normal(normal, element);
        let bucket = bucket_mut(buckets, tex_idx);
        let emit = if n == 3 { [0, 1, 2, 2] } else { [0, 1, 2, 3] };
        for i in emit {
            let p = transform_mesh_point(locals[i], element);
            bucket.vertices.push(MeshVertex {
                x: p[0],
                y: p[1],
                z: p[2],
                u: uvs[i][0],
                v: uvs[i][1],
                nx: nrm[0],
                ny: nrm[1],
                nz: nrm[2],
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            });
        }
    }
}

fn face_normal(corners: &[[f32; 3]]) -> [f32; 3] {
    if corners.len() < 3 {
        return [0.0, 1.0, 0.0];
    }
    let a = corners[0];
    let b = corners[1];
    let c = corners[2];
    let ux = b[0] - a[0];
    let uy = b[1] - a[1];
    let uz = b[2] - a[2];
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let vz = c[2] - a[2];
    let nx = uy * vz - uz * vy;
    let ny = uz * vx - ux * vz;
    let nz = ux * vy - uy * vx;
    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
    [nx / len, ny / len, nz / len]
}

fn bucket_mut<'a>(buckets: &'a mut Vec<(Option<u32>, Mesh)>, tex_idx: Option<u32>) -> &'a mut Mesh {
    if let Some(i) = buckets.iter().position(|(idx, _)| *idx == tex_idx) {
        return &mut buckets[i].1;
    }
    buckets.push((tex_idx, Mesh::default()));
    &mut buckets.last_mut().unwrap().1
}

fn transform_point(pos: [f32; 3], element: &BbElement) -> [f32; 3] {
    let ox = element.origin.x / 16.0;
    let oy = element.origin.y / 16.0;
    let oz = element.origin.z / 16.0;
    let mut x = pos[0] - ox;
    let mut y = pos[1] - oy;
    let mut z = pos[2] - oz;

    if let Some(rot) = &element.rotation {
        if let Some([rx, ry, rz]) = rot.euler {
            
            (x, y, z) = rotate_x(x, y, z, rx.to_radians());
            (x, y, z) = rotate_y(x, y, z, ry.to_radians());
            (x, y, z) = rotate_z(x, y, z, rz.to_radians());
        } else {
            let angle = rot.angle.to_radians();
            let pivot = [
                rot.origin.x / 16.0,
                rot.origin.y / 16.0,
                rot.origin.z / 16.0,
            ];
            x = pos[0] - pivot[0];
            y = pos[1] - pivot[1];
            z = pos[2] - pivot[2];
            match rot.axis.as_str() {
                "x" => (x, y, z) = rotate_x(x, y, z, angle),
                "z" => (x, y, z) = rotate_z(x, y, z, angle),
                _ => (x, y, z) = rotate_y(x, y, z, angle),
            }
            return [x + pivot[0], y + pivot[1], z + pivot[2]];
        }
    }

    [x + ox, y + oy, z + oz]
}


fn transform_mesh_point(local: [f32; 3], element: &BbElement) -> [f32; 3] {
    let mut x = local[0];
    let mut y = local[1];
    let mut z = local[2];
    if let Some(rot) = &element.rotation {
        if let Some([rx, ry, rz]) = rot.euler {
            
            (x, y, z) = rotate_z(x, y, z, rz.to_radians());
            (x, y, z) = rotate_y(x, y, z, ry.to_radians());
            (x, y, z) = rotate_x(x, y, z, rx.to_radians());
        } else {
            let angle = rot.angle.to_radians();
            match rot.axis.as_str() {
                "x" => (x, y, z) = rotate_x(x, y, z, angle),
                "z" => (x, y, z) = rotate_z(x, y, z, angle),
                _ => (x, y, z) = rotate_y(x, y, z, angle),
            }
        }
    }
    [
        x + element.origin.x / 16.0,
        y + element.origin.y / 16.0,
        z + element.origin.z / 16.0,
    ]
}

fn transform_normal(normal: [f32; 3], element: &BbElement) -> [f32; 3] {
    let mut x = normal[0];
    let mut y = normal[1];
    let mut z = normal[2];
    if let Some(rot) = &element.rotation {
        if let Some([rx, ry, rz]) = rot.euler {
            (x, y, z) = rotate_x(x, y, z, rx.to_radians());
            (x, y, z) = rotate_y(x, y, z, ry.to_radians());
            (x, y, z) = rotate_z(x, y, z, rz.to_radians());
        } else {
            let angle = rot.angle.to_radians();
            match rot.axis.as_str() {
                "x" => (x, y, z) = rotate_x(x, y, z, angle),
                "z" => (x, y, z) = rotate_z(x, y, z, angle),
                _ => (x, y, z) = rotate_y(x, y, z, angle),
            }
        }
    }
    normalize3(x, y, z)
}

fn transform_mesh_normal(normal: [f32; 3], element: &BbElement) -> [f32; 3] {
    let mut x = normal[0];
    let mut y = normal[1];
    let mut z = normal[2];
    if let Some(rot) = &element.rotation {
        if let Some([rx, ry, rz]) = rot.euler {
            (x, y, z) = rotate_z(x, y, z, rz.to_radians());
            (x, y, z) = rotate_y(x, y, z, ry.to_radians());
            (x, y, z) = rotate_x(x, y, z, rx.to_radians());
        } else {
            let angle = rot.angle.to_radians();
            match rot.axis.as_str() {
                "x" => (x, y, z) = rotate_x(x, y, z, angle),
                "z" => (x, y, z) = rotate_z(x, y, z, angle),
                _ => (x, y, z) = rotate_y(x, y, z, angle),
            }
        }
    }
    normalize3(x, y, z)
}

fn normalize3(x: f32, y: f32, z: f32) -> [f32; 3] {
    let len = (x * x + y * y + z * z).sqrt().max(1e-6);
    [x / len, y / len, z / len]
}

fn rotate_x(x: f32, y: f32, z: f32, a: f32) -> (f32, f32, f32) {
    let (s, c) = a.sin_cos();
    (x, y * c - z * s, y * s + z * c)
}

fn rotate_y(x: f32, y: f32, z: f32, a: f32) -> (f32, f32, f32) {
    let (s, c) = a.sin_cos();
    (x * c + z * s, y, -x * s + z * c)
}

fn rotate_z(x: f32, y: f32, z: f32, a: f32) -> (f32, f32, f32) {
    let (s, c) = a.sin_cos();
    (x * c - y * s, x * s + y * c, z)
}


fn face_uvs(face: &BbFace, uv_width: f32, uv_height: f32) -> [[f32; 2]; 4] {
    let u0 = face.uv[0] / uv_width;
    let v0 = face.uv[1] / uv_height;
    let u1 = face.uv[2] / uv_width;
    let v1 = face.uv[3] / uv_height;
    
    [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
}

fn cube_mesh(from: [f32; 3], to: [f32; 3], rgba: [f32; 4], uvs: Option<[[f32; 2]; 4]>) -> Mesh {
    let sx = 1.0 / 16.0;
    let (x0, y0, z0) = (from[0] * sx, from[1] * sx, from[2] * sx);
    let (x1, y1, z1) = (to[0] * sx, to[1] * sx, to[2] * sx);
    let color = [
        (rgba[0] * 255.0) as u8,
        (rgba[1] * 255.0) as u8,
        (rgba[2] * 255.0) as u8,
        (rgba[3] * 255.0) as u8,
    ];
    let default_uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let face_uv = uvs.unwrap_or(default_uvs);

    
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [0.0, 0.0, -1.0],
            [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]],
        ),
        (
            [0.0, 0.0, 1.0],
            [[x1, y0, z1], [x0, y0, z1], [x0, y1, z1], [x1, y1, z1]],
        ),
        (
            [0.0, -1.0, 0.0],
            [[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]],
        ),
        (
            [0.0, 1.0, 0.0],
            [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
        ),
        (
            [-1.0, 0.0, 0.0],
            [[x0, y0, z1], [x0, y0, z0], [x0, y1, z0], [x0, y1, z1]],
        ),
        (
            [1.0, 0.0, 0.0],
            [[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]],
        ),
    ];

    let mut vertices = Vec::with_capacity(24);
    for (normal, corners) in faces {
        for (pos, uv) in corners.iter().zip(face_uv.iter()) {
            vertices.push(MeshVertex {
                x: pos[0],
                y: pos[1],
                z: pos[2],
                u: uv[0],
                v: uv[1],
                nx: normal[0],
                ny: normal[1],
                nz: normal[2],
                r: color[0],
                g: color[1],
                b: color[2],
                a: color[3],
            });
        }
    }

    Mesh { vertices }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::format::{BbElementKind, BbRotation, BbVec3};
    use std::collections::HashMap;

    #[test]
    fn demo_cube_has_twenty_four_vertices() {
        let mesh = Mesh::demo_cube();
        assert_eq!(mesh.vertices.len(), 24);
        let min_y = mesh
            .vertices
            .iter()
            .map(|v| v.y)
            .fold(f32::INFINITY, f32::min);
        let max_y = mesh
            .vertices
            .iter()
            .map(|v| v.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((max_y - min_y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mesh_verts_stay_near_origin_after_rotation() {
        
        let mut vertices = HashMap::new();
        vertices.insert(
            "a".into(),
            BbVec3 {
                x: -1.0,
                y: 3.0,
                z: -1.0,
            },
        );
        vertices.insert(
            "b".into(),
            BbVec3 {
                x: 1.0,
                y: 3.0,
                z: -1.0,
            },
        );
        vertices.insert(
            "c".into(),
            BbVec3 {
                x: 1.0,
                y: 5.0,
                z: 1.0,
            },
        );
        vertices.insert(
            "d".into(),
            BbVec3 {
                x: -1.0,
                y: 5.0,
                z: 1.0,
            },
        );
        let mut faces = HashMap::new();
        let mut vertex_uvs = HashMap::new();
        for id in ["a", "b", "c", "d"] {
            vertex_uvs.insert(id.to_string(), [0.0, 0.0]);
        }
        faces.insert(
            "f".into(),
            BbFace {
                uv: [0.0, 0.0, 1.0, 1.0],
                texture: Some(0),
                rotation: 0,
                vertex_ids: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                vertex_uvs,
            },
        );
        let element = BbElement {
            name: "glasses".into(),
            kind: BbElementKind::Mesh,
            origin: BbVec3 {
                x: 0.0,
                y: 24.0,
                z: 0.0,
            },
            rotation: Some(BbRotation {
                euler: Some([-41.0, 0.0, 0.0]),
                ..Default::default()
            }),
            faces,
            vertices,
            ..Default::default()
        };
        let parts = meshes_from_elements(
            &[element],
            &[TextureRef {
                uv_width: 8.0,
                uv_height: 8.0,
                ..Default::default()
            }],
        );
        assert!(!parts.is_empty());
        for v in &parts[0].mesh.vertices {
            assert!(
                v.y > 1.2 && v.y < 2.2,
                "mesh y out of expected head band: {}",
                v.y
            );
            assert!(v.x.abs() < 0.5, "mesh x scattered: {}", v.x);
            assert!(v.z.abs() < 0.5, "mesh z scattered: {}", v.z);
        }
    }
}
