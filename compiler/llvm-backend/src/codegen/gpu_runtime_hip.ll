;
; MLIR's GPU runtime ABI (`mgpu*`) over HIP, linked in place of gpu_runtime.ll when the
; program is built for an AMD GPU. HIP (libamdhip64) is opened with dlopen, so a binary
; without a usable AMD GPU starts, then panics from the first module load (a global
; constructor) saying why, unless every `@gpu` function has a host fallback
; (`__neuro_gpu_fallback`), in which case `__neuro_gpu_usable` sends each call to its
; host body. The `__neuro_device_*` functions move a tensor's buffer to and from the
; GPU for `.to(Device::GPU(n))`, so a program that transfers a tensor links this module
; too. Failures call `__neuro_gpu_panic`, which the LLVM backend defines as an ordinary
; runtime panic.
;
; Generated from compiler/llvm-backend/src/codegen/gpu_runtime.c with clang -O2
; -emit-llvm -DNEURO_HIP, then stripped of the target datalayout and triple, attribute
; groups and metadata. Regenerate it with tools/regen_gpu_runtime.sh.
;

@ready = internal unnamed_addr global i1 false, align 4
@drv.0 = internal unnamed_addr global ptr null, align 8
@drv.1 = internal unnamed_addr global ptr null, align 8
@drv.2 = internal unnamed_addr global ptr null, align 8
@drv.3 = internal unnamed_addr global ptr null, align 8
@drv.4 = internal unnamed_addr global ptr null, align 8
@drv.5 = internal unnamed_addr global ptr null, align 8
@drv.6 = internal unnamed_addr global ptr null, align 8
@drv.7 = internal unnamed_addr global ptr null, align 8
@drv.8 = internal unnamed_addr global ptr null, align 8
@drv.9 = internal unnamed_addr global ptr null, align 8
@drv.10 = internal unnamed_addr global ptr null, align 8
@drv.11 = internal unnamed_addr global ptr null, align 8
@drv.12 = internal unnamed_addr global ptr null, align 8
@drv.13 = internal unnamed_addr global ptr null, align 8
@.str = private unnamed_addr constant [21 x i8] c"hipModuleGetFunction\00", align 1
@.str.1 = private unnamed_addr constant [22 x i8] c"hipModuleLaunchKernel\00", align 1
@shared_stream = internal global ptr null, align 8
@.str.2 = private unnamed_addr constant [25 x i8] c"hipStreamCreateWithFlags\00", align 1
@.str.3 = private unnamed_addr constant [21 x i8] c"hipStreamSynchronize\00", align 1
@.str.4 = private unnamed_addr constant [8 x i8] c"hipFree\00", align 1
@.str.5 = private unnamed_addr constant [15 x i8] c"hipMemcpyAsync\00", align 1
@TRANSFER = internal constant [14 x i8] c"`Device::GPU`\00", align 1
@.str.6 = private unnamed_addr constant [52 x i8] c"`Device::GPU(%d)` names no GPU: this machine has %d\00", align 1
@.str.7 = private unnamed_addr constant [72 x i8] c"`Device::GPU(%d)` is not supported yet: a tensor can live on GPU 0 only\00", align 1
@.str.8 = private unnamed_addr constant [10 x i8] c"hipMalloc\00", align 1
@probed = internal unnamed_addr global i1 false, align 4
@libraries.rel = internal unnamed_addr constant [4 x i32] [i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.9 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32), i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.10 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32), i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.11 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32), i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.12 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32)], align 4
@NO_LIBRARY = internal constant [53 x i8] c"the HIP runtime (libamdhip64.so) could not be loaded\00", align 16
@unusable_reason = internal unnamed_addr global ptr null, align 8
@TOO_OLD = internal constant [27 x i8] c"the HIP runtime is too old\00", align 16
@NO_DEVICE = internal constant [34 x i8] c"the HIP runtime reports no device\00", align 16
@.str.9 = private unnamed_addr constant [15 x i8] c"libamdhip64.so\00", align 1
@.str.10 = private unnamed_addr constant [17 x i8] c"libamdhip64.so.7\00", align 1
@.str.11 = private unnamed_addr constant [17 x i8] c"libamdhip64.so.6\00", align 1
@.str.12 = private unnamed_addr constant [29 x i8] c"/opt/rocm/lib/libamdhip64.so\00", align 1
@.str.13 = private unnamed_addr constant [8 x i8] c"hipInit\00", align 1
@.str.14 = private unnamed_addr constant [16 x i8] c"hipGetErrorName\00", align 1
@.str.15 = private unnamed_addr constant [18 x i8] c"hipGetDeviceCount\00", align 1
@.str.16 = private unnamed_addr constant [13 x i8] c"hipSetDevice\00", align 1
@.str.17 = private unnamed_addr constant [18 x i8] c"hipModuleLoadData\00", align 1
@.str.18 = private unnamed_addr constant [16 x i8] c"hipModuleUnload\00", align 1
@.str.19 = private unnamed_addr constant [17 x i8] c"hipMallocManaged\00", align 1
@.str.20 = private unnamed_addr constant [21 x i8] c"an unknown HIP error\00", align 1
@__neuro_gpu_fallback = external local_unnamed_addr constant i8, align 1
@LAUNCH = internal constant [7 x i8] c"`@gpu`\00", align 1
@load_failure = internal global [128 x i8] zeroinitializer, align 16
@.str.21 = private unnamed_addr constant [51 x i8] c"its driver cannot load this program's kernels (%s)\00", align 1
@.str.22 = private unnamed_addr constant [44 x i8] c"%s needs an AMD GPU, and none is usable: %s\00", align 1
@.str.23 = private unnamed_addr constant [18 x i8] c"%s failed with %s\00", align 1
@.str.24 = private unnamed_addr constant [14 x i8] c"GPU error: %s\00", align 1

