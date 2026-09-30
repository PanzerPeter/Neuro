// MLIR's GPU runtime ABI (`mgpu*`) over the CUDA driver API: the calls a `@gpu`
// launcher makes (module load and launch, streams) and the ones the device staging
// around it makes (allocation and copies). Provenance of gpu_runtime.ll.
//
// The driver is opened with dlopen rather than linked, so a binary built with `@gpu`
// still starts on a machine without an NVIDIA driver and says why it cannot run,
// instead of failing in the dynamic loader. Every launcher's module is loaded by a
// global constructor, so that check happens at startup, before `main`.
//
// When every `@gpu` function in the program has a host fallback, the backend defines
// `__neuro_gpu_fallback` as 1: a module load then leaves its module null instead of
// aborting, and each call asks `__neuro_gpu_usable` which body to run. Nothing reaches a
// null module, because the host body is chosen whenever the probe failed.
//
// Failures end in `__neuro_gpu_panic`, which the LLVM backend defines: it prints
// `panic: <message>` and aborts like every other runtime panic, flushing buffered
// standard output first. Only `mgpuMemAlloc` reports failure by returning null, which
// the device allocator turns into its own diagnostic.
//
// The `__neuro_device_*` functions are the other client: `.to(Device::GPU(n))` and a
// `@gpu` result left on the device. A program that transfers a tensor links this file
// even when it has no `@gpu` function, and finds out whether a GPU is usable at its first
// transfer rather than at startup.
//
// Single-threaded by design: the primary context of device 0 is made current once, on
// the thread that loads the first module, and Neuro programs run on that thread.
//
// Regenerate with:
//   clang -O2 -S -emit-llvm -fno-stack-protector -fno-unwind-tables \
//     -fno-asynchronous-unwind-tables gpu_runtime.c -o gpu_runtime.ll
// then strip the target datalayout and triple, attribute groups and metadata.

#include <dlfcn.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

typedef int CUresult;
typedef int CUdevice;
typedef void *CUcontext;
typedef void *CUmodule;
typedef void *CUfunction;
typedef void *CUstream;
typedef unsigned long long CUdeviceptr;

#define CU_STREAM_NON_BLOCKING 1u
#define CU_MEM_ATTACH_GLOBAL 1u

extern void __neuro_gpu_panic(const char *message, int64_t length)
    __attribute__((noreturn));

extern const uint8_t __neuro_gpu_fallback;

static struct {
    CUresult (*init)(unsigned);
    CUresult (*get_error_name)(CUresult, const char **);
    CUresult (*device_get_count)(int *);
    CUresult (*device_get)(CUdevice *, int);
    CUresult (*primary_ctx_retain)(CUcontext *, CUdevice);
    CUresult (*ctx_set_current)(CUcontext);
    CUresult (*module_load_data)(CUmodule *, const void *);
    CUresult (*module_unload)(CUmodule);
    CUresult (*module_get_function)(CUfunction *, CUmodule, const char *);
    CUresult (*launch_kernel)(CUfunction, unsigned, unsigned, unsigned, unsigned,
                              unsigned, unsigned, unsigned, CUstream, void **,
                              void **);
    CUresult (*stream_create)(CUstream *, unsigned);
    CUresult (*stream_synchronize)(CUstream);
    CUresult (*mem_alloc)(CUdeviceptr *, size_t);
    CUresult (*mem_alloc_managed)(CUdeviceptr *, size_t, unsigned);
    CUresult (*mem_free)(CUdeviceptr);
    CUresult (*memcpy_async)(CUdeviceptr, CUdeviceptr, size_t, CUstream);
} cu;

// The `_v2` names are what cuda.h's macros resolve the unsuffixed ones to.
static const struct {
    const char *name;
    void **slot;
} entry_points[] = {
    {"cuInit", (void **)&cu.init},
    {"cuGetErrorName", (void **)&cu.get_error_name},
    {"cuDeviceGetCount", (void **)&cu.device_get_count},
    {"cuDeviceGet", (void **)&cu.device_get},
    {"cuDevicePrimaryCtxRetain", (void **)&cu.primary_ctx_retain},
    {"cuCtxSetCurrent", (void **)&cu.ctx_set_current},
    {"cuModuleLoadData", (void **)&cu.module_load_data},
    {"cuModuleUnload", (void **)&cu.module_unload},
    {"cuModuleGetFunction", (void **)&cu.module_get_function},
    {"cuLaunchKernel", (void **)&cu.launch_kernel},
    {"cuStreamCreate", (void **)&cu.stream_create},
    {"cuStreamSynchronize", (void **)&cu.stream_synchronize},
    {"cuMemAlloc_v2", (void **)&cu.mem_alloc},
    {"cuMemAllocManaged", (void **)&cu.mem_alloc_managed},
    {"cuMemFree_v2", (void **)&cu.mem_free},
    {"cuMemcpyAsync", (void **)&cu.memcpy_async},
};

