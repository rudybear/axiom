// axiom-renderer/src/camera.rs
//
// Perspective camera with look-at. Uses glam for matrix math.
// Designed for wgpu's clip space conventions (Y-up, depth [0,1], right-handed).

use glam::{Mat4, Vec3};

/// Camera state: position, target, and projection parameters.
pub struct CameraState {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_deg: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 3.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_deg: 45.0,
            near: 0.1,
            far: 100.0,
        }
    }
}

impl CameraState {
    /// Compute the view matrix (look-at, right-handed).
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, self.up)
    }

    /// Compute the projection matrix for the given aspect ratio.
    /// Uses wgpu's clip space conventions (Y-up, depth [0,1]).
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y_deg.to_radians(), aspect, self.near, self.far)
    }
}

/// Camera uniform buffer data, uploaded to the GPU each frame.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    pub eye_pos: [f32; 3],
    pub _padding: f32,
}

impl CameraUniform {
    /// Build the uniform data from camera state and viewport aspect ratio.
    pub fn from_state(state: &CameraState, aspect: f32) -> Self {
        let view = state.view_matrix();
        let proj = state.projection_matrix(aspect);
        let view_proj = proj * view;
        Self {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            eye_pos: state.eye.to_array(),
            _padding: 0.0,
        }
    }
}