define dso_local range(i32 0, 2) i32 @__neuro_gpu_usable() local_unnamed_addr {
  tail call fastcc void @probe()
  %1 = load i1, ptr @ready, align 4
  %2 = zext i1 %1 to i32
  ret i32 %2
}

define internal fastcc void @probe() unnamed_addr {
  %1 = alloca i32, align 4
  %2 = load i1, ptr @probed, align 4
  br i1 %2, label %86, label %3

3:                                                ; preds = %0
  store i1 true, ptr @probed, align 4
  br label %8

4:                                                ; preds = %8
  br i1 %14, label %17, label %5

5:                                                ; preds = %4
  %6 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.13)
  store ptr %6, ptr @drv.0, align 8
  %7 = icmp eq ptr %6, null
  br i1 %7, label %61, label %18

8:                                                ; preds = %3, %8
  %9 = phi i64 [ 0, %3 ], [ %13, %8 ]
  %10 = shl i64 %9, 2
  %11 = call ptr @llvm.load.relative.i64(ptr @libraries.rel, i64 %10)
  %12 = tail call ptr @dlopen(ptr noundef %11, i32 noundef 2)
  %13 = add nuw nsw i64 %9, 1
  %14 = icmp eq ptr %12, null
  %15 = icmp samesign ult i64 %9, 3
  %16 = select i1 %14, i1 %15, i1 false
  br i1 %16, label %8, label %4

17:                                               ; preds = %4
  store ptr @NO_LIBRARY, ptr @unusable_reason, align 8
  br label %86

18:                                               ; preds = %5
  %19 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.14)
  store ptr %19, ptr @drv.1, align 8
  %20 = icmp eq ptr %19, null
  br i1 %20, label %61, label %21

21:                                               ; preds = %18
  %22 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.15)
  store ptr %22, ptr @drv.2, align 8
  %23 = icmp eq ptr %22, null
  br i1 %23, label %61, label %24

24:                                               ; preds = %21
  %25 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.16)
  store ptr %25, ptr @drv.3, align 8
  %26 = icmp eq ptr %25, null
  br i1 %26, label %61, label %27

27:                                               ; preds = %24
  %28 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.17)
  store ptr %28, ptr @drv.4, align 8
  %29 = icmp eq ptr %28, null
  br i1 %29, label %61, label %30

