# Rotating 3D Model Viewer

An interactive 3D viewer that loads a glTF model and renders it with PBR (Physically Based Rendering) shading. The model can be viewed from different angles using WASD camera controls.

## Controls

| Key | Action |
|-----|--------|
| W | Move camera forward |
| S | Move camera backward |
| A | Move camera left |
| D | Move camera right |
| Q | Move camera up |
| E | Move camera down |

## Features Used

- `@module`, `@intent` annotations
- GPU builtins: `gpu_init`, `gpu_load_gltf`, `gpu_set_camera`, `gpu_render`
- Input builtins: `is_key_down` for keyboard polling
- Real-time render loop with `gpu_should_close`

## Requirements

- A compatible GPU with the AXIOM renderer runtime
- A `data/DamagedHelmet.glb` model file (or any glTF model)

## Run

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/vulkan/rotating_model.axm
```
