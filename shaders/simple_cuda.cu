
extern "C" {

__global__ void simple(int width, int height, float time,
                       cudaSurfaceObject_t tex) {
  int x = blockDim.x * blockIdx.x + threadIdx.x;
  int y = blockDim.y * blockIdx.y + threadIdx.y;
  if (x >= width || y >= height) {
    return;
  }
  surf2Dwrite(uchar4{255, 100, 100, 255}, tex, x, y);
}
}