30:                                               ; preds = %27
  %31 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.18)
  store ptr %31, ptr @drv.5, align 8
  %32 = icmp eq ptr %31, null
  br i1 %32, label %61, label %33

33:                                               ; preds = %30
  %34 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str)
  store ptr %34, ptr @drv.6, align 8
  %35 = icmp eq ptr %34, null
  br i1 %35, label %61, label %36

36:                                               ; preds = %33
  %37 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.1)
  store ptr %37, ptr @drv.7, align 8
  %38 = icmp eq ptr %37, null
  br i1 %38, label %61, label %39

39:                                               ; preds = %36
  %40 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.2)
  store ptr %40, ptr @drv.8, align 8
  %41 = icmp eq ptr %40, null
  br i1 %41, label %61, label %42

42:                                               ; preds = %39
  %43 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.3)
  store ptr %43, ptr @drv.9, align 8
  %44 = icmp eq ptr %43, null
  br i1 %44, label %61, label %45

45:                                               ; preds = %42
  %46 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.8)
  store ptr %46, ptr @drv.10, align 8
  %47 = icmp eq ptr %46, null
  br i1 %47, label %61, label %48

48:                                               ; preds = %45
  %49 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.19)
  store ptr %49, ptr @drv.11, align 8
  %50 = icmp eq ptr %49, null
  br i1 %50, label %61, label %51

51:                                               ; preds = %48
  %52 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.4)
  store ptr %52, ptr @drv.12, align 8
  %53 = icmp eq ptr %52, null
  br i1 %53, label %61, label %54

54:                                               ; preds = %51
  %55 = tail call ptr @dlsym(ptr noundef nonnull %12, ptr noundef nonnull @.str.5)
  store ptr %55, ptr @drv.13, align 8
  %56 = icmp eq ptr %55, null
  br i1 %56, label %61, label %57

57:                                               ; preds = %54
  %58 = load ptr, ptr @drv.0, align 8
  %59 = tail call i32 %58(i32 noundef 0)
  %60 = icmp eq i32 %59, 0
  br i1 %60, label %67, label %62

61:                                               ; preds = %54, %51, %48, %45, %42, %39, %36, %33, %30, %27, %24, %21, %18, %5
  store ptr @TOO_OLD, ptr @unusable_reason, align 8
  br label %86

62:                                               ; preds = %57
  %63 = load ptr, ptr @drv.1, align 8
  %64 = tail call ptr %63(i32 noundef range(i32 1, 0) %59)
  %65 = icmp eq ptr %64, null
  %66 = select i1 %65, ptr @.str.20, ptr %64
  store ptr %66, ptr @unusable_reason, align 8
  br label %86

67:                                               ; preds = %57
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  store i32 0, ptr %1, align 4
  %68 = load ptr, ptr @drv.2, align 8
  %69 = call i32 %68(ptr noundef nonnull %1)
  %70 = icmp ne i32 %69, 0
  %71 = load i32, ptr %1, align 4
  %72 = icmp eq i32 %71, 0
  %73 = select i1 %70, i1 true, i1 %72
  br i1 %73, label %74, label %75

74:                                               ; preds = %67
  store ptr @NO_DEVICE, ptr @unusable_reason, align 8
  br label %85

75:                                               ; preds = %67
  %76 = load ptr, ptr @drv.3, align 8
  %77 = call i32 %76(i32 noundef 0)
  %78 = icmp eq i32 %77, 0
  br i1 %78, label %84, label %79

79:                                               ; preds = %75
  %80 = load ptr, ptr @drv.1, align 8
  %81 = call ptr %80(i32 noundef range(i32 1, 0) %77)
  %82 = icmp eq ptr %81, null
  %83 = select i1 %82, ptr @.str.20, ptr %81
  store ptr %83, ptr @unusable_reason, align 8
  br label %85

84:                                               ; preds = %75
  store i1 true, ptr @ready, align 4
  br label %85

85:                                               ; preds = %84, %79, %74
  call void @llvm.lifetime.end.p0(ptr nonnull %1)
  br label %86

