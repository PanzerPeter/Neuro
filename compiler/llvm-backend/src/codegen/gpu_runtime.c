// MLIR's GPU runtime ABI (`mgpu*`) over the CUDA driver API, or over HIP when built with
// `-DNEURO_HIP`: the calls a `@gpu` launcher makes (module load and launch, streams) and
// the ones the device staging around it makes (allocation and copies). Provenance of
// gpu_runtime.ll (CUDA) and gpu_runtime_hip.ll (HIP). Only the vendor block below differs
// between the two; everything after it is shared, so a fix lands in both.
//
// The vendor library is opened with dlopen rather than linked, so a binary built with
// `@gpu` still starts on a machine without that GPU and says why it cannot run, instead
// of failing in the dynamic loader. Every launcher's module is loaded by a global
// constructor, so that check happens at startup, before `main`.
//
// When every `@gpu` function in the program has a host fallback, the backend defines
// `__neuro_gpu_fallback` as 1: a module load then leaves its module null instead of
// aborting, whether the probe failed or the driver refused the module, and each call
// asks `__neuro_gpu_usable` which body to run. Nothing reaches a null module, because
// the host body is chosen whenever either happened.
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
// Single-threaded by design: device 0 is made current once, on the thread that loads the
// first module, and Neuro programs run on that thread.
//
// Regenerate both .ll files with tools/regen_gpu_runtime.sh.

#include <dlfcn.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#ifdef NEURO_HIP

// HIP, ROCm's runtime API. It keeps a primary context per device itself, so selecting
// device 0 is all the setup there is, and its device pointers are plain pointers.
typedef int gpu_result;
typedef void *gpu_module;
typedef void *gpu_function;
typedef void *gpu_stream;

#define VENDOR "AMD"
#define NAME(cuda, hip) hip
#define STREAM_NON_BLOCKING 1u
#define MEM_ATTACH_GLOBAL 1u
#define MEMCPY_DEFAULT 4

// The unversioned name is what a ROCm install puts on the loader path; a runtime-only
// package ships just the versioned one, and a stock install leaves /opt/rocm/lib off it.
static const char *const libraries[] = {
    "libamdhip64.so",
    "libamdhip64.so.7",
    "libamdhip64.so.6",
    "/opt/rocm/lib/libamdhip64.so",
};
static const char NO_LIBRARY[] = "the HIP runtime (libamdhip64.so) could not be loaded";
static const char TOO_OLD[] = "the HIP runtime is too old";
static const char NO_DEVICE[] = "the HIP runtime reports no device";

static struct {
    gpu_result (*init)(unsigned);
    const char *(*get_error_name)(gpu_result);
    gpu_result (*device_get_count)(int *);
    gpu_result (*set_device)(int);
    gpu_result (*module_load_data)(gpu_module *, const void *);
    gpu_result (*module_unload)(gpu_module);
    gpu_result (*module_get_function)(gpu_function *, gpu_module, const char *);
    gpu_result (*launch_kernel)(gpu_function, unsigned, unsigned, unsigned, unsigned,
                                unsigned, unsigned, unsigned, gpu_stream, void **,
                                void **);
    gpu_result (*stream_create)(gpu_stream *, unsigned);
    gpu_result (*stream_synchronize)(gpu_stream);
    gpu_result (*mem_alloc)(void **, size_t);
    gpu_result (*mem_alloc_managed)(void **, size_t, unsigned);
    gpu_result (*mem_free)(void *);
    gpu_result (*memcpy_async)(void *, const void *, size_t, int, gpu_stream);
} drv;

static const struct {
    const char *name;
    void **slot;
} entry_points[] = {
    {"hipInit", (void **)&drv.init},
    {"hipGetErrorName", (void **)&drv.get_error_name},
    {"hipGetDeviceCount", (void **)&drv.device_get_count},
    {"hipSetDevice", (void **)&drv.set_device},
    {"hipModuleLoadData", (void **)&drv.module_load_data},
    {"hipModuleUnload", (void **)&drv.module_unload},
    {"hipModuleGetFunction", (void **)&drv.module_get_function},
    {"hipModuleLaunchKernel", (void **)&drv.launch_kernel},
    {"hipStreamCreateWithFlags", (void **)&drv.stream_create},
    {"hipStreamSynchronize", (void **)&drv.stream_synchronize},
    {"hipMalloc", (void **)&drv.mem_alloc},
    {"hipMallocManaged", (void **)&drv.mem_alloc_managed},
    {"hipFree", (void **)&drv.mem_free},
    {"hipMemcpyAsync", (void **)&drv.memcpy_async},
};

