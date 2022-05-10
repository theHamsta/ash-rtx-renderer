use ash::vk;
use cgmath::{Deg, Matrix4, Point3, Vector3, Vector4};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PushConstants {
    light_position: Vector4<f32>,
    view: Matrix4<f32>,
    model: Matrix4<f32>,
    proj: Matrix4<f32>,
}

impl PushConstants {
    pub fn new(extent: vk::Extent2D, translation: Point3<f32>, light_position: Vector4<f32>, zoom: f32, rotation: f32) -> Self {
        Self {
            light_position,
            model: Matrix4::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(rotation)),
            view: Matrix4::look_at_rh(
                zoom * Point3::new(0.0, 1.0, 5.0),
                translation,
                Vector3::new(0.0, 1.0, 0.0),
            ),
            proj: Matrix4::perspective(
                Deg(60.0),
                extent.width as f32 / extent.height as f32,
                0.01,
                100.0,
            ),
        }
    }
}

// Add perspective method
trait Matrix4Ext {
    fn perspective<A: Into<cgmath::Rad<f32>>>(
        fovy: A,
        aspecf32: f32,
        near: f32,
        far: f32,
    ) -> Matrix4<f32>;
}
impl Matrix4Ext for Matrix4<f32> {
    fn perspective<A: Into<cgmath::Rad<f32>>>(
        fovy: A,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Matrix4<f32> {
        use cgmath::{Angle, Rad};
        let f: Rad<f32> = fovy.into();
        let f = f / 2.0;
        let f = Rad::cot(f);
        Matrix4::<f32>::new(
            f / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            -f,
            0.0,
            0.0,
            0.0,
            0.0,
            far / (near - far),
            -1.0,
            0.0,
            0.0,
            (near * far) / (near - far),
            0.0,
        )
    }
}