86:                                               ; preds = %61, %17, %85, %62, %0
  ret void
}

define dso_local ptr @mgpuModuleLoad(ptr noundef %0, i64 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define internal fastcc ptr @load(ptr noundef %0) unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca ptr, align 8
  tail call fastcc void @probe()
  %4 = load i1, ptr @ready, align 4
  %5 = load i8, ptr @__neuro_gpu_fallback, align 1
  %6 = icmp eq i8 %5, 0
  %7 = select i1 %4, i1 true, i1 %6
  br i1 %7, label %8, label %35

8:                                                ; preds = %1
  br i1 %4, label %11, label %9

9:                                                ; preds = %8
  %10 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %10)
  unreachable

11:                                               ; preds = %8
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  store ptr null, ptr %3, align 8
  %12 = load ptr, ptr @drv.4, align 8
  %13 = call i32 %12(ptr noundef nonnull %3, ptr noundef %0)
  %14 = icmp ne i32 %13, 0
  %15 = icmp ne i8 %5, 0
  %16 = select i1 %14, i1 %15, i1 false
  br i1 %16, label %17, label %23

17:                                               ; preds = %11
  %18 = load ptr, ptr @drv.1, align 8
  %19 = call ptr %18(i32 noundef range(i32 1, 0) %13)
  %20 = icmp eq ptr %19, null
  %21 = select i1 %20, ptr @.str.20, ptr %19
  %22 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) @load_failure, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull %21)
  store ptr @load_failure, ptr @unusable_reason, align 8
  store i1 false, ptr @ready, align 4
  br label %33

23:                                               ; preds = %11
  %24 = icmp eq i32 %13, 0
  br i1 %24, label %31, label %25

25:                                               ; preds = %23
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %26 = load ptr, ptr @drv.1, align 8
  %27 = call ptr %26(i32 noundef range(i32 1, 0) %13)
  %28 = icmp eq ptr %27, null
  %29 = select i1 %28, ptr @.str.20, ptr %27
  %30 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.17, ptr noundef nonnull %29)
  call fastcc void @fail(ptr noundef %2)
  unreachable

31:                                               ; preds = %23
  %32 = load ptr, ptr %3, align 8
  br label %33

33:                                               ; preds = %31, %17
  %34 = phi ptr [ null, %17 ], [ %32, %31 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  br label %35

35:                                               ; preds = %1, %33
  %36 = phi ptr [ %34, %33 ], [ null, %1 ]
  ret ptr %36
}

define dso_local ptr @mgpuModuleLoadJIT(ptr noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define dso_local void @mgpuModuleUnload(ptr noundef %0) local_unnamed_addr {
  %2 = load i1, ptr @ready, align 4
  br i1 %2, label %3, label %6

3:                                                ; preds = %1
  %4 = load ptr, ptr @drv.5, align 8
  %5 = tail call i32 %4(ptr noundef %0)
  br label %6

6:                                                ; preds = %3, %1
  ret void
}

define dso_local ptr @mgpuModuleGetFunction(ptr noundef %0, ptr noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca ptr, align 8
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %5 = load ptr, ptr @drv.6, align 8
  %6 = call i32 %5(ptr noundef nonnull %4, ptr noundef %0, ptr noundef %1)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %14, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = load ptr, ptr @drv.1, align 8
  %10 = call ptr %9(i32 noundef range(i32 1, 0) %6)
  %11 = icmp eq ptr %10, null
  %12 = select i1 %11, ptr @.str.20, ptr %10
  %13 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str, ptr noundef nonnull %12)
  call fastcc void @fail(ptr noundef %3)
  unreachable

14:                                               ; preds = %2
  %15 = load ptr, ptr %4, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  ret ptr %15
}

declare void @llvm.lifetime.start.p0(ptr captures(none))

declare void @llvm.lifetime.end.p0(ptr captures(none))

