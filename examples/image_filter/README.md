# Sobel Edge Detection Filter

Applies the Sobel edge detection operator to a procedurally generated 512x512 grayscale image.

## How It Works

1. **Image Generation** -- Creates a concentric ring pattern blended with a diagonal gradient
2. **Sobel Convolution** -- Applies two 3x3 kernels (Gx, Gy) to compute horizontal and vertical gradients
3. **Edge Magnitude** -- Computes `sqrt(Gx^2 + Gy^2)` clamped to [0, 255]
4. **Statistics** -- Reports checksum, strong edge pixel count, and maximum magnitude

## Features Used

- `@module`, `@intent`, `@pure` annotations
- `@parallel_for` for parallelizing the convolution pass
- `sqrt`, `truncate` (f64 to i32), `to_f64` builtins
- Heap allocation for 512x512 pixel buffers
- Pointer read/write for image data

## Run

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/image_filter/sobel_filter.axm
```