static const char *error_name(gpu_result result) {
    const char *name = drv.get_error_name(result);
    return name != NULL ? name : "an unknown HIP error";
}

static gpu_result open_device(void) {
    return drv.set_device(0);
}

static gpu_result allocate(void **pointer, size_t size, int managed) {
    *pointer = NULL;
    return managed ? drv.mem_alloc_managed(pointer, size, MEM_ATTACH_GLOBAL)
                   : drv.mem_alloc(pointer, size);
}

static gpu_result release(void *pointer) {
    return drv.mem_free(pointer);
}

// Device and host pointers share one address space, so the runtime tells the direction.
static gpu_result copy(void *dst, void *src, size_t size, gpu_stream stream) {
    return drv.memcpy_async(dst, src, size, MEMCPY_DEFAULT, stream);
}

#else

// The CUDA driver API. Its device pointers are integers, and a context has to be
// retained and made current by hand.
typedef int gpu_result;
typedef int CUdevice;
typedef void *CUcontext;
typedef void *gpu_module;
typedef void *gpu_function;
typedef void *gpu_stream;
typedef unsigned long long CUdeviceptr;

#define VENDOR "NVIDIA"
#define NAME(cuda, hip) cuda
#define STREAM_NON_BLOCKING 1u
#define MEM_ATTACH_GLOBAL 1u

static const char *const libraries[] = {"libcuda.so.1"};
static const char NO_LIBRARY[] = "the CUDA driver (libcuda.so.1) could not be loaded";
static const char TOO_OLD[] = "the CUDA driver is too old";
static const char NO_DEVICE[] = "the CUDA driver reports no device";

static struct {
    gpu_result (*init)(unsigned);
    gpu_result (*get_error_name)(gpu_result, const char **);
    gpu_result (*device_get_count)(int *);
    gpu_result (*device_get)(CUdevice *, int);
    gpu_result (*primary_ctx_retain)(CUcontext *, CUdevice);
    gpu_result (*ctx_set_current)(CUcontext);
    gpu_result (*module_load_data)(gpu_module *, const void *);
    gpu_result (*module_unload)(gpu_module);
    gpu_result (*module_get_function)(gpu_function *, gpu_module, const char *);
    gpu_result (*launch_kernel)(gpu_function, unsigned, unsigned, unsigned, unsigned,
                                unsigned, unsigned, unsigned, gpu_stream, void **,
                                void **);
    gpu_result (*stream_create)(gpu_stream *, unsigned);
    gpu_result (*stream_synchronize)(gpu_stream);
    gpu_result (*mem_alloc)(CUdeviceptr *, size_t);
    gpu_result (*mem_alloc_managed)(CUdeviceptr *, size_t, unsigned);
    gpu_result (*mem_free)(CUdeviceptr);
    gpu_result (*memcpy_async)(CUdeviceptr, CUdeviceptr, size_t, gpu_stream);
} drv;

// The `_v2` names are what cuda.h's macros resolve the unsuffixed ones to.
static const struct {
    const char *name;
    void **slot;
} entry_points[] = {
    {"cuInit", (void **)&drv.init},
    {"cuGetErrorName", (void **)&drv.get_error_name},
    {"cuDeviceGetCount", (void **)&drv.device_get_count},
    {"cuDeviceGet", (void **)&drv.device_get},
    {"cuDevicePrimaryCtxRetain", (void **)&drv.primary_ctx_retain},
    {"cuCtxSetCurrent", (void **)&drv.ctx_set_current},
    {"cuModuleLoadData", (void **)&drv.module_load_data},
    {"cuModuleUnload", (void **)&drv.module_unload},
    {"cuModuleGetFunction", (void **)&drv.module_get_function},
    {"cuLaunchKernel", (void **)&drv.launch_kernel},
    {"cuStreamCreate", (void **)&drv.stream_create},
    {"cuStreamSynchronize", (void **)&drv.stream_synchronize},
    {"cuMemAlloc_v2", (void **)&drv.mem_alloc},
    {"cuMemAllocManaged", (void **)&drv.mem_alloc_managed},
    {"cuMemFree_v2", (void **)&drv.mem_free},
    {"cuMemcpyAsync", (void **)&drv.memcpy_async},
};