define dso_local void @mgpuLaunchKernel(ptr noundef %0, i64 noundef %1, i64 noundef %2, i64 noundef %3, i64 noundef %4, i64 noundef %5, i64 noundef %6, i32 noundef %7, ptr noundef %8, ptr noundef %9, ptr noundef %10, i64 noundef %11) local_unnamed_addr {
  %13 = alloca [128 x i8], align 16
  %14 = load ptr, ptr @drv.7, align 8
  %15 = trunc i64 %1 to i32
  %16 = trunc i64 %2 to i32
  %17 = trunc i64 %3 to i32
  %18 = trunc i64 %4 to i32
  %19 = trunc i64 %5 to i32
  %20 = trunc i64 %6 to i32
  %21 = tail call i32 %14(ptr noundef %0, i32 noundef %15, i32 noundef %16, i32 noundef %17, i32 noundef %18, i32 noundef %19, i32 noundef %20, i32 noundef %7, ptr noundef %8, ptr noundef %9, ptr noundef %10)
  %22 = icmp eq i32 %21, 0
  br i1 %22, label %29, label %23

23:                                               ; preds = %12
  call void @llvm.lifetime.start.p0(ptr nonnull %13)
  %24 = load ptr, ptr @drv.1, align 8
  %25 = tail call ptr %24(i32 noundef range(i32 1, 0) %21)
  %26 = icmp eq ptr %25, null
  %27 = select i1 %26, ptr @.str.20, ptr %25
  %28 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %13, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.1, ptr noundef nonnull %27)
  call fastcc void @fail(ptr noundef %13)
  unreachable

29:                                               ; preds = %12
  ret void
}

define dso_local ptr @mgpuStreamCreate() local_unnamed_addr {
  %1 = alloca [128 x i8], align 16
  %2 = load ptr, ptr @shared_stream, align 8
  %3 = icmp eq ptr %2, null
  br i1 %3, label %4, label %20

4:                                                ; preds = %0
  tail call fastcc void @probe()
  %5 = load i1, ptr @ready, align 4
  br i1 %5, label %8, label %6

6:                                                ; preds = %4
  %7 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %7)
  unreachable

8:                                                ; preds = %4
  %9 = load ptr, ptr @drv.8, align 8
  %10 = tail call i32 %9(ptr noundef nonnull @shared_stream, i32 noundef 1)
  %11 = icmp eq i32 %10, 0
  br i1 %11, label %12, label %14

12:                                               ; preds = %8
  %13 = load ptr, ptr @shared_stream, align 8
  br label %20

14:                                               ; preds = %8
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  %15 = load ptr, ptr @drv.1, align 8
  %16 = tail call ptr %15(i32 noundef range(i32 1, 0) %10)
  %17 = icmp eq ptr %16, null
  %18 = select i1 %17, ptr @.str.20, ptr %16
  %19 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %1, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.2, ptr noundef nonnull %18)
  call fastcc void @fail(ptr noundef %1)
  unreachable

20:                                               ; preds = %12, %0
  %21 = phi ptr [ %13, %12 ], [ %2, %0 ]
  ret ptr %21
}

define dso_local void @mgpuStreamSynchronize(ptr noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = load ptr, ptr @drv.9, align 8
  %4 = tail call i32 %3(ptr noundef %0)
  %5 = icmp eq i32 %4, 0
  br i1 %5, label %12, label %6

6:                                                ; preds = %1
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %7 = load ptr, ptr @drv.1, align 8
  %8 = tail call ptr %7(i32 noundef range(i32 1, 0) %4)
  %9 = icmp eq ptr %8, null
  %10 = select i1 %9, ptr @.str.20, ptr %8
  %11 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.3, ptr noundef nonnull %10)
  call fastcc void @fail(ptr noundef %2)
  unreachable

12:                                               ; preds = %1
  ret void
}

define dso_local void @mgpuStreamDestroy(ptr noundef readnone captures(none) %0) local_unnamed_addr {
  ret void
}

define dso_local ptr @mgpuMemAlloc(i64 noundef %0, ptr noundef readnone captures(none) %1, i8 noundef zeroext %2) local_unnamed_addr {
  %4 = alloca ptr, align 8
  tail call fastcc void @probe()
  %5 = load i1, ptr @ready, align 4
  br i1 %5, label %8, label %6

6:                                                ; preds = %3
  %7 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %7)
  unreachable

