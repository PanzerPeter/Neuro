;
; MLIR's GPU runtime ABI (`mgpu*`) over the CUDA driver API, linked into every module
; that runs `@gpu` bodies. The CUDA driver is opened with dlopen, so a binary without a
; usable NVIDIA GPU starts, then panics from the first module load (a global
; constructor) saying why, unless every `@gpu` function has a host fallback
; (`__neuro_gpu_fallback`), in which case `__neuro_gpu_usable` sends each call to its
; host body. The `__neuro_device_*` functions move a tensor's buffer to and from the
; GPU for `.to(Device::GPU(n))`, so a program that transfers a tensor links this module
; too. Failures call `__neuro_gpu_panic`, which the LLVM backend defines as an ordinary
; runtime panic.
;
; Generated from compiler/llvm-backend/src/codegen/gpu_runtime.c with clang -O2
; -emit-llvm, then stripped of the target datalayout and triple, attribute groups and
; metadata. Regenerate it with tools/regen_gpu_runtime.sh.
;

%struct.anon = type { ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr }
%struct.anon.0 = type { ptr, ptr }

@ready = internal unnamed_addr global i1 false, align 4
@drv = internal global %struct.anon zeroinitializer, align 8
@.str = private unnamed_addr constant [20 x i8] c"cuModuleGetFunction\00", align 1
@.str.1 = private unnamed_addr constant [15 x i8] c"cuLaunchKernel\00", align 1
@shared_stream = internal global ptr null, align 8
@.str.2 = private unnamed_addr constant [15 x i8] c"cuStreamCreate\00", align 1
@.str.3 = private unnamed_addr constant [20 x i8] c"cuStreamSynchronize\00", align 1
@.str.4 = private unnamed_addr constant [10 x i8] c"cuMemFree\00", align 1
@.str.5 = private unnamed_addr constant [14 x i8] c"cuMemcpyAsync\00", align 1
@TRANSFER = internal constant [14 x i8] c"`Device::GPU`\00", align 1
@.str.6 = private unnamed_addr constant [52 x i8] c"`Device::GPU(%d)` names no GPU: this machine has %d\00", align 1
@.str.7 = private unnamed_addr constant [72 x i8] c"`Device::GPU(%d)` is not supported yet: a tensor can live on GPU 0 only\00", align 1
@.str.8 = private unnamed_addr constant [11 x i8] c"cuMemAlloc\00", align 1
@probed = internal unnamed_addr global i1 false, align 4
@NO_LIBRARY = internal constant [51 x i8] c"the CUDA driver (libcuda.so.1) could not be loaded\00", align 16
@unusable_reason = internal unnamed_addr global ptr null, align 8
@entry_points = internal unnamed_addr constant [16 x %struct.anon.0] [%struct.anon.0 { ptr @.str.10, ptr @drv }, %struct.anon.0 { ptr @.str.11, ptr getelementptr (i8, ptr @drv, i64 8) }, %struct.anon.0 { ptr @.str.12, ptr getelementptr (i8, ptr @drv, i64 16) }, %struct.anon.0 { ptr @.str.13, ptr getelementptr (i8, ptr @drv, i64 24) }, %struct.anon.0 { ptr @.str.14, ptr getelementptr (i8, ptr @drv, i64 32) }, %struct.anon.0 { ptr @.str.15, ptr getelementptr (i8, ptr @drv, i64 40) }, %struct.anon.0 { ptr @.str.16, ptr getelementptr (i8, ptr @drv, i64 48) }, %struct.anon.0 { ptr @.str.17, ptr getelementptr (i8, ptr @drv, i64 56) }, %struct.anon.0 { ptr @.str, ptr getelementptr (i8, ptr @drv, i64 64) }, %struct.anon.0 { ptr @.str.1, ptr getelementptr (i8, ptr @drv, i64 72) }, %struct.anon.0 { ptr @.str.2, ptr getelementptr (i8, ptr @drv, i64 80) }, %struct.anon.0 { ptr @.str.3, ptr getelementptr (i8, ptr @drv, i64 88) }, %struct.anon.0 { ptr @.str.18, ptr getelementptr (i8, ptr @drv, i64 96) }, %struct.anon.0 { ptr @.str.19, ptr getelementptr (i8, ptr @drv, i64 104) }, %struct.anon.0 { ptr @.str.20, ptr getelementptr (i8, ptr @drv, i64 112) }, %struct.anon.0 { ptr @.str.5, ptr getelementptr (i8, ptr @drv, i64 120) }], align 16
@TOO_OLD = internal constant [27 x i8] c"the CUDA driver is too old\00", align 16
@NO_DEVICE = internal constant [34 x i8] c"the CUDA driver reports no device\00", align 16
@.str.9 = private unnamed_addr constant [13 x i8] c"libcuda.so.1\00", align 1
@.str.10 = private unnamed_addr constant [7 x i8] c"cuInit\00", align 1
@.str.11 = private unnamed_addr constant [15 x i8] c"cuGetErrorName\00", align 1
@.str.12 = private unnamed_addr constant [17 x i8] c"cuDeviceGetCount\00", align 1
@.str.13 = private unnamed_addr constant [12 x i8] c"cuDeviceGet\00", align 1
@.str.14 = private unnamed_addr constant [25 x i8] c"cuDevicePrimaryCtxRetain\00", align 1
@.str.15 = private unnamed_addr constant [16 x i8] c"cuCtxSetCurrent\00", align 1
@.str.16 = private unnamed_addr constant [17 x i8] c"cuModuleLoadData\00", align 1
@.str.17 = private unnamed_addr constant [15 x i8] c"cuModuleUnload\00", align 1
@.str.18 = private unnamed_addr constant [14 x i8] c"cuMemAlloc_v2\00", align 1
@.str.19 = private unnamed_addr constant [18 x i8] c"cuMemAllocManaged\00", align 1
@.str.20 = private unnamed_addr constant [13 x i8] c"cuMemFree_v2\00", align 1
@.str.21 = private unnamed_addr constant [22 x i8] c"an unknown CUDA error\00", align 1
@__neuro_gpu_fallback = external local_unnamed_addr constant i8, align 1
@LAUNCH = internal constant [7 x i8] c"`@gpu`\00", align 1
@load_failure = internal global [128 x i8] zeroinitializer, align 16
@.str.22 = private unnamed_addr constant [51 x i8] c"its driver cannot load this program's kernels (%s)\00", align 1
@.str.23 = private unnamed_addr constant [47 x i8] c"%s needs an NVIDIA GPU, and none is usable: %s\00", align 1
@.str.24 = private unnamed_addr constant [18 x i8] c"%s failed with %s\00", align 1
@.str.25 = private unnamed_addr constant [14 x i8] c"GPU error: %s\00", align 1

