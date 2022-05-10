use log::info;
use ply_rs::ply;
use std::mem::transmute;
use std::{os::unix::prelude::OsStrExt, path::Path};

#[derive(Debug, Default, Clone, Copy)]
pub struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Normal {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Vertex {
    pos: Position,
    normal: Option<Normal>,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct Triangle {
    indices: [i32; 3],
}

#[derive(thiserror::Error, Debug)]
pub enum MeshIOError {
    #[error("Unsupported mesh file type: {0:?}")]
    UnsupportedMeshFileType(String),
    #[error("Mesh file has no file extension")]
    NoFileExtension,
    #[error("Vertex attribute number does not agree with number of vertices: {0} attributes vs {1} vertices")]
    InvalidNumberOfVertexAttributes(usize, usize),
}

fn get_normals(mesh: &tri_mesh::mesh::Mesh) -> anyhow::Result<Vec<Normal>> {
    let mut normals = Vec::with_capacity(mesh.no_vertices());
    for v in mesh.vertex_iter() {
        let vec = mesh.vertex_normal(v);
        normals.push(Normal {
            x: vec.x as f32,
            y: vec.y as f32,
            z: vec.z as f32,
        });
    }
    Ok(normals)
}

fn get_positions(mesh: &tri_mesh::mesh::Mesh) -> Vec<Position> {
    mesh.vertex_iter()
        .map(|v| {
            let pos = mesh.vertex_position(v);
            Position {
                x: pos.x as f32,
                y: pos.y as f32,
                z: pos.z as f32,
            }
        })
        .collect()
}

fn get_indices(mesh: &tri_mesh::mesh::Mesh) -> Vec<Triangle> {
    mesh.face_iter()
        .map(|f| {
            let (a, b, c) = mesh.face_vertices(f);
            unsafe {
                Triangle {
                    indices: [transmute(a), transmute(b), transmute(c)],
                }
            }
        })
        .collect()
}

impl ply::PropertyAccess for Position {
    fn new() -> Self {
        Self::default()
    }
    fn set_property(&mut self, key: String, property: ply::Property) {
        match (key.as_ref(), property) {
            ("x", ply::Property::Float(v)) => self.x = v,
            ("y", ply::Property::Float(v)) => self.y = v,
            ("z", ply::Property::Float(v)) => self.z = v,
            _ => (),
        }
    }
}

impl ply::PropertyAccess for Vertex {
    fn new() -> Self {
        Vertex::default()
    }
    fn set_property(&mut self, key: String, property: ply::Property) {
        match (key.as_ref(), property) {
            ("x", ply::Property::Float(v)) => self.pos.x = v,
            ("y", ply::Property::Float(v)) => self.pos.y = v,
            ("z", ply::Property::Float(v)) => self.pos.z = v,
            ("nx", ply::Property::Float(v)) => self.normal.get_or_insert(Default::default()).x = v,
            ("ny", ply::Property::Float(v)) => self.normal.get_or_insert(Default::default()).y = v,
            ("nz", ply::Property::Float(v)) => self.normal.get_or_insert(Default::default()).z = v,
            _ => (),
        }
    }
}

impl ply::PropertyAccess for Triangle {
    fn new() -> Self {
        Triangle {
            indices: Default::default(),
        }
    }
    fn set_property(&mut self, key: String, property: ply::Property) {
        match (key.as_ref(), property) {
            ("vertex_indices", ply::Property::ListInt(vec)) => {
                let result = vec.try_into();
                match result {
                    Ok(triangle) => self.indices = triangle,
                    Err(err) => log::error!("Found face that's not a triangle: {err:?}"),
                }
            }
            (k, _) => panic!("Face: Unexpected key/value combination: key: {}", k),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ReadOptions {
    OnlyTriangles,
    WithAttributes,
}

#[derive(Debug)]
pub struct Mesh {
    positions: Vec<Position>,
    triangles: Vec<Triangle>,
    vertex_normals: Option<Vec<Normal>>,
}

impl Mesh {
    pub fn num_triangles(&self) -> usize {
        self.triangles.len()
    }

    pub fn num_vertices(&self) -> usize {
        self.positions.len()
    }

    pub fn has_vertex_normals(&self) -> bool {
        self.vertex_normals.is_some()
    }

    fn from_ply(path: impl AsRef<Path>, options: ReadOptions) -> anyhow::Result<Self> {
        info!("Reading {:?}", path.as_ref().to_str());
        let f = std::fs::File::open(&path)?;
        let mut f = std::io::BufReader::new(f);

        let face_parser = ply_rs::parser::Parser::<Triangle>::new();
        match options {
            ReadOptions::OnlyTriangles => {
                let vertex_parser = ply_rs::parser::Parser::<Position>::new();

                let header = vertex_parser.read_header(&mut f).unwrap();

                let mut positions = Vec::new();
                let mut triangles = Vec::new();
                for (_ignore_key, element) in &header.elements {
                    match element.name.as_ref() {
                        "vertex" => {
                            positions =
                                vertex_parser.read_payload_for_element(&mut f, element, &header)?;
                        }
                        "face" => {
                            triangles =
                                face_parser.read_payload_for_element(&mut f, element, &header)?;
                        }
                        _ => (),
                    }
                }
                Ok(Mesh {
                    positions,
                    triangles,
                    vertex_normals: None,
                })
            }
            ReadOptions::WithAttributes => {
                let vertex_parser = ply_rs::parser::Parser::<Vertex>::new();
                let f = std::fs::File::open(&path)?;
                let mut f = std::io::BufReader::new(f);
                let header = vertex_parser.read_header(&mut f).unwrap();
                let mut vertices = Vec::new();
                let mut triangles = Vec::new();

                for (_ignore_key, element) in &header.elements {
                    match element.name.as_ref() {
                        "vertex" => {
                            vertices =
                                vertex_parser.read_payload_for_element(&mut f, element, &header)?;
                        }
                        "face" => {
                            triangles =
                                face_parser.read_payload_for_element(&mut f, element, &header)?;
                        }
                        _ => (),
                    }
                }
                let positions: Vec<_> = vertices.iter().map(|v| v.pos).collect();
                let vertex_normals: Vec<_> = vertices.iter().flat_map(|v| v.normal).collect();

                let vertex_normals = match (vertex_normals.len(), positions.len()) {
                    (0, _) => Ok({
                        let mesh = tri_mesh::mesh_builder::MeshBuilder::new()
                            .with_positions(
                                positions
                                    .iter()
                                    .flat_map(|p| [p.x as f64, p.y as f64, p.z as f64])
                                    .collect(),
                            )
                            .with_indices(
                                triangles
                                    .iter()
                                    .flat_map(|t| {
                                        [
                                            t.indices[0] as u32,
                                            t.indices[1] as u32,
                                            t.indices[2] as u32,
                                        ]
                                    })
                                    .collect(),
                            )
                            .build()
                            .map_err(|err| anyhow::anyhow!("Failed to calc normals: {err:?}"))?;

                        Some(get_normals(&mesh)?)
                    }),
                    (a, b) if a == b => Ok(Some(vertex_normals)),
                    (a, b) => {
                        anyhow::Result::Err(MeshIOError::InvalidNumberOfVertexAttributes(a, b))
                    }
                }?;

                Ok(Mesh {
                    positions,
                    triangles,
                    vertex_normals,
                })
            }
        }
    }

    fn from_obj(path: impl AsRef<Path>, options: ReadOptions) -> anyhow::Result<Self> {
        info!("Reading {:?}", path.as_ref().to_str());
        let obj_source = std::fs::read_to_string(path.as_ref())?;
        let mesh = tri_mesh::mesh_builder::MeshBuilder::new()
            .with_obj(obj_source)
            .build()
            .map_err(|err| anyhow::anyhow!("Failed to read obj: {err:?}"))?;

        match options {
            ReadOptions::OnlyTriangles => Ok(Mesh {
                positions: get_positions(&mesh),
                triangles: get_indices(&mesh),
                vertex_normals: None,
            }),
            ReadOptions::WithAttributes => Ok(Mesh {
                positions: get_positions(&mesh),
                triangles: get_indices(&mesh),
                vertex_normals: Some(get_normals(&mesh)?),
            }),
        }
    }

    pub fn from_file(path: &impl AsRef<Path>, options: ReadOptions) -> anyhow::Result<Self> {
        let ext = path
            .as_ref()
            .extension()
            .ok_or(MeshIOError::NoFileExtension)?;
        match ext.as_bytes() {
            b"ply" | b"PLY" => Mesh::from_ply(path, options),
            b"obj" | b"OBJ" => Mesh::from_obj(path, options),
            ext => Err(MeshIOError::UnsupportedMeshFileType(
                String::from_utf8_lossy(ext).to_string(),
            )
            .into()),
        }
    }

    /// Get a reference to the mesh's positions.
    #[must_use]
    pub fn positions(&self) -> &[Position] {
        self.positions.as_ref()
    }

    /// Get a reference to the mesh's vertex normals.
    #[must_use]
    pub fn vertex_normals(&self) -> Option<&Vec<Normal>> {
        self.vertex_normals.as_ref()
    }

    /// Get a reference to the mesh's triangles.
    #[must_use]
    pub fn triangles(&self) -> &[Triangle] {
        self.triangles.as_ref()
    }
}
