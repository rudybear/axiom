# Game Engine Plan v2 — A Real Game Written in AXIOM + Lux

## Vision

Build a playable game demo in AXIOM that showcases:
- **AXIOM** for all CPU logic (game loop, physics, AI, input, ECS)
- **Lux** for all GPU shaders (PBR materials, lighting, particles, post-processing)
- **wgpu/Vulkan** for rendering backend (already working)
- **Self-optimization** via LLM optimizer during development

## The Game: "Asteroid Field"

A 3D asteroid field survival game:
- Player ship (glTF model) navigates through an asteroid field
- WASD controls ship movement, mouse aims
- Asteroids spawn at edges, drift toward center
- Collision = game over, score = survival time
- Particles on collision, engine trail behind ship
- HUD: score, health, FPS counter

This exercises: physics, collision detection, spawning/despawning (ECS), rendering, input, audio, particles — all the systems a real game needs.

## Architecture

```
game.axm (AXIOM)                         Lux Shaders
├── Game loop                             ├── pbr_material.lux (already have)
├── ECS world (lib/ecs.axm)              ├── particle.lux (need)
├── Systems:                              ├── skybox.lux (need)
│   ├── input_system (@pure)             └── post_process.lux (need)
│   ├── physics_system (@pure @parallel)
│   ├── collision_system (@pure)
│   ├── spawn_system
│   ├── particle_system (@pure)
│   └── score_system
├── gpu_* calls for rendering
└── Audio (play_beep on events)
```

## Phased Implementation

### Phase G1: Core Game Loop (Week 1)
**Files:** `game/asteroid_field.axm`, `game/README.md`

- Window creation (gpu_init)
- Game state (player position, velocity, score, alive)
- Input handling (WASD + escape)
- Camera following player
- Render single glTF model (the ship)
- Frame timing, delta time

### Phase G2: Asteroids + Collision (Week 1-2)
**Files:** `game/asteroids.axm` (included via @include)

- Asteroid spawning (random positions at screen edges)
- Asteroid movement (drift toward center with variation)
- Sphere-sphere collision detection (player vs asteroids)
- Despawn off-screen asteroids
- Score = frame count while alive
- Game over state

### Phase G3: Particles + Effects (Week 2)
**Files:** `game/particles.axm`

- Explosion particles on collision (arena-allocated, 100 per explosion)
- Engine trail behind ship (continuous particle emission)
- Particle physics: velocity, gravity, lifetime, fade
- All @pure for optimization

### Phase G4: HUD + Polish (Week 2-3)
- Score display (print to console for now; GPU text later)
- FPS counter
- Game restart on R key
- Sound effects: engine hum, explosion, game over
- Multiple lives

### Phase G5: Lux Shader Integration (Week 3-4)
**Files:** `game/shaders/` directory with .lux files

- PBR material for ship and asteroids
- Particle billboard shader
- Skybox (space background)
- Post-processing (bloom on explosions, vignette)

### Phase G6: Self-Optimization (Week 4)
- Add @strategy blocks to physics and collision systems
- Run `axiom optimize` to tune:
  - Collision detection spatial partitioning threshold
  - Particle batch sizes
  - Physics substep count
- Record optimization history in the game code

## Technical Requirements

### Already Have:
- [x] GPU rendering (gpu_init, gpu_load_gltf, gpu_render, gpu_set_camera)
- [x] Input system (is_key_down, get_mouse_x/y)
- [x] Audio (play_beep)
- [x] ECS library (lib/ecs.axm)
- [x] Arena allocator (for particles)
- [x] @pure + @parallel_for (for physics systems)
- [x] Structs (for game state)
- [x] PBR shader (Cook-Torrance BRDF)
- [x] glTF loading
- [x] Screenshot capture (gpu_screenshot)

### Need to Add:
- [ ] Delta time in the render loop (gpu_get_frame_time exists but needs integration)
- [ ] Multiple glTF model loading (load ship + asteroid models)
- [ ] Model transform (rotation, scale, translation per instance)
- [ ] Instanced rendering for asteroids (gpu_draw_instanced exists)
- [ ] Text rendering (or just print to console)
- [ ] Skybox rendering

### Need from Lux:
- [ ] Particle billboard shader (.lux → .spv)
- [ ] Skybox shader
- [ ] Bloom post-processing shader

## Game Data

### Models:
- Ship: any simple spaceship glTF (download from Sketchfab or use a primitive)
- Asteroid: any rock-like glTF (or just use DamagedHelmet as placeholder)

### For MVP (Phase G1-G2):
Use the DamagedHelmet as both ship and asteroid (different scales). Replace with proper models later.

## Success Criteria

The game must:
- [ ] Run at 60fps on the RTX PRO 6000
- [ ] Have zero per-frame heap allocations (arena only for particles)
- [ ] Use @pure on all physics/collision systems
- [ ] Be self-optimizable via `axiom optimize`
- [ ] Be writable in <500 lines of AXIOM (excluding lib/ecs.axm)
- [ ] Be fun to play for at least 30 seconds