define dso_local range(i32 0, 2) i32 @__neuro_gpu_usable() local_unnamed_addr {
  tail call fastcc void @probe()
  %1 = load i1, ptr @ready, align 4
  %2 = zext i1 %1 to i32
  ret i32 %2
}

define internal fastcc void @probe() unnamed_addr {
  %1 = alloca ptr, align 8
  %2 = alloca i32, align 4
  %3 = alloca ptr, align 8
  %4 = alloca ptr, align 8
  %5 = alloca i32, align 4
  %6 = load i1, ptr @probed, align 4
  br i1 %6, label %70, label %7

7:                                                ; preds = %0
  store i1 true, ptr @probed, align 4
  %8 = tail call ptr @dlopen(ptr noundef nonnull @.str.9, i32 noundef 2)
  %9 = icmp eq ptr %8, null
  br i1 %9, label %10, label %14

10:                                               ; preds = %7
  store ptr @NO_LIBRARY, ptr @unusable_reason, align 8
  br label %70

11:                                               ; preds = %14
  %12 = add nuw nsw i64 %15, 1
  %13 = icmp eq i64 %12, 16
  br i1 %13, label %23, label %14

14:                                               ; preds = %7, %11
  %15 = phi i64 [ %12, %11 ], [ 0, %7 ]
  %16 = getelementptr inbounds nuw %struct.anon.0, ptr @entry_points, i64 %15
  %17 = load ptr, ptr %16, align 16
  %18 = tail call ptr @dlsym(ptr noundef nonnull %8, ptr noundef %17)
  %19 = getelementptr inbounds nuw i8, ptr %16, i64 8
  %20 = load ptr, ptr %19, align 8
  store ptr %18, ptr %20, align 8
  %21 = icmp eq ptr %18, null
  br i1 %21, label %22, label %11

22:                                               ; preds = %14
  store ptr @TOO_OLD, ptr @unusable_reason, align 8
  br label %70

23:                                               ; preds = %11
  %24 = load ptr, ptr @drv, align 8
  %25 = tail call i32 %24(i32 noundef 0)
  %26 = icmp eq i32 %25, 0
  br i1 %26, label %35, label %27

27:                                               ; preds = %23
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  store ptr null, ptr %4, align 8
  %28 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 8), align 8
  %29 = call i32 %28(i32 noundef range(i32 1, 0) %25, ptr noundef nonnull %4)
  %30 = icmp ne i32 %29, 0
  %31 = load ptr, ptr %4, align 8
  %32 = icmp eq ptr %31, null
  %33 = select i1 %30, i1 true, i1 %32
  %34 = select i1 %33, ptr @.str.21, ptr %31
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  store ptr %34, ptr @unusable_reason, align 8
  br label %70

