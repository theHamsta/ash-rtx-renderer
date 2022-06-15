
extern "C" {

__global__ void simple(int width, int height, float time,
                       cudaSurfaceObject_t tex) {
  int x = blockDim.x * blockIdx.x + threadIdx.x;
  int y = blockDim.y * blockIdx.y + threadIdx.y;
  if (x >= width || y >= height) {
    return;
  }
  unsigned char val = x % 255;
  surf2Dwrite(uchar4{val, 0, 0, 255}, tex, x, y);
}

}