static int probed;
static int ready;
// Why the probe found no usable GPU: a literal or a driver-owned error name, both static.
static const char *unusable_reason;

#define MESSAGE_CAPACITY 256

// `length` is what snprintf returned for `message`, clamped here to what it holds.
static void __attribute__((noreturn)) report(const char *message, int length) {
    if (length < 0) {
        length = 0;
    }
    if (length >= MESSAGE_CAPACITY) {
        length = MESSAGE_CAPACITY - 1;
    }
    __neuro_gpu_panic(message, length);
}

static void __attribute__((noreturn)) fail(const char *format, const char *detail) {
    char message[MESSAGE_CAPACITY];
    report(message, snprintf(message, sizeof message, format, detail));
}

// `what` names what asked for the GPU: a `@gpu` launch, or a tensor transfer.
static void __attribute__((noreturn)) unusable(const char *what, const char *reason) {
    char message[MESSAGE_CAPACITY];
    report(message, snprintf(message, sizeof message,
                             "%s needs an NVIDIA GPU, and none is usable: %s", what, reason));
}

static const char LAUNCH[] = "`@gpu`";
static const char TRANSFER[] = "`Device::GPU`";

static const char *error_name(CUresult result) {
    const char *name = NULL;
    if (cu.get_error_name(result, &name) != 0 || name == NULL) {
        return "an unknown CUDA error";
    }
    return name;
}

static void check(CUresult result, const char *call) {
    if (result == 0) {
        return;
    }
    char context[128];
    snprintf(context, sizeof context, "%s failed with %s", call, error_name(result));
    fail("GPU error: %s", context);
}

// Open the driver and make device 0's primary context current, once. A failure is
// recorded rather than reported: whether it is fatal is the caller's to decide.
static void probe(void) {
    if (probed) {
        return;
    }
    probed = 1;
    void *driver = dlopen("libcuda.so.1", RTLD_NOW | RTLD_LOCAL);
    if (driver == NULL) {
        unusable_reason = "the CUDA driver (libcuda.so.1) could not be loaded";
        return;
    }
    for (size_t i = 0; i < sizeof entry_points / sizeof entry_points[0]; i++) {
        *entry_points[i].slot = dlsym(driver, entry_points[i].name);
        if (*entry_points[i].slot == NULL) {
            unusable_reason = "the CUDA driver is too old";
            return;
        }
    }
    CUresult result = cu.init(0);
    if (result != 0) {
        unusable_reason = error_name(result);
        return;
    }
    int count = 0;
    if (cu.device_get_count(&count) != 0 || count == 0) {
        unusable_reason = "the CUDA driver reports no device";
        return;
    }
    CUdevice device;
    CUcontext context;
    if ((result = cu.device_get(&device, 0)) != 0 ||
        (result = cu.primary_ctx_retain(&context, device)) != 0 ||
        (result = cu.ctx_set_current(context)) != 0) {
        unusable_reason = error_name(result);
        return;
    }
    ready = 1;
}

static void ensure_ready_for(const char *what) {
    probe();
    if (!ready) {
        unusable(what, unusable_reason);
    }
}

static void ensure_ready(void) {
    ensure_ready_for(LAUNCH);
}

// Which body a `@gpu(fallback: true)` function runs: its kernels, or its host copy.
int32_t __neuro_gpu_usable(void) {
    probe();
    return ready;
}

static CUmodule load(const void *data) {
    probe();
    if (!ready && __neuro_gpu_fallback) {
        return NULL;
    }
    ensure_ready();
    CUmodule module;
    check(cu.module_load_data(&module, data), "cuModuleLoadData");
    return module;
}

CUmodule mgpuModuleLoad(void *data, size_t size) {
    (void)size;
    return load(data);
}

// PTX is NUL-terminated text, so the driver needs no size; the optimization level is
// left to the driver's default, its highest.
CUmodule mgpuModuleLoadJIT(void *data, int32_t optimization_level) {
    (void)optimization_level;
    return load(data);
}

// Runs from a global destructor, which may come after the driver has shut down at
// exit; a failure there has nothing left to protect.
void mgpuModuleUnload(CUmodule module) {
    if (ready) {
        cu.module_unload(module);
    }
}

CUfunction mgpuModuleGetFunction(CUmodule module, const char *name) {
    CUfunction function;
    check(cu.module_get_function(&function, module, name), "cuModuleGetFunction");
    return function;
}