35:                                               ; preds = %23
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  store i32 0, ptr %5, align 4
  %36 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 16), align 8
  %37 = call i32 %36(ptr noundef nonnull %5)
  %38 = icmp ne i32 %37, 0
  %39 = load i32, ptr %5, align 4
  %40 = icmp eq i32 %39, 0
  %41 = select i1 %38, i1 true, i1 %40
  br i1 %41, label %42, label %43

42:                                               ; preds = %35
  store ptr @NO_DEVICE, ptr @unusable_reason, align 8
  br label %69

43:                                               ; preds = %35
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %44 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 24), align 8
  %45 = call i32 %44(ptr noundef nonnull %2, i32 noundef 0)
  %46 = icmp eq i32 %45, 0
  br i1 %46, label %47, label %52

47:                                               ; preds = %43
  %48 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 32), align 8
  %49 = load i32, ptr %2, align 4
  %50 = call i32 %48(ptr noundef nonnull %3, i32 noundef %49)
  %51 = icmp eq i32 %50, 0
  br i1 %51, label %54, label %52

52:                                               ; preds = %43, %47
  %53 = phi i32 [ %50, %47 ], [ %45, %43 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  br label %59

54:                                               ; preds = %47
  %55 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 40), align 8
  %56 = load ptr, ptr %3, align 8
  %57 = call i32 %55(ptr noundef %56)
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  %58 = icmp eq i32 %57, 0
  br i1 %58, label %68, label %59

59:                                               ; preds = %52, %54
  %60 = phi i32 [ %53, %52 ], [ %57, %54 ]
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  store ptr null, ptr %1, align 8
  %61 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 8), align 8
  %62 = call i32 %61(i32 noundef range(i32 1, 0) %60, ptr noundef nonnull %1)
  %63 = icmp ne i32 %62, 0
  %64 = load ptr, ptr %1, align 8
  %65 = icmp eq ptr %64, null
  %66 = select i1 %63, i1 true, i1 %65
  %67 = select i1 %66, ptr @.str.21, ptr %64
  call void @llvm.lifetime.end.p0(ptr nonnull %1)
  store ptr %67, ptr @unusable_reason, align 8
  br label %69

68:                                               ; preds = %54
  store i1 true, ptr @ready, align 4
  br label %69

69:                                               ; preds = %68, %59, %42
  call void @llvm.lifetime.end.p0(ptr nonnull %5)
  br label %70