8:                                                ; preds = %3
  %9 = icmp eq i64 %0, 0
  br i1 %9, label %23, label %10

10:                                               ; preds = %8
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  store ptr null, ptr %4, align 8
  %11 = icmp eq i8 %2, 0
  br i1 %11, label %15, label %12

12:                                               ; preds = %10
  %13 = load ptr, ptr @drv.11, align 8
  %14 = call i32 %13(ptr noundef nonnull %4, i64 noundef %0, i32 noundef 1)
  br label %18

15:                                               ; preds = %10
  %16 = load ptr, ptr @drv.10, align 8
  %17 = call i32 %16(ptr noundef nonnull %4, i64 noundef %0)
  br label %18

18:                                               ; preds = %12, %15
  %19 = phi i32 [ %14, %12 ], [ %17, %15 ]
  %20 = icmp eq i32 %19, 0
  %21 = load ptr, ptr %4, align 8
  %22 = select i1 %20, ptr %21, ptr null
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  br label %23

23:                                               ; preds = %8, %18
  %24 = phi ptr [ %22, %18 ], [ null, %8 ]
  ret ptr %24
}

define dso_local void @mgpuMemFree(ptr noundef %0, ptr noundef readnone captures(none) %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = load ptr, ptr @drv.12, align 8
  %5 = tail call i32 %4(ptr noundef %0)
  %6 = icmp eq i32 %5, 0
  br i1 %6, label %13, label %7

7:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %8 = load ptr, ptr @drv.1, align 8
  %9 = tail call ptr %8(i32 noundef range(i32 1, 0) %5)
  %10 = icmp eq ptr %9, null
  %11 = select i1 %10, ptr @.str.20, ptr %9
  %12 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.4, ptr noundef nonnull %11)
  call fastcc void @fail(ptr noundef %3)
  unreachable

13:                                               ; preds = %2
  ret void
}

define dso_local void @mgpuMemcpy(ptr noundef %0, ptr noundef %1, i64 noundef %2, ptr noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = load ptr, ptr @drv.13, align 8
  %7 = tail call i32 %6(ptr noundef %0, ptr noundef %1, i64 noundef %2, i32 noundef 4, ptr noundef %3)
  %8 = icmp eq i32 %7, 0
  br i1 %8, label %15, label %9

9:                                                ; preds = %4
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %10 = load ptr, ptr @drv.1, align 8
  %11 = tail call ptr %10(i32 noundef range(i32 1, 0) %7)
  %12 = icmp eq ptr %11, null
  %13 = select i1 %12, ptr @.str.20, ptr %11
  %14 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.5, ptr noundef nonnull %13)
  call fastcc void @fail(ptr noundef %5)
  unreachable

15:                                               ; preds = %4
  ret void
}

define dso_local void @__neuro_device_check(i32 noundef %0) local_unnamed_addr {
  %2 = alloca i32, align 4
  %3 = alloca [256 x i8], align 16
  tail call fastcc void @probe()
  %4 = load i1, ptr @ready, align 4
  br i1 %4, label %7, label %5

5:                                                ; preds = %1
  %6 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %6)
  unreachable

7:                                                ; preds = %1
  %8 = icmp eq i32 %0, 0
  br i1 %8, label %9, label %10

9:                                                ; preds = %7
  ret void

10:                                               ; preds = %7
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  store i32 0, ptr %2, align 4
  %11 = load ptr, ptr @drv.2, align 8
  %12 = call i32 %11(ptr noundef nonnull %2)
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %13 = icmp sgt i32 %0, -1
  %14 = load i32, ptr %2, align 4
  %15 = icmp slt i32 %0, %14
  %16 = select i1 %13, i1 %15, i1 false
  br i1 %16, label %19, label %17

17:                                               ; preds = %10
  %18 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef nonnull @.str.6, i32 noundef %0, i32 noundef %14)
  call fastcc void @report(ptr noundef %3, i32 noundef %18)
  unreachable

