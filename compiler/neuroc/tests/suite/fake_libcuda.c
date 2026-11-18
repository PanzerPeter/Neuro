// A stand-in for libcuda.so.1 with several devices and no GPU, for the multi-GPU tests.
//
// Device memory is host memory, so transfers really copy and a round trip keeps its
// values. Kernels do not run: a launch only records the device it happened on. What the
// fake does enforce is the driver's rule that a stream, a module, a function and a device
// buffer belong to the context that made them: using one while another device's context
// is current, freeing a buffer from the wrong context, or copying into a buffer on a
// device other than the stream's, prints `fake-cuda: violation` and aborts. A launch and
// a module load print `fake-cuda: launch on device N` / `module on device N` to stderr.
//
// FAKE_CUDA_DEVICES sets the device count, 2 when unset. Only the entry points the
// runtime resolves are defined.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_BUFFERS 4096
#define INVALID_VALUE 1
#define INVALID_DEVICE 101

typedef unsigned long long CUdeviceptr;

struct handle {
    int device;
};

static struct handle contexts[64];
static int current = -1;

static struct {
    char *base;
    size_t size;
    int device;
} buffers[MAX_BUFFERS];

static void violation(const char *what, int owner) {
    fprintf(stderr, "fake-cuda: violation: %s of device %d while device %d is current\n",
            what, owner, current);
    abort();
}

static void owned(const char *what, int owner) {
    if (owner != current) {
        violation(what, owner);
    }
}

static struct handle *make(void) {
    struct handle *handle = malloc(sizeof *handle);
    handle->device = current;
    return handle;
}

// The device whose buffer contains `address`, or -1 for host memory.
static int device_of(CUdeviceptr address) {
    char *at = (char *)(size_t)address;
    for (int i = 0; i < MAX_BUFFERS; i++) {
        if (buffers[i].base != NULL && at >= buffers[i].base &&
            at < buffers[i].base + buffers[i].size) {
            return buffers[i].device;
        }
    }
    return -1;
}

static int device_count(void) {
    const char *count = getenv("FAKE_CUDA_DEVICES");
    return count != NULL ? atoi(count) : 2;
}

int cuInit(unsigned flags) {
    (void)flags;
    return 0;
}

int cuGetErrorName(int error, const char **name) {
    *name = error == INVALID_DEVICE ? "CUDA_ERROR_INVALID_DEVICE" : "CUDA_ERROR_FAKE";
    return 0;
}

int cuDeviceGetCount(int *count) {
    *count = device_count();
    return 0;
}

int cuDeviceGet(int *device, int ordinal) {
    if (ordinal < 0 || ordinal >= device_count()) {
        return INVALID_DEVICE;
    }
    *device = ordinal;
    return 0;
}

int cuDevicePrimaryCtxRetain(void **context, int device) {
    contexts[device].device = device;
    *context = &contexts[device];
    return 0;
}

int cuCtxSetCurrent(void *context) {
    current = context == NULL ? -1 : ((struct handle *)context)->device;
    return 0;
}

int cuModuleLoadData(void **module, const void *image) {
    (void)image;
    if (current < 0) {
        return INVALID_VALUE;
    }
    fprintf(stderr, "fake-cuda: module on device %d\n", current);
    *module = make();
    return 0;
}

int cuModuleUnload(void *module) {
    free(module);
    return 0;
}

int cuModuleGetFunction(void **function, void *module, const char *name) {
    (void)name;
    owned("module", ((struct handle *)module)->device);
    *function = make();
    return 0;
}

int cuLaunchKernel(void *function, unsigned gx, unsigned gy, unsigned gz, unsigned bx,
                   unsigned by, unsigned bz, unsigned shared, void *stream, void **params,
                   void **extra) {
    (void)gx, (void)gy, (void)gz, (void)bx, (void)by, (void)bz, (void)shared;
    (void)params, (void)extra;
    owned("function", ((struct handle *)function)->device);
    owned("stream", ((struct handle *)stream)->device);
    fprintf(stderr, "fake-cuda: launch on device %d\n", current);
    return 0;
}

int cuStreamCreate(void **stream, unsigned flags) {
    (void)flags;
    *stream = make();
    return 0;
}

int cuStreamSynchronize(void *stream) {
    owned("stream", ((struct handle *)stream)->device);
    return 0;
}

int cuMemAlloc_v2(CUdeviceptr *address, size_t size) {
    for (int i = 0; i < MAX_BUFFERS; i++) {
        if (buffers[i].base == NULL) {
            buffers[i].base = malloc(size);
            buffers[i].size = size;
            buffers[i].device = current;
            *address = (CUdeviceptr)(size_t)buffers[i].base;
            return 0;
        }
    }
    return INVALID_VALUE;
}

int cuMemAllocManaged(CUdeviceptr *address, size_t size, unsigned flags) {
    (void)flags;
    return cuMemAlloc_v2(address, size);
}

int cuMemFree_v2(CUdeviceptr address) {
    char *at = (char *)(size_t)address;
    for (int i = 0; i < MAX_BUFFERS; i++) {
        if (buffers[i].base == at) {
            owned("free of a buffer", buffers[i].device);
            free(at);
            buffers[i].base = NULL;
            return 0;
        }
    }
    return INVALID_VALUE;
}

int cuMemcpyAsync(CUdeviceptr dst, CUdeviceptr src, size_t size, void *stream) {
    int owner = ((struct handle *)stream)->device;
    owned("stream", owner);
    int target = device_of(dst);
    if (target >= 0 && target != owner) {
        violation("copy into a buffer", target);
    }
    memcpy((void *)(size_t)dst, (const void *)(size_t)src, size);
    return 0;
}