70:                                               ; preds = %22, %10, %69, %27, %0
  ret void
}

define dso_local ptr @mgpuModuleLoad(ptr noundef %0, i64 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define internal fastcc ptr @load(ptr noundef %0) unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca ptr, align 8
  %4 = alloca ptr, align 8
  tail call fastcc void @probe()
  %5 = load i1, ptr @ready, align 4
  %6 = load i8, ptr @__neuro_gpu_fallback, align 1
  %7 = icmp eq i8 %6, 0
  %8 = select i1 %5, i1 true, i1 %7
  br i1 %8, label %9, label %36

9:                                                ; preds = %1
  br i1 %5, label %12, label %10

10:                                               ; preds = %9
  %11 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %11)
  unreachable

12:                                               ; preds = %9
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  store ptr null, ptr %4, align 8
  %13 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 48), align 8
  %14 = call i32 %13(ptr noundef nonnull %4, ptr noundef %0)
  %15 = icmp ne i32 %14, 0
  %16 = icmp ne i8 %6, 0
  %17 = select i1 %15, i1 %16, i1 false
  br i1 %17, label %18, label %27

18:                                               ; preds = %12
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  store ptr null, ptr %3, align 8
  %19 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 8), align 8
  %20 = call i32 %19(i32 noundef range(i32 1, 0) %14, ptr noundef nonnull %3)
  %21 = icmp ne i32 %20, 0
  %22 = load ptr, ptr %3, align 8
  %23 = icmp eq ptr %22, null
  %24 = select i1 %21, i1 true, i1 %23
  %25 = select i1 %24, ptr @.str.21, ptr %22
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  %26 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) @load_failure, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef %25)
  store ptr @load_failure, ptr @unusable_reason, align 8
  store i1 false, ptr @ready, align 4
  br label %34

27:                                               ; preds = %12
  %28 = icmp eq i32 %14, 0
  br i1 %28, label %32, label %29

29:                                               ; preds = %27
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %30 = call fastcc ptr @error_name(i32 noundef %14)
  %31 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.16, ptr noundef %30)
  call fastcc void @fail(ptr noundef %2)
  unreachable

32:                                               ; preds = %27
  %33 = load ptr, ptr %4, align 8
  br label %34

34:                                               ; preds = %32, %18
  %35 = phi ptr [ null, %18 ], [ %33, %32 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  br label %36

36:                                               ; preds = %1, %34
  %37 = phi ptr [ %35, %34 ], [ null, %1 ]
  ret ptr %37
}

define dso_local ptr @mgpuModuleLoadJIT(ptr noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define dso_local void @mgpuModuleUnload(ptr noundef %0) local_unnamed_addr {
  %2 = load i1, ptr @ready, align 4
  br i1 %2, label %3, label %6

3:                                                ; preds = %1
  %4 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 56), align 8
  %5 = tail call i32 %4(ptr noundef %0)
  br label %6

6:                                                ; preds = %3, %1
  ret void
}

define dso_local ptr @mgpuModuleGetFunction(ptr noundef %0, ptr noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca ptr, align 8
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %5 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 64), align 8
  %6 = call i32 %5(ptr noundef nonnull %4, ptr noundef %0, ptr noundef %1)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str, ptr noundef %9)
  call fastcc void @fail(ptr noundef %3)
  unreachable

11:                                               ; preds = %2
  %12 = load ptr, ptr %4, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  ret ptr %12
}

declare void @llvm.lifetime.start.p0(ptr captures(none))

declare void @llvm.lifetime.end.p0(ptr captures(none))