19:                                               ; preds = %10
  %20 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef nonnull @.str.7, i32 noundef %0)
  call fastcc void @report(ptr noundef %3, i32 noundef %20)
  unreachable
}

define internal fastcc void @report(ptr noundef nonnull %0, i32 noundef %1) unnamed_addr {
  %3 = tail call i32 @llvm.smax.i32(i32 %1, i32 0)
  %4 = tail call i32 @llvm.umin.i32(i32 %3, i32 255)
  %5 = zext nneg i32 %4 to i64
  tail call void @__neuro_gpu_panic(ptr noundef nonnull %0, i64 noundef %5)
  unreachable
}

declare noundef i32 @snprintf(ptr noalias noundef writeonly captures(none), i64 noundef, ptr noundef readonly captures(none), ...) local_unnamed_addr

define dso_local ptr @__neuro_device_alloc(i64 noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca ptr, align 8
  tail call fastcc void @probe()
  %4 = load i1, ptr @ready, align 4
  br i1 %4, label %7, label %5

5:                                                ; preds = %1
  %6 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %6)
  unreachable

7:                                                ; preds = %1
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %8 = tail call i64 @llvm.umax.i64(i64 %0, i64 1)
  store ptr null, ptr %3, align 8
  %9 = load ptr, ptr @drv.10, align 8
  %10 = call i32 %9(ptr noundef nonnull %3, i64 noundef %8)
  %11 = icmp eq i32 %10, 0
  br i1 %11, label %18, label %12

12:                                               ; preds = %7
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %13 = load ptr, ptr @drv.1, align 8
  %14 = call ptr %13(i32 noundef range(i32 1, 0) %10)
  %15 = icmp eq ptr %14, null
  %16 = select i1 %15, ptr @.str.20, ptr %14
  %17 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.8, ptr noundef nonnull %16)
  call fastcc void @fail(ptr noundef %2)
  unreachable

18:                                               ; preds = %7
  %19 = load ptr, ptr %3, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  ret ptr %19
}

define dso_local ptr @__neuro_device_upload(ptr noundef %0, i64 noundef %1, i32 noundef %2) local_unnamed_addr {
  %4 = alloca [128 x i8], align 16
  %5 = alloca [128 x i8], align 16
  tail call void @__neuro_device_check(i32 noundef %2)
  %6 = tail call ptr @__neuro_device_alloc(i64 noundef %1)
  %7 = tail call ptr @mgpuStreamCreate()
  %8 = load ptr, ptr @drv.13, align 8
  %9 = tail call i32 %8(ptr noundef %6, ptr noundef %0, i64 noundef %1, i32 noundef 4, ptr noundef %7)
  %10 = icmp eq i32 %9, 0
  br i1 %10, label %17, label %11

11:                                               ; preds = %3
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %12 = load ptr, ptr @drv.1, align 8
  %13 = tail call ptr %12(i32 noundef range(i32 1, 0) %9)
  %14 = icmp eq ptr %13, null
  %15 = select i1 %14, ptr @.str.20, ptr %13
  %16 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.5, ptr noundef nonnull %15)
  call fastcc void @fail(ptr noundef %5)
  unreachable

17:                                               ; preds = %3
  %18 = load ptr, ptr @drv.9, align 8
  %19 = tail call i32 %18(ptr noundef %7)
  %20 = icmp eq i32 %19, 0
  br i1 %20, label %27, label %21

21:                                               ; preds = %17
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %22 = load ptr, ptr @drv.1, align 8
  %23 = tail call ptr %22(i32 noundef range(i32 1, 0) %19)
  %24 = icmp eq ptr %23, null
  %25 = select i1 %24, ptr @.str.20, ptr %23
  %26 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.3, ptr noundef nonnull %25)
  call fastcc void @fail(ptr noundef %4)
  unreachable

27:                                               ; preds = %17
  ret ptr %6
}

