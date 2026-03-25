# A* Pathfinding

A* pathfinding algorithm on a 100x100 grid. Finds the shortest path from (0,0) to (99,99) while avoiding procedurally generated obstacles.

## How It Works

1. **Grid Generation** -- Creates a 100x100 grid with ~30% random obstacles. A diagonal corridor is cleared to guarantee a path exists.
2. **Min-Heap Priority Queue** -- Implements a binary min-heap sorted by f-cost for efficient open-set management.
3. **A* Search** -- Classic A* with Manhattan distance heuristic and 4-directional movement (cost = 1 per step).
4. **Path Reconstruction** -- Traces the parent chain from goal back to start.

## Data Structures

All stored as flat heap-allocated arrays:
- `grid[10000]` -- obstacle map (0=walkable, 1=blocked)
- `g_cost[10000]` -- cost from start to each node
- `f_cost[10000]` -- g_cost + heuristic estimate
- `parent[10000]` -- parent index for path reconstruction
- `closed[10000]` -- visited set
- `heap[20000]` -- min-heap of node indices

## Features Used

- `@module`, `@intent`, `@pure` annotations
- Heap allocation for all data structures
- Min-heap priority queue (sift-up, sift-down)
- Stack arrays for neighbor offsets
- LCG pseudo-random number generator

## Run

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/pathfinder/astar.axm
```