define dso_local void @mgpuLaunchKernel(ptr noundef %0, i64 noundef %1, i64 noundef %2, i64 noundef %3, i64 noundef %4, i64 noundef %5, i64 noundef %6, i32 noundef %7, ptr noundef %8, ptr noundef %9, ptr noundef %10, i64 noundef %11) local_unnamed_addr {
  %13 = alloca [128 x i8], align 16
  %14 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 72), align 8
  %15 = trunc i64 %1 to i32
  %16 = trunc i64 %2 to i32
  %17 = trunc i64 %3 to i32
  %18 = trunc i64 %4 to i32
  %19 = trunc i64 %5 to i32
  %20 = trunc i64 %6 to i32
  %21 = tail call i32 %14(ptr noundef %0, i32 noundef %15, i32 noundef %16, i32 noundef %17, i32 noundef %18, i32 noundef %19, i32 noundef %20, i32 noundef %7, ptr noundef %8, ptr noundef %9, ptr noundef %10)
  %22 = icmp eq i32 %21, 0
  br i1 %22, label %26, label %23

23:                                               ; preds = %12
  call void @llvm.lifetime.start.p0(ptr nonnull %13)
  %24 = tail call fastcc ptr @error_name(i32 noundef %21)
  %25 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %13, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.1, ptr noundef %24)
  call fastcc void @fail(ptr noundef %13)
  unreachable

26:                                               ; preds = %12
  ret void
}

define dso_local ptr @mgpuStreamCreate() local_unnamed_addr {
  %1 = alloca [128 x i8], align 16
  %2 = load ptr, ptr @shared_stream, align 8
  %3 = icmp eq ptr %2, null
  br i1 %3, label %4, label %17

4:                                                ; preds = %0
  tail call fastcc void @probe()
  %5 = load i1, ptr @ready, align 4
  br i1 %5, label %8, label %6

6:                                                ; preds = %4
  %7 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %7)
  unreachable

8:                                                ; preds = %4
  %9 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 80), align 8
  %10 = tail call i32 %9(ptr noundef nonnull @shared_stream, i32 noundef 1)
  %11 = icmp eq i32 %10, 0
  br i1 %11, label %12, label %14

12:                                               ; preds = %8
  %13 = load ptr, ptr @shared_stream, align 8
  br label %17

14:                                               ; preds = %8
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  %15 = tail call fastcc ptr @error_name(i32 noundef %10)
  %16 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %1, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.2, ptr noundef %15)
  call fastcc void @fail(ptr noundef %1)
  unreachable

17:                                               ; preds = %12, %0
  %18 = phi ptr [ %13, %12 ], [ %2, %0 ]
  ret ptr %18
}

define dso_local void @mgpuStreamSynchronize(ptr noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %4 = tail call i32 %3(ptr noundef %0)
  %5 = icmp eq i32 %4, 0
  br i1 %5, label %9, label %6

6:                                                ; preds = %1
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %7 = tail call fastcc ptr @error_name(i32 noundef %4)
  %8 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.3, ptr noundef %7)
  call fastcc void @fail(ptr noundef %2)
  unreachable

9:                                                ; preds = %1
  ret void
}

define dso_local void @mgpuStreamDestroy(ptr noundef readnone captures(none) %0) local_unnamed_addr {
  ret void
}

define dso_local ptr @mgpuMemAlloc(i64 noundef %0, ptr noundef readnone captures(none) %1, i8 noundef zeroext %2) local_unnamed_addr {
  %4 = alloca i64, align 8
  tail call fastcc void @probe()
  %5 = load i1, ptr @ready, align 4
  br i1 %5, label %8, label %6

6:                                                ; preds = %3
  %7 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %7)
  unreachable

8:                                                ; preds = %3
  %9 = icmp eq i64 %0, 0
  br i1 %9, label %24, label %10

10:                                               ; preds = %8
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  store i64 0, ptr %4, align 8
  %11 = icmp eq i8 %2, 0
  br i1 %11, label %15, label %12

12:                                               ; preds = %10
  %13 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 104), align 8
  %14 = call i32 %13(ptr noundef nonnull %4, i64 noundef %0, i32 noundef 1)
  br label %18

15:                                               ; preds = %10
  %16 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 96), align 8
  %17 = call i32 %16(ptr noundef nonnull %4, i64 noundef %0)
  br label %18

