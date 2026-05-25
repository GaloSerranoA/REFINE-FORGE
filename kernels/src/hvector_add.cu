#include <cuda_runtime.h>

#include <cstdio>
#include <cstdlib>

__global__ void hvector_add_kernel(const int *a, const int *b, int *out, int n) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        out[idx] = a[idx] + b[idx];
    }
}

static void check_cuda(cudaError_t status, const char *label) {
    if (status != cudaSuccess) {
        std::fprintf(stderr, "%s: %s\n", label, cudaGetErrorString(status));
        std::exit(1);
    }
}

int main() {
    constexpr int n = 4;
    const int host_a[n] = {1, 2, 3, 4};
    const int host_b[n] = {5, 6, 7, 8};
    int host_out[n] = {0, 0, 0, 0};

    int *dev_a = nullptr;
    int *dev_b = nullptr;
    int *dev_out = nullptr;
    const std::size_t bytes = sizeof(host_a);

    check_cuda(cudaMalloc(&dev_a, bytes), "cudaMalloc(dev_a)");
    check_cuda(cudaMalloc(&dev_b, bytes), "cudaMalloc(dev_b)");
    check_cuda(cudaMalloc(&dev_out, bytes), "cudaMalloc(dev_out)");
    check_cuda(cudaMemcpy(dev_a, host_a, bytes, cudaMemcpyHostToDevice), "cudaMemcpy(host_a)");
    check_cuda(cudaMemcpy(dev_b, host_b, bytes, cudaMemcpyHostToDevice), "cudaMemcpy(host_b)");

    hvector_add_kernel<<<1, 32>>>(dev_a, dev_b, dev_out, n);
    check_cuda(cudaGetLastError(), "hvector_add_kernel");
    check_cuda(cudaDeviceSynchronize(), "cudaDeviceSynchronize");
    check_cuda(cudaMemcpy(host_out, dev_out, bytes, cudaMemcpyDeviceToHost), "cudaMemcpy(out)");

    check_cuda(cudaFree(dev_a), "cudaFree(dev_a)");
    check_cuda(cudaFree(dev_b), "cudaFree(dev_b)");
    check_cuda(cudaFree(dev_out), "cudaFree(dev_out)");

    if (host_out[0] != 6 || host_out[1] != 8 || host_out[2] != 10 || host_out[3] != 12) {
        std::fprintf(stderr, "unexpected output: %d %d %d %d\n", host_out[0], host_out[1],
                     host_out[2], host_out[3]);
        return 2;
    }

    std::printf("hvector_add_cuda_v1: %d %d %d %d\n", host_out[0], host_out[1], host_out[2],
                host_out[3]);
    return 0;
}
