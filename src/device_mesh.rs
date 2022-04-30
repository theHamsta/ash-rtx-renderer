//
// device_mesh.rs
// Copyright (C) 2022 Stephan Seitz <stephan.seitz@fau.de>
// Distributed under terms of the GPLv3 license.
//

use std::{collections::HashMap, rc::Rc};

use ash::vk;

use crate::mesh::Mesh;

struct DeviceMesh {
    mesh: Rc<Mesh>,
    buffers: HashMap<String, vk::Buffer>,
}