18:                                               ; preds = %12, %15
  %19 = phi i32 [ %14, %12 ], [ %17, %15 ]
  %20 = load i64, ptr %4, align 8
  %21 = inttoptr i64 %20 to ptr
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  %22 = icmp eq i32 %19, 0
  %23 = select i1 %22, ptr %21, ptr null
  br label %24

24:                                               ; preds = %8, %18
  %25 = phi ptr [ %23, %18 ], [ null, %8 ]
  ret ptr %25
}

define dso_local void @mgpuMemFree(ptr noundef %0, ptr noundef readnone captures(none) %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 112), align 8
  %5 = ptrtoint ptr %0 to i64
  %6 = tail call i32 %4(i64 noundef %5)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = tail call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.4, ptr noundef %9)
  call fastcc void @fail(ptr noundef %3)
  unreachable

11:                                               ; preds = %2
  ret void
}

define dso_local void @mgpuMemcpy(ptr noundef %0, ptr noundef %1, i64 noundef %2, ptr noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 120), align 8
  %7 = ptrtoint ptr %0 to i64
  %8 = ptrtoint ptr %1 to i64
  %9 = tail call i32 %6(i64 noundef %7, i64 noundef %8, i64 noundef %2, ptr noundef %3)
  %10 = icmp eq i32 %9, 0
  br i1 %10, label %14, label %11

11:                                               ; preds = %4
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %12 = tail call fastcc ptr @error_name(i32 noundef %9)
  %13 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.5, ptr noundef %12)
  call fastcc void @fail(ptr noundef %5)
  unreachable

14:                                               ; preds = %4
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
  %11 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 16), align 8
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
  %3 = alloca i64, align 8
  tail call fastcc void @probe()
  %4 = load i1, ptr @ready, align 4
  br i1 %4, label %7, label %5

5:                                                ; preds = %1
  %6 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %6)
  unreachable

7:                                                ; preds = %1
  %8 = tail call i64 @llvm.umax.i64(i64 %0, i64 1)
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  store i64 0, ptr %3, align 8
  %9 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 96), align 8
  %10 = call i32 %9(ptr noundef nonnull %3, i64 noundef %8)
  %11 = load i64, ptr %3, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  %12 = icmp eq i32 %10, 0
  br i1 %12, label %16, label %13

13:                                               ; preds = %7
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %14 = call fastcc ptr @error_name(i32 noundef %10)
  %15 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.8, ptr noundef %14)
  call fastcc void @fail(ptr noundef %2)
  unreachable

16:                                               ; preds = %7
  %17 = inttoptr i64 %11 to ptr
  ret ptr %17
}

define dso_local ptr @__neuro_device_upload(ptr noundef %0, i64 noundef %1, i32 noundef %2) local_unnamed_addr {
  %4 = alloca [128 x i8], align 16
  %5 = alloca [128 x i8], align 16
  tail call void @__neuro_device_check(i32 noundef %2)
  %6 = tail call ptr @__neuro_device_alloc(i64 noundef %1)
  %7 = tail call ptr @mgpuStreamCreate()
  %8 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 120), align 8
  %9 = ptrtoint ptr %6 to i64
  %10 = ptrtoint ptr %0 to i64
  %11 = tail call i32 %8(i64 noundef %9, i64 noundef %10, i64 noundef %1, ptr noundef %7)
  %12 = icmp eq i32 %11, 0
  br i1 %12, label %16, label %13

13:                                               ; preds = %3
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %14 = tail call fastcc ptr @error_name(i32 noundef %11)
  %15 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.5, ptr noundef %14)
  call fastcc void @fail(ptr noundef %5)
  unreachable

16:                                               ; preds = %3
  %17 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %18 = tail call i32 %17(ptr noundef %7)
  %19 = icmp eq i32 %18, 0
  br i1 %19, label %23, label %20

20:                                               ; preds = %16
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %21 = tail call fastcc ptr @error_name(i32 noundef %18)
  %22 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.3, ptr noundef %21)
  call fastcc void @fail(ptr noundef %4)
  unreachable