static const char *error_name(gpu_result result) {
    const char *name = NULL;
    if (drv.get_error_name(result, &name) != 0 || name == NULL) {
        return "an unknown CUDA error";
    }
    return name;
}

static gpu_result open_device(void) {
    CUdevice device;
    CUcontext context;
    gpu_result result;
    if ((result = drv.device_get(&device, 0)) != 0 ||
        (result = drv.primary_ctx_retain(&context, device)) != 0) {
        return result;
    }
    return drv.ctx_set_current(context);
}

static gpu_result allocate(void **pointer, size_t size, int managed) {
    CUdeviceptr address = 0;
    gpu_result result = managed ? drv.mem_alloc_managed(&address, size, MEM_ATTACH_GLOBAL)
                                : drv.mem_alloc(&address, size);
    *pointer = (void *)(uintptr_t)address;
    return result;
}

static gpu_result release(void *pointer) {
    return drv.mem_free((CUdeviceptr)(uintptr_t)pointer);
}

static gpu_result copy(void *dst, void *src, size_t size, gpu_stream stream) {
    return drv.memcpy_async((CUdeviceptr)(uintptr_t)dst, (CUdeviceptr)(uintptr_t)src, size,
                            stream);
}

#endif

extern void __neuro_gpu_panic(const char *message, int64_t length)
    __attribute__((noreturn));

extern const uint8_t __neuro_gpu_fallback;

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
                             "%s needs an " VENDOR " GPU, and none is usable: %s", what,
                             reason));
}

static const char LAUNCH[] = "`@gpu`";
static const char TRANSFER[] = "`Device::GPU`";

static void check(gpu_result result, const char *call) {
    if (result == 0) {
        return;
    }
    char context[128];
    snprintf(context, sizeof context, "%s failed with %s", call, error_name(result));
    fail("GPU error: %s", context);
}

