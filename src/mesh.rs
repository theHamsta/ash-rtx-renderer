use ply_rs::ply;
use std::{os::unix::prelude::OsStrExt, path::Path};

#[derive(Debug, Default, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Default, Clone, Copy)]
struct Normal {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Default, Clone, Copy)]
struct Vertex {
    pos: Position,
    normal: Option<Normal>,
}

#[derive(Debug, Default)]
struct Triangle {
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
                            positions = vertex_parser
                                .read_payload_for_element(&mut f, &element, &header)?;
                        }
                        "face" => {
                            triangles =
                                face_parser.read_payload_for_element(&mut f, &element, &header)?;
                        }
                        _ => (),
                    }
                }
                return Ok(Mesh {
                    positions,
                    triangles,
                    vertex_normals: None,
                });
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
                            vertices = vertex_parser
                                .read_payload_for_element(&mut f, &element, &header)?;
                        }
                        "face" => {
                            triangles =
                                face_parser.read_payload_for_element(&mut f, &element, &header)?;
                        }
                        _ => (),
                    }
                }
                let positions: Vec<_> = vertices.iter().map(|v| v.pos).collect();
                let vertex_normals: Vec<_> = vertices.iter().flat_map(|v| v.normal).collect();

                let vertex_normals = match (vertex_normals.len(), positions.len()) {
                    (0, _) => Ok(None),
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

    pub fn from_file(path: &impl AsRef<Path>, options: ReadOptions) -> anyhow::Result<Self> {
        let ext = path
            .as_ref()
            .extension()
            .ok_or(MeshIOError::NoFileExtension)?;
        match ext.as_bytes() {
            b"ply" | b"PLY" => Mesh::from_ply(path, options),
            ext => Err(MeshIOError::UnsupportedMeshFileType(
                String::from_utf8_lossy(&ext).to_string(),
            )
            .into()),
        }
    }
}