void mgpuLaunchKernel(CUfunction function, intptr_t grid_x, intptr_t grid_y,
                      intptr_t grid_z, intptr_t block_x, intptr_t block_y,
                      intptr_t block_z, int32_t shared_bytes, CUstream stream,
                      void **params, void **extra, size_t param_count) {
    (void)param_count;
    check(cu.launch_kernel(function, grid_x, grid_y, grid_z, block_x, block_y,
                           block_z, shared_bytes, stream, params, extra),
          "cuLaunchKernel");
}

// Every caller gets the same stream, created on first use and kept for the life of the
// program. The launchers ask for a stream per kernel and the staging around them for one
// per call; creating and destroying each costs more than a small kernel runs, and one
// in-order stream is also what orders a call's copies before its kernels and its
// kernels before the copy back. The driver releases it at exit.
static CUstream shared_stream;

CUstream mgpuStreamCreate(void) {
    if (shared_stream == NULL) {
        ensure_ready();
        check(cu.stream_create(&shared_stream, CU_STREAM_NON_BLOCKING), "cuStreamCreate");
    }
    return shared_stream;
}

void mgpuStreamSynchronize(CUstream stream) {
    check(cu.stream_synchronize(stream), "cuStreamSynchronize");
}

void mgpuStreamDestroy(CUstream stream) {
    (void)stream;
}

// `host_shared` is a byte, not a `bool`, to match the `i8` the callers declare.
void *mgpuMemAlloc(uint64_t size, CUstream stream, uint8_t host_shared) {
    (void)stream;
    ensure_ready();
    if (size == 0) {
        return NULL;
    }
    CUdeviceptr pointer = 0;
    CUresult result = host_shared
                          ? cu.mem_alloc_managed(&pointer, size, CU_MEM_ATTACH_GLOBAL)
                          : cu.mem_alloc(&pointer, size);
    return result == 0 ? (void *)(uintptr_t)pointer : NULL;
}

void mgpuMemFree(void *pointer, CUstream stream) {
    (void)stream;
    check(cu.mem_free((CUdeviceptr)(uintptr_t)pointer), "cuMemFree");
}

void mgpuMemcpy(void *dst, void *src, size_t size, CUstream stream) {
    check(cu.memcpy_async((CUdeviceptr)(uintptr_t)dst, (CUdeviceptr)(uintptr_t)src,
                          size, stream),
          "cuMemcpyAsync");
}

// A device tensor's buffer is an allocation of its own rather than a piece of the device
// arena: it lives until the tensor is dropped, which no call's mark can see.

// Only device 0's context is ever made current, so a tensor can live on GPU 0 alone.
void __neuro_device_check(int32_t device) {
    ensure_ready_for(TRANSFER);
    if (device == 0) {
        return;
    }
    int count = 0;
    cu.device_get_count(&count);
    char message[MESSAGE_CAPACITY];
    if (device < 0 || device >= count) {
        report(message, snprintf(message, sizeof message,
                                 "`Device::GPU(%d)` names no GPU: this machine has %d",
                                 device, count));
    }
    report(message, snprintf(message, sizeof message,
                             "`Device::GPU(%d)` is not supported yet: a tensor can "
                             "live on GPU 0 only",
                             device));
}

void *__neuro_device_alloc(uint64_t size) {
    ensure_ready_for(TRANSFER);
    CUdeviceptr pointer = 0;
    // A tensor with a zero extent still gets an address of its own.
    check(cu.mem_alloc(&pointer, size == 0 ? 1 : size), "cuMemAlloc");
    return (void *)(uintptr_t)pointer;
}

// Waits for the copy: the caller releases the host buffer next.
void *__neuro_device_upload(void *host, uint64_t size, int32_t device) {
    __neuro_device_check(device);
    void *buffer = __neuro_device_alloc(size);
    CUstream stream = mgpuStreamCreate();
    mgpuMemcpy(buffer, host, size, stream);
    mgpuStreamSynchronize(stream);
    return buffer;
}

// Queued behind every kernel still writing `device`, on the one stream they run on.
void __neuro_device_download(void *host, void *device, uint64_t size) {
    CUstream stream = mgpuStreamCreate();
    mgpuMemcpy(host, device, size, stream);
    mgpuStreamSynchronize(stream);
}

// A kernel still queued may read the buffer, so the stream drains first.
void __neuro_device_free(void *device) {
    mgpuStreamSynchronize(mgpuStreamCreate());
    check(cu.mem_free((CUdeviceptr)(uintptr_t)device), "cuMemFree");
}