define dso_local void @__neuro_device_download(ptr noundef %0, ptr noundef %1, i64 noundef %2) local_unnamed_addr {
  %4 = alloca [128 x i8], align 16
  %5 = alloca [128 x i8], align 16
  %6 = tail call ptr @mgpuStreamCreate()
  %7 = load ptr, ptr @drv.13, align 8
  %8 = tail call i32 %7(ptr noundef %0, ptr noundef %1, i64 noundef %2, i32 noundef 4, ptr noundef %6)
  %9 = icmp eq i32 %8, 0
  br i1 %9, label %16, label %10

10:                                               ; preds = %3
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %11 = load ptr, ptr @drv.1, align 8
  %12 = tail call ptr %11(i32 noundef range(i32 1, 0) %8)
  %13 = icmp eq ptr %12, null
  %14 = select i1 %13, ptr @.str.20, ptr %12
  %15 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.5, ptr noundef nonnull %14)
  call fastcc void @fail(ptr noundef %5)
  unreachable

16:                                               ; preds = %3
  %17 = load ptr, ptr @drv.9, align 8
  %18 = tail call i32 %17(ptr noundef %6)
  %19 = icmp eq i32 %18, 0
  br i1 %19, label %26, label %20

20:                                               ; preds = %16
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %21 = load ptr, ptr @drv.1, align 8
  %22 = tail call ptr %21(i32 noundef range(i32 1, 0) %18)
  %23 = icmp eq ptr %22, null
  %24 = select i1 %23, ptr @.str.20, ptr %22
  %25 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.3, ptr noundef nonnull %24)
  call fastcc void @fail(ptr noundef %4)
  unreachable

26:                                               ; preds = %16
  ret void
}

define dso_local void @__neuro_device_free(ptr noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca [128 x i8], align 16
  %4 = tail call ptr @mgpuStreamCreate()
  %5 = load ptr, ptr @drv.9, align 8
  %6 = tail call i32 %5(ptr noundef %4)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %14, label %8

8:                                                ; preds = %1
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = load ptr, ptr @drv.1, align 8
  %10 = tail call ptr %9(i32 noundef range(i32 1, 0) %6)
  %11 = icmp eq ptr %10, null
  %12 = select i1 %11, ptr @.str.20, ptr %10
  %13 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.3, ptr noundef nonnull %12)
  call fastcc void @fail(ptr noundef %3)
  unreachable

14:                                               ; preds = %1
  %15 = load ptr, ptr @drv.12, align 8
  %16 = tail call i32 %15(ptr noundef %0)
  %17 = icmp eq i32 %16, 0
  br i1 %17, label %24, label %18

18:                                               ; preds = %14
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %19 = load ptr, ptr @drv.1, align 8
  %20 = tail call ptr %19(i32 noundef range(i32 1, 0) %16)
  %21 = icmp eq ptr %20, null
  %22 = select i1 %21, ptr @.str.20, ptr %20
  %23 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull @.str.4, ptr noundef nonnull %22)
  call fastcc void @fail(ptr noundef %2)
  unreachable

24:                                               ; preds = %14
  ret void
}

declare ptr @dlopen(ptr noundef, i32 noundef) local_unnamed_addr

declare ptr @dlsym(ptr noundef, ptr noundef) local_unnamed_addr

define internal fastcc void @unusable(ptr noundef %0, ptr noundef %1) unnamed_addr {
  %3 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %4 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef nonnull @.str.22, ptr noundef %0, ptr noundef %1)
  call fastcc void @report(ptr noundef %3, i32 noundef %4)
  unreachable
}

define internal fastcc void @fail(ptr noundef nonnull %0) unnamed_addr {
  %2 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %3 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 256, ptr noundef nonnull @.str.24, ptr noundef nonnull %0)
  call fastcc void @report(ptr noundef %2, i32 noundef %3)
  unreachable
}

declare void @__neuro_gpu_panic(ptr noundef, i64 noundef) local_unnamed_addr

declare i32 @llvm.smax.i32(i32, i32)

declare i32 @llvm.umin.i32(i32, i32)

declare i64 @llvm.umax.i64(i64, i64)

declare ptr @llvm.load.relative.i64(ptr, i64)