23:                                               ; preds = %16
  ret ptr %6
}

define dso_local void @__neuro_device_download(ptr noundef %0, ptr noundef %1, i64 noundef %2) local_unnamed_addr {
  %4 = alloca [128 x i8], align 16
  %5 = alloca [128 x i8], align 16
  %6 = tail call ptr @mgpuStreamCreate()
  %7 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 120), align 8
  %8 = ptrtoint ptr %0 to i64
  %9 = ptrtoint ptr %1 to i64
  %10 = tail call i32 %7(i64 noundef %8, i64 noundef %9, i64 noundef %2, ptr noundef %6)
  %11 = icmp eq i32 %10, 0
  br i1 %11, label %15, label %12

12:                                               ; preds = %3
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %13 = tail call fastcc ptr @error_name(i32 noundef %10)
  %14 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.5, ptr noundef %13)
  call fastcc void @fail(ptr noundef %5)
  unreachable

15:                                               ; preds = %3
  %16 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %17 = tail call i32 %16(ptr noundef %6)
  %18 = icmp eq i32 %17, 0
  br i1 %18, label %22, label %19

19:                                               ; preds = %15
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %20 = tail call fastcc ptr @error_name(i32 noundef %17)
  %21 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.3, ptr noundef %20)
  call fastcc void @fail(ptr noundef %4)
  unreachable

22:                                               ; preds = %15
  ret void
}

define dso_local void @__neuro_device_free(ptr noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca [128 x i8], align 16
  %4 = tail call ptr @mgpuStreamCreate()
  %5 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %6 = tail call i32 %5(ptr noundef %4)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %1
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = tail call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.3, ptr noundef %9)
  call fastcc void @fail(ptr noundef %3)
  unreachable

11:                                               ; preds = %1
  %12 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 112), align 8
  %13 = ptrtoint ptr %0 to i64
  %14 = tail call i32 %12(i64 noundef %13)
  %15 = icmp eq i32 %14, 0
  br i1 %15, label %19, label %16

16:                                               ; preds = %11
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %17 = tail call fastcc ptr @error_name(i32 noundef %14)
  %18 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef nonnull @.str.4, ptr noundef %17)
  call fastcc void @fail(ptr noundef %2)
  unreachable

19:                                               ; preds = %11
  ret void
}

declare ptr @dlopen(ptr noundef, i32 noundef) local_unnamed_addr

declare ptr @dlsym(ptr noundef, ptr noundef) local_unnamed_addr

define internal fastcc ptr @error_name(i32 noundef range(i32 1, 0) %0) unnamed_addr {
  %2 = alloca ptr, align 8
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  store ptr null, ptr %2, align 8
  %3 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 8), align 8
  %4 = call i32 %3(i32 noundef %0, ptr noundef nonnull %2)
  %5 = icmp ne i32 %4, 0
  %6 = load ptr, ptr %2, align 8
  %7 = icmp eq ptr %6, null
  %8 = select i1 %5, i1 true, i1 %7
  %9 = select i1 %8, ptr @.str.21, ptr %6
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  ret ptr %9
}

define internal fastcc void @unusable(ptr noundef %0, ptr noundef %1) unnamed_addr {
  %3 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %4 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef nonnull @.str.23, ptr noundef %0, ptr noundef %1)
  call fastcc void @report(ptr noundef %3, i32 noundef %4)
  unreachable
}

define internal fastcc void @fail(ptr noundef nonnull %0) unnamed_addr {
  %2 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %3 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 256, ptr noundef nonnull @.str.25, ptr noundef nonnull %0)
  call fastcc void @report(ptr noundef %2, i32 noundef %3)
  unreachable
}

declare void @__neuro_gpu_panic(ptr noundef, i64 noundef) local_unnamed_addr

declare i32 @llvm.smax.i32(i32, i32)

declare i32 @llvm.umin.i32(i32, i32)

declare i64 @llvm.umax.i64(i64, i64)