// Open the vendor library and make device 0 current, once. A failure is recorded rather
// than reported: whether it is fatal is the caller's to decide.
static void probe(void) {
    if (probed) {
        return;
    }
    probed = 1;
    void *library = NULL;
    for (size_t i = 0; library == NULL && i < sizeof libraries / sizeof libraries[0]; i++) {
        library = dlopen(libraries[i], RTLD_NOW | RTLD_LOCAL);
    }
    if (library == NULL) {
        unusable_reason = NO_LIBRARY;
        return;
    }
    for (size_t i = 0; i < sizeof entry_points / sizeof entry_points[0]; i++) {
        *entry_points[i].slot = dlsym(library, entry_points[i].name);
        if (*entry_points[i].slot == NULL) {
            unusable_reason = TOO_OLD;
            return;
        }
    }
    gpu_result result = drv.init(0);
    if (result != 0) {
        unusable_reason = error_name(result);
        return;
    }
    int count = 0;
    if (drv.device_get_count(&count) != 0 || count == 0) {
        unusable_reason = NO_DEVICE;
        return;
    }
    if ((result = open_device()) != 0) {
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

// Why a present GPU is still unusable: its driver refused a module, say PTX newer than
// it reads, a chip too old to JIT it, or a code object built for another chip.
static char load_failure[128];

static gpu_module load(const void *data) {
    probe();
    if (!ready && __neuro_gpu_fallback) {
        return NULL;
    }
    if (!ready) {
        unusable(LAUNCH, unusable_reason);
    }
    gpu_module module = NULL;
    gpu_result result = drv.module_load_data(&module, data);
    if (result != 0 && __neuro_gpu_fallback) {
        // Loads run in global constructors, before any call has picked its GPU body, so
        // clearing `ready` sends every call to its host body. Modules already loaded
        // sit unused until exit.
        snprintf(load_failure, sizeof load_failure,
                 "its driver cannot load this program's kernels (%s)", error_name(result));
        unusable_reason = load_failure;
        ready = 0;
        return NULL;
    }
    check(result, NAME("cuModuleLoadData", "hipModuleLoadData"));
    return module;
}

// An AMD kernel is a code object, which carries its own size.
gpu_module mgpuModuleLoad(void *data, size_t size) {
    (void)size;
    return load(data);
}

// Only NVIDIA's launchers call this. PTX is NUL-terminated text, so the driver needs no
// size; the optimization level is left to the driver's default, its highest.
gpu_module mgpuModuleLoadJIT(void *data, int32_t optimization_level) {
    (void)optimization_level;
    return load(data);
}

// Runs from a global destructor, which may come after the driver has shut down at
// exit; a failure there has nothing left to protect.
void mgpuModuleUnload(gpu_module module) {
    if (ready) {
        drv.module_unload(module);
    }
}

gpu_function mgpuModuleGetFunction(gpu_module module, const char *name) {
    gpu_function function;
    check(drv.module_get_function(&function, module, name),
          NAME("cuModuleGetFunction", "hipModuleGetFunction"));
    return function;
}

void mgpuLaunchKernel(gpu_function function, intptr_t grid_x, intptr_t grid_y,
                      intptr_t grid_z, intptr_t block_x, intptr_t block_y,
                      intptr_t block_z, int32_t shared_bytes, gpu_stream stream,
                      void **params, void **extra, size_t param_count) {
    (void)param_count;
    check(drv.launch_kernel(function, grid_x, grid_y, grid_z, block_x, block_y, block_z,
                            shared_bytes, stream, params, extra),
          NAME("cuLaunchKernel", "hipModuleLaunchKernel"));
}

// Every caller gets the same stream, created on first use and kept for the life of the
// program. The launchers ask for a stream per kernel and the staging around them for one
// per call; creating and destroying each costs more than a small kernel runs, and one
// in-order stream is also what orders a call's copies before its kernels and its
// kernels before the copy back. The driver releases it at exit.
static gpu_stream shared_stream;

gpu_stream mgpuStreamCreate(void) {
    if (shared_stream == NULL) {
        ensure_ready();
        check(drv.stream_create(&shared_stream, STREAM_NON_BLOCKING),
              NAME("cuStreamCreate", "hipStreamCreateWithFlags"));
    }
    return shared_stream;
}

void mgpuStreamSynchronize(gpu_stream stream) {
    check(drv.stream_synchronize(stream), NAME("cuStreamSynchronize", "hipStreamSynchronize"));
}

void mgpuStreamDestroy(gpu_stream stream) {
    (void)stream;
}

// `host_shared` is a byte, not a `bool`, to match the `i8` the callers declare.
void *mgpuMemAlloc(uint64_t size, gpu_stream stream, uint8_t host_shared) {
    (void)stream;
    ensure_ready();
    if (size == 0) {
        return NULL;
    }
    void *pointer = NULL;
    return allocate(&pointer, size, host_shared) == 0 ? pointer : NULL;
}

void mgpuMemFree(void *pointer, gpu_stream stream) {
    (void)stream;
    check(release(pointer), NAME("cuMemFree", "hipFree"));
}

void mgpuMemcpy(void *dst, void *src, size_t size, gpu_stream stream) {
    check(copy(dst, src, size, stream), NAME("cuMemcpyAsync", "hipMemcpyAsync"));
}

// A device tensor's buffer is an allocation of its own rather than a piece of the device
// arena: it lives until the tensor is dropped, which no call's mark can see.

// Only device 0 is ever made current, so a tensor can live on GPU 0 alone.
void __neuro_device_check(int32_t device) {
    ensure_ready_for(TRANSFER);
    if (device == 0) {
        return;
    }
    int count = 0;
    drv.device_get_count(&count);
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
    void *pointer = NULL;
    // A tensor with a zero extent still gets an address of its own.
    check(allocate(&pointer, size == 0 ? 1 : size, 0), NAME("cuMemAlloc", "hipMalloc"));
    return pointer;
}

// Waits for the copy: the caller releases the host buffer next.
void *__neuro_device_upload(void *host, uint64_t size, int32_t device) {
    __neuro_device_check(device);
    void *buffer = __neuro_device_alloc(size);
    gpu_stream stream = mgpuStreamCreate();
    mgpuMemcpy(buffer, host, size, stream);
    mgpuStreamSynchronize(stream);
    return buffer;
}

// Queued behind every kernel still writing `device`, on the one stream they run on.
void __neuro_device_download(void *host, void *device, uint64_t size) {
    gpu_stream stream = mgpuStreamCreate();
    mgpuMemcpy(host, device, size, stream);
    mgpuStreamSynchronize(stream);
}

// A kernel still queued may read the buffer, so the stream drains first.
void __neuro_device_free(void *device) {
    mgpuStreamSynchronize(mgpuStreamCreate());
    check(release(device), NAME("cuMemFree", "hipFree"));
}
