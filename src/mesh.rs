use ply_rs::ply;
use std::{borrow::Borrow, path::Path};

#[derive(Debug)]
struct Vertex {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug)]
struct Triangle {
    indices: [i32; 3],
}

#[derive(thiserror::Error, Debug)]
pub enum MeshError {
    #[error("Unsupported mesh file type: {0}")]
    UnsupportedMeshFileType(String),
    #[error("Mesh file has no file extension")]
    NoFileExtension,
}

impl ply::PropertyAccess for Vertex {
    fn new() -> Self {
        Vertex {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
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

#[derive(Debug)]
pub struct Mesh {
    vertices: Vec<Vertex>,
    triangles: Vec<Triangle>,
}

impl Mesh {
    pub fn num_triangles(&self) -> usize {
        self.triangles.len()
    }

    pub fn num_vertices(&self) -> usize {
        self.vertices.len()
    }

    fn from_ply(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let f = std::fs::File::open(path)?;
        let mut f = std::io::BufReader::new(f);

        let vertex_parser = ply_rs::parser::Parser::<Vertex>::new();
        let face_parser = ply_rs::parser::Parser::<Triangle>::new();

        let header = vertex_parser.read_header(&mut f).unwrap();

        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for (_ignore_key, element) in &header.elements {
            match element.name.as_ref() {
                "vertex" => {
                    vertices = vertex_parser.read_payload_for_element(&mut f, &element, &header)?;
                }
                "face" => {
                    triangles = face_parser.read_payload_for_element(&mut f, &element, &header)?;
                }
                _ => panic!("Enexpeced element!"),
            }
        }
        Ok(Mesh {
            vertices,
            triangles,
        })
    }

    pub fn from_file(path: &impl AsRef<Path>) -> anyhow::Result<Self> {
        let ext = path
            .as_ref()
            .extension()
            .ok_or(MeshError::NoFileExtension)?
            .to_string_lossy();
        match ext.borrow() {
            "ply" | "PLY" => Mesh::from_ply(path),
            ext => Err(MeshError::UnsupportedMeshFileType(ext.to_owned()).into()),
        }
    }
}
