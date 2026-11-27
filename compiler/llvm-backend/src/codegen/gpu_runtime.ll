;
; MLIR's GPU runtime ABI (`mgpu*`) over the CUDA driver API, linked into every module
; that runs `@gpu` bodies. The CUDA driver is opened with dlopen, so a binary without a
; usable NVIDIA GPU starts, then panics from the first module load (a global
; constructor) saying why, unless every `@gpu` function has a host fallback
; (`__neuro_gpu_fallback`), in which case `__neuro_gpu_usable` sends each call to its
; host body. The `__neuro_device_*` functions move a tensor's buffer to and from the
; GPU for `.to(Device::GPU(n))`, so a program that transfers a tensor links this module
; too. It keeps a context, stream, device arena (`_mlir_memref_to_llvm_alloc` / `_free`)
; and copy of each kernel module per device, acting on the current one. Failures call
; `__neuro_gpu_panic`, which the LLVM backend defines as an ordinary runtime panic.
;
; Generated from compiler/llvm-backend/src/codegen/gpu_runtime.c with clang -O2
; -emit-llvm, then stripped of the target datalayout and triple, attribute groups and
; metadata. Regenerate it with tools/regen_gpu_runtime.sh.
;

%struct.anon = type { ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr }
%struct.device = type { ptr, ptr, i64 }
%struct.anon.0 = type { ptr, ptr }

@ready = internal unnamed_addr global i1 false, align 4
@current = internal unnamed_addr global i32 0, align 4
@.str = private unnamed_addr constant [16 x i8] c"cuCtxSetCurrent\00", align 1
@.str.1 = private unnamed_addr constant [109 x i8] c"a `@gpu` call's operands live on GPU %d and GPU %d: move them to one device with `.to(Device::GPU(n))` first\00", align 1
@device_count = internal global i32 0, align 4
@drv = internal global %struct.anon zeroinitializer, align 8
@.str.2 = private unnamed_addr constant [17 x i8] c"cuModuleLoadData\00", align 1
@.str.3 = private unnamed_addr constant [20 x i8] c"cuModuleGetFunction\00", align 1
@.str.4 = private unnamed_addr constant [15 x i8] c"cuLaunchKernel\00", align 1
@devices = internal global [64 x %struct.device] zeroinitializer, align 16
@.str.5 = private unnamed_addr constant [15 x i8] c"cuStreamCreate\00", align 1
@.str.6 = private unnamed_addr constant [20 x i8] c"cuStreamSynchronize\00", align 1
@.str.7 = private unnamed_addr constant [10 x i8] c"cuMemFree\00", align 1
@.str.8 = private unnamed_addr constant [14 x i8] c"cuMemcpyAsync\00", align 1
@ALLOCATION_FAILED = internal constant [77 x i8] c"device memory allocation failed: no GPU is available, or it is out of memory\00", align 16
@TRANSFER = internal constant [14 x i8] c"`Device::GPU`\00", align 1
@.str.9 = private unnamed_addr constant [52 x i8] c"`Device::GPU(%d)` names no GPU: this machine has %d\00", align 1
@.str.10 = private unnamed_addr constant [11 x i8] c"cuMemAlloc\00", align 1
@probed = internal unnamed_addr global i1 false, align 4
@NO_LIBRARY = internal constant [51 x i8] c"the CUDA driver (libcuda.so.1) could not be loaded\00", align 16
@unusable_reason = internal unnamed_addr global ptr null, align 8
@entry_points = internal unnamed_addr constant [16 x %struct.anon.0] [%struct.anon.0 { ptr @.str.12, ptr @drv }, %struct.anon.0 { ptr @.str.13, ptr getelementptr (i8, ptr @drv, i64 8) }, %struct.anon.0 { ptr @.str.14, ptr getelementptr (i8, ptr @drv, i64 16) }, %struct.anon.0 { ptr @.str.15, ptr getelementptr (i8, ptr @drv, i64 24) }, %struct.anon.0 { ptr @.str.16, ptr getelementptr (i8, ptr @drv, i64 32) }, %struct.anon.0 { ptr @.str, ptr getelementptr (i8, ptr @drv, i64 40) }, %struct.anon.0 { ptr @.str.2, ptr getelementptr (i8, ptr @drv, i64 48) }, %struct.anon.0 { ptr @.str.17, ptr getelementptr (i8, ptr @drv, i64 56) }, %struct.anon.0 { ptr @.str.3, ptr getelementptr (i8, ptr @drv, i64 64) }, %struct.anon.0 { ptr @.str.4, ptr getelementptr (i8, ptr @drv, i64 72) }, %struct.anon.0 { ptr @.str.5, ptr getelementptr (i8, ptr @drv, i64 80) }, %struct.anon.0 { ptr @.str.6, ptr getelementptr (i8, ptr @drv, i64 88) }, %struct.anon.0 { ptr @.str.18, ptr getelementptr (i8, ptr @drv, i64 96) }, %struct.anon.0 { ptr @.str.19, ptr getelementptr (i8, ptr @drv, i64 104) }, %struct.anon.0 { ptr @.str.20, ptr getelementptr (i8, ptr @drv, i64 112) }, %struct.anon.0 { ptr @.str.8, ptr getelementptr (i8, ptr @drv, i64 120) }], align 16
@TOO_OLD = internal constant [27 x i8] c"the CUDA driver is too old\00", align 16
@NO_DEVICE = internal constant [34 x i8] c"the CUDA driver reports no device\00", align 16
@.str.11 = private unnamed_addr constant [13 x i8] c"libcuda.so.1\00", align 1
@.str.12 = private unnamed_addr constant [7 x i8] c"cuInit\00", align 1
@.str.13 = private unnamed_addr constant [15 x i8] c"cuGetErrorName\00", align 1
@.str.14 = private unnamed_addr constant [17 x i8] c"cuDeviceGetCount\00", align 1
@.str.15 = private unnamed_addr constant [12 x i8] c"cuDeviceGet\00", align 1
@.str.16 = private unnamed_addr constant [25 x i8] c"cuDevicePrimaryCtxRetain\00", align 1
@.str.17 = private unnamed_addr constant [15 x i8] c"cuModuleUnload\00", align 1
@.str.18 = private unnamed_addr constant [14 x i8] c"cuMemAlloc_v2\00", align 1
@.str.19 = private unnamed_addr constant [18 x i8] c"cuMemAllocManaged\00", align 1
@.str.20 = private unnamed_addr constant [13 x i8] c"cuMemFree_v2\00", align 1
@.str.21 = private unnamed_addr constant [22 x i8] c"an unknown CUDA error\00", align 1
@__neuro_gpu_launcher = external constant [0 x i8], align 1
@.str.22 = private unnamed_addr constant [18 x i8] c"%s failed with %s\00", align 1
@.str.23 = private unnamed_addr constant [14 x i8] c"GPU error: %s\00", align 1
@contexts = internal global [64 x ptr] zeroinitializer, align 16
@__neuro_gpu_fallback = external local_unnamed_addr constant i8, align 1
@load_failure = internal global [128 x i8] zeroinitializer, align 16
@.str.24 = private unnamed_addr constant [51 x i8] c"its driver cannot load this program's kernels (%s)\00", align 1
@.str.25 = private unnamed_addr constant [46 x i8] c"no host memory left to record a kernel module\00", align 1
@.str.26 = private unnamed_addr constant [47 x i8] c"%s needs an NVIDIA GPU, and none is usable: %s\00", align 1

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
  %4 = load i1, ptr @probed, align 4
  br i1 %4, label %75, label %5

5:                                                ; preds = %0
  store i1 true, ptr @probed, align 4
  %6 = tail call ptr @dlopen(ptr noundef nonnull @.str.11, i32 noundef 2)
  %7 = icmp eq ptr %6, null
  br i1 %7, label %8, label %12

8:                                                ; preds = %5
  store ptr @NO_LIBRARY, ptr @unusable_reason, align 8
  br label %75

9:                                                ; preds = %12
  %10 = add nuw nsw i64 %13, 1
  %11 = icmp eq i64 %10, 16
  br i1 %11, label %21, label %12

12:                                               ; preds = %5, %9
  %13 = phi i64 [ %10, %9 ], [ 0, %5 ]
  %14 = getelementptr inbounds nuw [16 x i8], ptr @entry_points, i64 %13
  %15 = load ptr, ptr %14, align 16
  %16 = tail call ptr @dlsym(ptr noundef nonnull %6, ptr noundef %15)
  %17 = getelementptr inbounds nuw i8, ptr %14, i64 8
  %18 = load ptr, ptr %17, align 8
  store ptr %16, ptr %18, align 8
  %19 = icmp eq ptr %16, null
  br i1 %19, label %20, label %9

20:                                               ; preds = %12
  store ptr @TOO_OLD, ptr @unusable_reason, align 8
  br label %75

21:                                               ; preds = %9
  %22 = load ptr, ptr @drv, align 8
  %23 = tail call i32 %22(i32 noundef 0)
  %24 = icmp eq i32 %23, 0
  br i1 %24, label %33, label %25

25:                                               ; preds = %21
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  store ptr null, ptr %3, align 8
  %26 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 8), align 8
  %27 = call i32 %26(i32 noundef range(i32 1, 0) %23, ptr noundef nonnull %3)
  %28 = icmp ne i32 %27, 0
  %29 = load ptr, ptr %3, align 8
  %30 = icmp eq ptr %29, null
  %31 = select i1 %28, i1 true, i1 %30
  %32 = select i1 %31, ptr @.str.21, ptr %29
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  store ptr %32, ptr @unusable_reason, align 8
  br label %75

33:                                               ; preds = %21
  %34 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 16), align 8
  %35 = tail call i32 %34(ptr noundef nonnull @device_count)
  %36 = icmp ne i32 %35, 0
  %37 = load i32, ptr @device_count, align 4
  %38 = icmp slt i32 %37, 1
  %39 = select i1 %36, i1 true, i1 %38
  br i1 %39, label %40, label %41

40:                                               ; preds = %33
  store ptr @NO_DEVICE, ptr @unusable_reason, align 8
  br label %75

41:                                               ; preds = %33
  %42 = icmp samesign ugt i32 %37, 64
  br i1 %42, label %43, label %44

43:                                               ; preds = %41
  store i32 64, ptr @device_count, align 4
  br label %44

44:                                               ; preds = %43, %41
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %45 = load ptr, ptr @contexts, align 16
  %46 = icmp eq ptr %45, null
  br i1 %46, label %47, label %60

47:                                               ; preds = %44
  %48 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 24), align 8
  %49 = call i32 %48(ptr noundef nonnull %2, i32 noundef 0)
  %50 = icmp eq i32 %49, 0
  br i1 %50, label %51, label %58

51:                                               ; preds = %47
  %52 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 32), align 8
  %53 = load i32, ptr %2, align 4
  %54 = call i32 %52(ptr noundef nonnull @contexts, i32 noundef %53)
  %55 = icmp eq i32 %54, 0
  br i1 %55, label %56, label %58

56:                                               ; preds = %51
  %57 = load ptr, ptr @contexts, align 16
  br label %60

58:                                               ; preds = %47, %51
  %59 = phi i32 [ %54, %51 ], [ %49, %47 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  br label %65

60:                                               ; preds = %44, %56
  %61 = phi ptr [ %57, %56 ], [ %45, %44 ]
  %62 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 40), align 8
  %63 = call i32 %62(ptr noundef %61)
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  %64 = icmp eq i32 %63, 0
  br i1 %64, label %74, label %65

65:                                               ; preds = %58, %60
  %66 = phi i32 [ %59, %58 ], [ %63, %60 ]
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  store ptr null, ptr %1, align 8
  %67 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 8), align 8
  %68 = call i32 %67(i32 noundef range(i32 1, 0) %66, ptr noundef nonnull %1)
  %69 = icmp ne i32 %68, 0
  %70 = load ptr, ptr %1, align 8
  %71 = icmp eq ptr %70, null
  %72 = select i1 %69, i1 true, i1 %71
  %73 = select i1 %72, ptr @.str.21, ptr %70
  call void @llvm.lifetime.end.p0(ptr nonnull %1)
  store ptr %73, ptr @unusable_reason, align 8
  br label %75

74:                                               ; preds = %60
  store i1 true, ptr @ready, align 4
  br label %75

75:                                               ; preds = %20, %8, %74, %65, %40, %25, %0
  ret void
}

define dso_local i32 @__neuro_device_switch(i32 noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca i32, align 4
  %4 = load i32, ptr @current, align 4
  %5 = tail call i32 @llvm.smax.i32(i32 %0, i32 0)
  %6 = icmp eq i32 %5, %4
  br i1 %6, label %39, label %7

7:                                                ; preds = %1
  tail call fastcc void @probe()
  %8 = load i1, ptr @ready, align 4
  br i1 %8, label %11, label %9

9:                                                ; preds = %7
  %10 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @__neuro_gpu_launcher, ptr noundef %10)
  unreachable

11:                                               ; preds = %7
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %12 = zext nneg i32 %5 to i64
  %13 = getelementptr inbounds nuw [8 x i8], ptr @contexts, i64 %12
  %14 = load ptr, ptr %13, align 8
  %15 = icmp eq ptr %14, null
  br i1 %15, label %16, label %29

16:                                               ; preds = %11
  %17 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 24), align 8
  %18 = call i32 %17(ptr noundef nonnull %3, i32 noundef %5)
  %19 = icmp eq i32 %18, 0
  br i1 %19, label %20, label %27

20:                                               ; preds = %16
  %21 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 32), align 8
  %22 = load i32, ptr %3, align 4
  %23 = call i32 %21(ptr noundef nonnull %13, i32 noundef %22)
  %24 = icmp eq i32 %23, 0
  br i1 %24, label %25, label %27

25:                                               ; preds = %20
  %26 = load ptr, ptr %13, align 8
  br label %29

27:                                               ; preds = %16, %20
  %28 = phi i32 [ %23, %20 ], [ %18, %16 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  br label %34

29:                                               ; preds = %11, %25
  %30 = phi ptr [ %26, %25 ], [ %14, %11 ]
  %31 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 40), align 8
  %32 = call i32 %31(ptr noundef %30)
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  %33 = icmp eq i32 %32, 0
  br i1 %33, label %38, label %34

34:                                               ; preds = %27, %29
  %35 = phi i32 [ %28, %27 ], [ %32, %29 ]
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %36 = call fastcc ptr @error_name(i32 noundef %35)
  %37 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str, ptr noundef %36)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

38:                                               ; preds = %29
  store i32 %5, ptr @current, align 4
  br label %39

39:                                               ; preds = %38, %1
  ret i32 %4
}

declare void @llvm.lifetime.start.p0(ptr captures(none))

declare void @llvm.lifetime.end.p0(ptr captures(none))

define dso_local noundef i32 @__neuro_device_join(i32 noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = alloca [256 x i8], align 16
  %4 = or i32 %1, %0
  %5 = icmp slt i32 %4, 0
  %6 = icmp eq i32 %1, %0
  %7 = or i1 %6, %5
  br i1 %7, label %8, label %11

8:                                                ; preds = %2
  %9 = icmp slt i32 %0, 0
  %10 = select i1 %9, i32 %1, i32 %0
  ret i32 %10

11:                                               ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %12 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef nonnull @.str.1, i32 noundef %0, i32 noundef %1)
  call fastcc void @report(ptr noundef nonnull %3, i32 noundef %12)
  unreachable
}

define internal fastcc void @report(ptr noundef %0, i32 noundef %1) unnamed_addr {
  %3 = tail call i32 @llvm.smax.i32(i32 %1, i32 0)
  %4 = tail call i32 @llvm.umin.i32(i32 %3, i32 255)
  %5 = zext nneg i32 %4 to i64
  tail call void @__neuro_gpu_panic(ptr noundef %0, i64 noundef %5)
  unreachable
}

declare noundef i32 @snprintf(ptr noalias noundef writeonly captures(none), i64 noundef, ptr noundef readonly captures(none), ...) local_unnamed_addr

define dso_local noundef ptr @mgpuModuleLoad(ptr noundef %0, i64 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define internal fastcc noundef ptr @load(ptr noundef %0) unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca ptr, align 8
  %4 = alloca ptr, align 8
  tail call fastcc void @probe()
  %5 = load i1, ptr @ready, align 4
  %6 = load i8, ptr @__neuro_gpu_fallback, align 1
  %7 = icmp eq i8 %6, 0
  %8 = select i1 %5, i1 true, i1 %7
  br i1 %8, label %9, label %44

9:                                                ; preds = %1
  br i1 %5, label %12, label %10

10:                                               ; preds = %9
  %11 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @__neuro_gpu_launcher, ptr noundef %11)
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
  %26 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) @load_failure, i64 noundef 128, ptr noundef nonnull @.str.24, ptr noundef %25)
  store ptr @load_failure, ptr @unusable_reason, align 8
  store i1 false, ptr @ready, align 4
  br label %42

27:                                               ; preds = %12
  %28 = icmp eq i32 %14, 0
  br i1 %28, label %32, label %29

29:                                               ; preds = %27
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %30 = call fastcc ptr @error_name(i32 noundef %14)
  %31 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.2, ptr noundef %30)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

32:                                               ; preds = %27
  %33 = call noalias dereferenceable_or_null(520) ptr @calloc(i64 noundef 1, i64 noundef 520)
  %34 = icmp eq ptr %33, null
  br i1 %34, label %35, label %36

35:                                               ; preds = %32
  call fastcc void @fail(ptr noundef nonnull @.str.25)
  unreachable

36:                                               ; preds = %32
  store ptr %0, ptr %33, align 8
  %37 = load ptr, ptr %4, align 8
  %38 = getelementptr inbounds nuw i8, ptr %33, i64 8
  %39 = load i32, ptr @current, align 4
  %40 = sext i32 %39 to i64
  %41 = getelementptr inbounds [8 x i8], ptr %38, i64 %40
  store ptr %37, ptr %41, align 8
  br label %42

42:                                               ; preds = %36, %18
  %43 = phi ptr [ null, %18 ], [ %33, %36 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  br label %44

44:                                               ; preds = %1, %42
  %45 = phi ptr [ %43, %42 ], [ null, %1 ]
  ret ptr %45
}

define dso_local noundef ptr @mgpuModuleLoadJIT(ptr noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define dso_local void @mgpuModuleUnload(ptr noundef captures(address_is_null) %0) local_unnamed_addr {
  %2 = alloca i32, align 4
  %3 = alloca i32, align 4
  %4 = icmp ne ptr %0, null
  %5 = load i1, ptr @ready, align 4
  %6 = select i1 %4, i1 %5, i1 false
  br i1 %6, label %7, label %70

7:                                                ; preds = %1
  %8 = load i32, ptr @device_count, align 4
  %9 = icmp sgt i32 %8, 0
  br i1 %9, label %10, label %12

10:                                               ; preds = %7
  %11 = getelementptr inbounds nuw i8, ptr %0, i64 8
  br label %34

12:                                               ; preds = %65, %7
  %13 = load i32, ptr @current, align 4
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %14 = sext i32 %13 to i64
  %15 = getelementptr inbounds [8 x i8], ptr @contexts, i64 %14
  %16 = load ptr, ptr %15, align 8
  %17 = icmp eq ptr %16, null
  br i1 %17, label %18, label %29

18:                                               ; preds = %12
  %19 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 24), align 8
  %20 = call i32 %19(ptr noundef nonnull %3, i32 noundef %13)
  %21 = icmp eq i32 %20, 0
  br i1 %21, label %22, label %33

22:                                               ; preds = %18
  %23 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 32), align 8
  %24 = load i32, ptr %3, align 4
  %25 = call i32 %23(ptr noundef nonnull %15, i32 noundef %24)
  %26 = icmp eq i32 %25, 0
  br i1 %26, label %27, label %33

27:                                               ; preds = %22
  %28 = load ptr, ptr %15, align 8
  br label %29

29:                                               ; preds = %27, %12
  %30 = phi ptr [ %28, %27 ], [ %16, %12 ]
  %31 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 40), align 8
  %32 = call i32 %31(ptr noundef %30)
  br label %33

33:                                               ; preds = %18, %22, %29
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  call void @free(ptr noundef nonnull %0)
  br label %70

34:                                               ; preds = %10, %65
  %35 = phi i64 [ 0, %10 ], [ %66, %65 ]
  %36 = getelementptr inbounds nuw [8 x i8], ptr %11, i64 %35
  %37 = load ptr, ptr %36, align 8
  %38 = icmp eq ptr %37, null
  br i1 %38, label %65, label %39

39:                                               ; preds = %34
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %40 = getelementptr inbounds nuw [8 x i8], ptr @contexts, i64 %35
  %41 = load ptr, ptr %40, align 8
  %42 = icmp eq ptr %41, null
  br i1 %42, label %43, label %56

43:                                               ; preds = %39
  %44 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 24), align 8
  %45 = trunc nuw nsw i64 %35 to i32
  %46 = call i32 %44(ptr noundef nonnull %2, i32 noundef %45)
  %47 = icmp eq i32 %46, 0
  br i1 %47, label %48, label %55

48:                                               ; preds = %43
  %49 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 32), align 8
  %50 = load i32, ptr %2, align 4
  %51 = call i32 %49(ptr noundef nonnull %40, i32 noundef %50)
  %52 = icmp eq i32 %51, 0
  br i1 %52, label %53, label %55

53:                                               ; preds = %48
  %54 = load ptr, ptr %40, align 8
  br label %56

55:                                               ; preds = %43, %48
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  br label %65

56:                                               ; preds = %39, %53
  %57 = phi ptr [ %54, %53 ], [ %41, %39 ]
  %58 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 40), align 8
  %59 = call i32 %58(ptr noundef %57)
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  %60 = icmp eq i32 %59, 0
  br i1 %60, label %61, label %65

61:                                               ; preds = %56
  %62 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 56), align 8
  %63 = load ptr, ptr %36, align 8
  %64 = call i32 %62(ptr noundef %63)
  br label %65

65:                                               ; preds = %55, %34, %56, %61
  %66 = add nuw nsw i64 %35, 1
  %67 = load i32, ptr @device_count, align 4
  %68 = sext i32 %67 to i64
  %69 = icmp slt i64 %66, %68
  br i1 %69, label %34, label %12

70:                                               ; preds = %1, %33
  ret void
}

declare void @free(ptr allocptr noundef captures(none)) local_unnamed_addr

define dso_local ptr @mgpuModuleGetFunction(ptr noundef %0, ptr noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca [128 x i8], align 16
  %5 = alloca ptr, align 8
  %6 = getelementptr inbounds nuw i8, ptr %0, i64 8
  %7 = load i32, ptr @current, align 4
  %8 = sext i32 %7 to i64
  %9 = getelementptr inbounds [8 x i8], ptr %6, i64 %8
  %10 = load ptr, ptr %9, align 8
  %11 = icmp eq ptr %10, null
  br i1 %11, label %12, label %25

12:                                               ; preds = %2
  %13 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 48), align 8
  %14 = load ptr, ptr %0, align 8
  %15 = tail call i32 %13(ptr noundef nonnull %9, ptr noundef %14)
  %16 = icmp eq i32 %15, 0
  br i1 %16, label %17, label %22

17:                                               ; preds = %12
  %18 = load i32, ptr @current, align 4
  %19 = sext i32 %18 to i64
  %20 = getelementptr inbounds [8 x i8], ptr %6, i64 %19
  %21 = load ptr, ptr %20, align 8
  br label %25

22:                                               ; preds = %12
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %23 = tail call fastcc ptr @error_name(i32 noundef %15)
  %24 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.2, ptr noundef %23)
  call fastcc void @fail(ptr noundef nonnull %4)
  unreachable

25:                                               ; preds = %17, %2
  %26 = phi ptr [ %21, %17 ], [ %10, %2 ]
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %27 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 64), align 8
  %28 = call i32 %27(ptr noundef nonnull %5, ptr noundef %26, ptr noundef %1)
  %29 = icmp eq i32 %28, 0
  br i1 %29, label %33, label %30

30:                                               ; preds = %25
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %31 = call fastcc ptr @error_name(i32 noundef %28)
  %32 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.3, ptr noundef %31)
  call fastcc void @fail(ptr noundef nonnull %3)
  unreachable

33:                                               ; preds = %25
  %34 = load ptr, ptr %5, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %5)
  ret ptr %34
}

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
  %25 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %13, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.4, ptr noundef %24)
  call fastcc void @fail(ptr noundef nonnull %13)
  unreachable

26:                                               ; preds = %12
  ret void
}

define dso_local ptr @mgpuStreamCreate() local_unnamed_addr {
  %1 = alloca [128 x i8], align 16
  %2 = load i32, ptr @current, align 4
  %3 = sext i32 %2 to i64
  %4 = getelementptr inbounds [24 x i8], ptr @devices, i64 %3
  %5 = load ptr, ptr %4, align 8
  %6 = icmp eq ptr %5, null
  br i1 %6, label %7, label %20

7:                                                ; preds = %0
  tail call fastcc void @probe()
  %8 = load i1, ptr @ready, align 4
  br i1 %8, label %11, label %9

9:                                                ; preds = %7
  %10 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @__neuro_gpu_launcher, ptr noundef %10)
  unreachable

11:                                               ; preds = %7
  %12 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 80), align 8
  %13 = tail call i32 %12(ptr noundef nonnull %4, i32 noundef 1)
  %14 = icmp eq i32 %13, 0
  br i1 %14, label %15, label %17

15:                                               ; preds = %11
  %16 = load ptr, ptr %4, align 8
  br label %20

17:                                               ; preds = %11
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  %18 = tail call fastcc ptr @error_name(i32 noundef %13)
  %19 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %1, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.5, ptr noundef %18)
  call fastcc void @fail(ptr noundef nonnull %1)
  unreachable

20:                                               ; preds = %15, %0
  %21 = phi ptr [ %16, %15 ], [ %5, %0 ]
  ret ptr %21
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
  %8 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.6, ptr noundef %7)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

9:                                                ; preds = %1
  ret void
}

define dso_local void @mgpuStreamDestroy(ptr nofree noundef readnone captures(none) %0) local_unnamed_addr {
  ret void
}

define dso_local ptr @mgpuMemAlloc(i64 noundef %0, ptr nofree noundef readnone captures(none) %1, i8 noundef zeroext %2) local_unnamed_addr {
  %4 = alloca i64, align 8
  tail call fastcc void @probe()
  %5 = load i1, ptr @ready, align 4
  br i1 %5, label %8, label %6

6:                                                ; preds = %3
  %7 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @__neuro_gpu_launcher, ptr noundef %7)
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

define dso_local void @mgpuMemFree(ptr noundef %0, ptr nofree noundef readnone captures(none) %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 112), align 8
  %5 = ptrtoint ptr %0 to i64
  %6 = tail call i32 %4(i64 noundef %5)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = tail call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.7, ptr noundef %9)
  call fastcc void @fail(ptr noundef nonnull %3)
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
  %13 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.8, ptr noundef %12)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

14:                                               ; preds = %4
  ret void
}

define dso_local nonnull ptr @_mlir_memref_to_llvm_alloc(i64 noundef %0) local_unnamed_addr {
  %2 = alloca i64, align 8
  %3 = alloca i64, align 8
  %4 = load i32, ptr @current, align 4
  %5 = sext i32 %4 to i64
  %6 = getelementptr inbounds [24 x i8], ptr @devices, i64 %5
  %7 = getelementptr inbounds nuw i8, ptr %6, i64 8
  %8 = load ptr, ptr %7, align 8
  %9 = icmp eq ptr %8, null
  br i1 %9, label %10, label %21

10:                                               ; preds = %1
  tail call fastcc void @probe()
  %11 = load i1, ptr @ready, align 4
  br i1 %11, label %14, label %12

12:                                               ; preds = %10
  %13 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @__neuro_gpu_launcher, ptr noundef %13)
  unreachable

14:                                               ; preds = %10
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  store i64 0, ptr %3, align 8
  %15 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 96), align 8
  %16 = call i32 %15(ptr noundef nonnull %3, i64 noundef 67108864)
  %17 = load i64, ptr %3, align 8
  %18 = inttoptr i64 %17 to ptr
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  %19 = icmp eq i32 %16, 0
  %20 = select i1 %19, ptr %18, ptr null
  store ptr %20, ptr %7, align 8
  br label %21

21:                                               ; preds = %14, %1
  %22 = phi ptr [ %20, %14 ], [ %8, %1 ]
  %23 = ptrtoint ptr %22 to i64
  %24 = getelementptr inbounds nuw i8, ptr %6, i64 16
  %25 = load i64, ptr %24, align 8
  %26 = add i64 %23, 255
  %27 = add i64 %26, %25
  %28 = and i64 %27, -256
  %29 = sub i64 %28, %23
  %30 = icmp eq ptr %22, null
  %31 = icmp ugt i64 %29, 67108864
  %32 = select i1 %30, i1 true, i1 %31
  %33 = sub nuw nsw i64 67108864, %29
  %34 = icmp ugt i64 %0, %33
  %35 = select i1 %32, i1 true, i1 %34
  br i1 %35, label %39, label %36

36:                                               ; preds = %21
  %37 = add nuw nsw i64 %29, %0
  store i64 %37, ptr %24, align 8
  %38 = icmp eq i64 %28, 0
  br i1 %38, label %39, label %53

39:                                               ; preds = %21, %36
  call fastcc void @probe()
  %40 = load i1, ptr @ready, align 4
  br i1 %40, label %43, label %41

41:                                               ; preds = %39
  %42 = load ptr, ptr @unusable_reason, align 8
  call fastcc void @unusable(ptr noundef nonnull @__neuro_gpu_launcher, ptr noundef %42)
  unreachable

43:                                               ; preds = %39
  %44 = icmp eq i64 %0, 0
  br i1 %44, label %52, label %45

45:                                               ; preds = %43
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  store i64 0, ptr %2, align 8
  %46 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 96), align 8
  %47 = call i32 %46(ptr noundef nonnull %2, i64 noundef %0)
  %48 = load i64, ptr %2, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  %49 = icmp ne i32 %47, 0
  %50 = icmp eq i64 %48, 0
  %51 = select i1 %49, i1 true, i1 %50
  br i1 %51, label %52, label %53

52:                                               ; preds = %43, %45
  call fastcc void @report(ptr noundef nonnull @ALLOCATION_FAILED, i32 noundef 76)
  unreachable

53:                                               ; preds = %45, %36
  %54 = phi i64 [ %48, %45 ], [ %28, %36 ]
  %55 = inttoptr i64 %54 to ptr
  ret ptr %55
}

define dso_local void @_mlir_memref_to_llvm_free(ptr noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = icmp eq ptr %0, null
  br i1 %3, label %23, label %4

4:                                                ; preds = %1
  %5 = load i32, ptr @current, align 4
  %6 = sext i32 %5 to i64
  %7 = getelementptr inbounds [24 x i8], ptr @devices, i64 %6
  %8 = getelementptr inbounds nuw i8, ptr %7, i64 8
  %9 = load ptr, ptr %8, align 8
  %10 = ptrtoint ptr %9 to i64
  %11 = icmp eq ptr %9, null
  %12 = ptrtoint ptr %0 to i64
  %13 = sub i64 %12, %10
  %14 = icmp ugt i64 %13, 67108863
  %15 = or i1 %11, %14
  br i1 %15, label %16, label %23

16:                                               ; preds = %4
  %17 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 112), align 8
  %18 = tail call i32 %17(i64 noundef %12)
  %19 = icmp eq i32 %18, 0
  br i1 %19, label %23, label %20

20:                                               ; preds = %16
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %21 = tail call fastcc ptr @error_name(i32 noundef %18)
  %22 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.7, ptr noundef %21)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

23:                                               ; preds = %16, %1, %4
  ret void
}

define dso_local i64 @__neuro_device_mark() local_unnamed_addr {
  %1 = load i32, ptr @current, align 4
  %2 = sext i32 %1 to i64
  %3 = getelementptr inbounds [24 x i8], ptr @devices, i64 %2
  %4 = getelementptr inbounds nuw i8, ptr %3, i64 16
  %5 = load i64, ptr %4, align 8
  ret i64 %5
}

define dso_local void @__neuro_device_restore(i64 noundef %0) local_unnamed_addr {
  %2 = load i32, ptr @current, align 4
  %3 = sext i32 %2 to i64
  %4 = getelementptr inbounds [24 x i8], ptr @devices, i64 %3
  %5 = getelementptr inbounds nuw i8, ptr %4, i64 16
  store i64 %0, ptr %5, align 8
  ret void
}

define dso_local void @__neuro_device_check(i32 noundef %0) local_unnamed_addr {
  %2 = alloca [256 x i8], align 16
  tail call fastcc void @probe()
  %3 = load i1, ptr @ready, align 4
  br i1 %3, label %6, label %4

4:                                                ; preds = %1
  %5 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %5)
  unreachable

6:                                                ; preds = %1
  %7 = icmp sgt i32 %0, -1
  %8 = load i32, ptr @device_count, align 4
  %9 = icmp slt i32 %0, %8
  %10 = select i1 %7, i1 %9, i1 false
  br i1 %10, label %11, label %12

11:                                               ; preds = %6
  ret void

12:                                               ; preds = %6
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %13 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 256, ptr noundef nonnull @.str.9, i32 noundef %0, i32 noundef %8)
  call fastcc void @report(ptr noundef nonnull %2, i32 noundef %13)
  unreachable
}

define dso_local ptr @__neuro_device_alloc(i64 noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca i64, align 8
  %5 = alloca i64, align 8
  tail call fastcc void @probe()
  %6 = load i1, ptr @ready, align 4
  br i1 %6, label %9, label %7

7:                                                ; preds = %2
  %8 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %8)
  unreachable

9:                                                ; preds = %2
  %10 = icmp ne i32 %1, 0
  %11 = load i32, ptr @current, align 4
  %12 = icmp eq i32 %11, 0
  %13 = select i1 %10, i1 %12, i1 false
  br i1 %13, label %16, label %14

14:                                               ; preds = %9
  %15 = tail call i64 @llvm.umax.i64(i64 %0, i64 1)
  br label %48

16:                                               ; preds = %9
  %17 = tail call i64 @llvm.umax.i64(i64 %0, i64 1)
  %18 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @devices, i64 8), align 8
  %19 = icmp eq ptr %18, null
  br i1 %19, label %20, label %31

20:                                               ; preds = %16
  tail call fastcc void @probe()
  %21 = load i1, ptr @ready, align 4
  br i1 %21, label %24, label %22

22:                                               ; preds = %20
  %23 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @__neuro_gpu_launcher, ptr noundef %23)
  unreachable

24:                                               ; preds = %20
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  store i64 0, ptr %5, align 8
  %25 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 96), align 8
  %26 = call i32 %25(ptr noundef nonnull %5, i64 noundef 67108864)
  %27 = load i64, ptr %5, align 8
  %28 = inttoptr i64 %27 to ptr
  call void @llvm.lifetime.end.p0(ptr nonnull %5)
  %29 = icmp eq i32 %26, 0
  %30 = select i1 %29, ptr %28, ptr null
  store ptr %30, ptr getelementptr inbounds nuw (i8, ptr @devices, i64 8), align 8
  br label %31

31:                                               ; preds = %24, %16
  %32 = phi ptr [ %30, %24 ], [ %18, %16 ]
  %33 = ptrtoint ptr %32 to i64
  %34 = load i64, ptr getelementptr inbounds nuw (i8, ptr @devices, i64 16), align 16
  %35 = add i64 %33, 255
  %36 = add i64 %35, %34
  %37 = and i64 %36, -256
  %38 = sub i64 %37, %33
  %39 = icmp eq ptr %32, null
  %40 = icmp ugt i64 %38, 67108864
  %41 = select i1 %39, i1 true, i1 %40
  %42 = sub nuw nsw i64 67108864, %38
  %43 = icmp ugt i64 %17, %42
  %44 = select i1 %41, i1 true, i1 %43
  br i1 %44, label %48, label %45

45:                                               ; preds = %31
  %46 = add nuw nsw i64 %38, %17
  store i64 %46, ptr getelementptr inbounds nuw (i8, ptr @devices, i64 16), align 16
  %47 = icmp eq i64 %37, 0
  br i1 %47, label %48, label %57

48:                                               ; preds = %14, %31, %45
  %49 = phi i64 [ %15, %14 ], [ %17, %31 ], [ %17, %45 ]
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  store i64 0, ptr %4, align 8
  %50 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 96), align 8
  %51 = call i32 %50(ptr noundef nonnull %4, i64 noundef %49)
  %52 = load i64, ptr %4, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  %53 = icmp eq i32 %51, 0
  br i1 %53, label %57, label %54

54:                                               ; preds = %48
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %55 = call fastcc ptr @error_name(i32 noundef %51)
  %56 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.10, ptr noundef %55)
  call fastcc void @fail(ptr noundef nonnull %3)
  unreachable

57:                                               ; preds = %48, %45
  %58 = phi i64 [ %37, %45 ], [ %52, %48 ]
  %59 = inttoptr i64 %58 to ptr
  ret ptr %59
}

define dso_local ptr @__neuro_device_upload(ptr noundef %0, i64 noundef %1, i32 noundef %2, i32 noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = alloca [128 x i8], align 16
  %7 = alloca [256 x i8], align 16
  tail call fastcc void @probe()
  %8 = load i1, ptr @ready, align 4
  br i1 %8, label %11, label %9

9:                                                ; preds = %4
  %10 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %10)
  unreachable

11:                                               ; preds = %4
  %12 = icmp sgt i32 %2, -1
  %13 = load i32, ptr @device_count, align 4
  %14 = icmp slt i32 %2, %13
  %15 = select i1 %12, i1 %14, i1 false
  br i1 %15, label %18, label %16

16:                                               ; preds = %11
  call void @llvm.lifetime.start.p0(ptr nonnull %7)
  %17 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %7, i64 noundef 256, ptr noundef nonnull @.str.9, i32 noundef %2, i32 noundef %13)
  call fastcc void @report(ptr noundef nonnull %7, i32 noundef %17)
  unreachable

18:                                               ; preds = %11
  %19 = tail call i32 @__neuro_device_switch(i32 noundef %2)
  %20 = tail call ptr @__neuro_device_alloc(i64 noundef %1, i32 noundef %3)
  %21 = tail call ptr @mgpuStreamCreate()
  %22 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 120), align 8
  %23 = ptrtoint ptr %20 to i64
  %24 = ptrtoint ptr %0 to i64
  %25 = tail call i32 %22(i64 noundef %23, i64 noundef %24, i64 noundef %1, ptr noundef %21)
  %26 = icmp eq i32 %25, 0
  br i1 %26, label %30, label %27

27:                                               ; preds = %18
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %28 = tail call fastcc ptr @error_name(i32 noundef %25)
  %29 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %6, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.8, ptr noundef %28)
  call fastcc void @fail(ptr noundef nonnull %6)
  unreachable

30:                                               ; preds = %18
  %31 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %32 = tail call i32 %31(ptr noundef %21)
  %33 = icmp eq i32 %32, 0
  br i1 %33, label %37, label %34

34:                                               ; preds = %30
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %35 = tail call fastcc ptr @error_name(i32 noundef %32)
  %36 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.6, ptr noundef %35)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

37:                                               ; preds = %30
  %38 = tail call i32 @__neuro_device_switch(i32 noundef %19)
  ret ptr %20
}

define dso_local void @__neuro_device_copy(ptr noundef %0, ptr noundef %1, i64 noundef %2, i32 noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = alloca [128 x i8], align 16
  %7 = tail call i32 @__neuro_device_switch(i32 noundef %3)
  %8 = tail call ptr @mgpuStreamCreate()
  %9 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 120), align 8
  %10 = ptrtoint ptr %0 to i64
  %11 = ptrtoint ptr %1 to i64
  %12 = tail call i32 %9(i64 noundef %10, i64 noundef %11, i64 noundef %2, ptr noundef %8)
  %13 = icmp eq i32 %12, 0
  br i1 %13, label %17, label %14

14:                                               ; preds = %4
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %15 = tail call fastcc ptr @error_name(i32 noundef %12)
  %16 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %6, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.8, ptr noundef %15)
  call fastcc void @fail(ptr noundef nonnull %6)
  unreachable

17:                                               ; preds = %4
  %18 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %19 = tail call i32 %18(ptr noundef %8)
  %20 = icmp eq i32 %19, 0
  br i1 %20, label %24, label %21

21:                                               ; preds = %17
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %22 = tail call fastcc ptr @error_name(i32 noundef %19)
  %23 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.6, ptr noundef %22)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

24:                                               ; preds = %17
  %25 = tail call i32 @__neuro_device_switch(i32 noundef %7)
  ret void
}

define dso_local ptr @__neuro_device_clone(ptr noundef %0, i64 noundef %1, i32 noundef %2, i32 noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = tail call i32 @__neuro_device_switch(i32 noundef %2)
  %7 = tail call ptr @__neuro_device_alloc(i64 noundef %1, i32 noundef %3)
  %8 = tail call ptr @mgpuStreamCreate()
  %9 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 120), align 8
  %10 = ptrtoint ptr %7 to i64
  %11 = ptrtoint ptr %0 to i64
  %12 = tail call i32 %9(i64 noundef %10, i64 noundef %11, i64 noundef %1, ptr noundef %8)
  %13 = icmp eq i32 %12, 0
  br i1 %13, label %17, label %14

14:                                               ; preds = %4
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %15 = tail call fastcc ptr @error_name(i32 noundef %12)
  %16 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.8, ptr noundef %15)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

17:                                               ; preds = %4
  %18 = tail call i32 @__neuro_device_switch(i32 noundef %6)
  ret ptr %7
}

define dso_local void @__neuro_device_free(ptr noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca [128 x i8], align 16
  %5 = sext i32 %1 to i64
  %6 = getelementptr inbounds [24 x i8], ptr @devices, i64 %5
  %7 = getelementptr inbounds nuw i8, ptr %6, i64 8
  %8 = load ptr, ptr %7, align 8
  %9 = ptrtoint ptr %8 to i64
  %10 = icmp eq ptr %8, null
  %11 = ptrtoint ptr %0 to i64
  %12 = sub i64 %11, %9
  %13 = icmp ugt i64 %12, 67108863
  %14 = or i1 %10, %13
  br i1 %14, label %15, label %33

15:                                               ; preds = %2
  %16 = tail call i32 @__neuro_device_switch(i32 noundef %1)
  %17 = tail call ptr @mgpuStreamCreate()
  %18 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %19 = tail call i32 %18(ptr noundef %17)
  %20 = icmp eq i32 %19, 0
  br i1 %20, label %24, label %21

21:                                               ; preds = %15
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %22 = tail call fastcc ptr @error_name(i32 noundef %19)
  %23 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.6, ptr noundef %22)
  call fastcc void @fail(ptr noundef nonnull %4)
  unreachable

24:                                               ; preds = %15
  %25 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 112), align 8
  %26 = tail call i32 %25(i64 noundef %11)
  %27 = icmp eq i32 %26, 0
  br i1 %27, label %31, label %28

28:                                               ; preds = %24
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %29 = tail call fastcc ptr @error_name(i32 noundef %26)
  %30 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.7, ptr noundef %29)
  call fastcc void @fail(ptr noundef nonnull %3)
  unreachable

31:                                               ; preds = %24
  %32 = tail call i32 @__neuro_device_switch(i32 noundef %16)
  br label %33

33:                                               ; preds = %2, %31
  ret void
}

define dso_local ptr @__neuro_device_move(ptr noundef %0, i64 noundef %1, i32 noundef %2, i32 noundef %3, i32 noundef %4) local_unnamed_addr {
  %6 = alloca [128 x i8], align 16
  %7 = alloca [128 x i8], align 16
  %8 = alloca [128 x i8], align 16
  %9 = alloca [256 x i8], align 16
  tail call fastcc void @probe()
  %10 = load i1, ptr @ready, align 4
  br i1 %10, label %13, label %11

11:                                               ; preds = %5
  %12 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %12)
  unreachable

13:                                               ; preds = %5
  %14 = icmp sgt i32 %3, -1
  %15 = load i32, ptr @device_count, align 4
  %16 = icmp slt i32 %3, %15
  %17 = select i1 %14, i1 %16, i1 false
  br i1 %17, label %20, label %18

18:                                               ; preds = %13
  call void @llvm.lifetime.start.p0(ptr nonnull %9)
  %19 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %9, i64 noundef 256, ptr noundef nonnull @.str.9, i32 noundef %3, i32 noundef %15)
  call fastcc void @report(ptr noundef nonnull %9, i32 noundef %19)
  unreachable

20:                                               ; preds = %13
  %21 = icmp eq i32 %2, %3
  br i1 %21, label %52, label %22

22:                                               ; preds = %20
  %23 = tail call i32 @__neuro_device_switch(i32 noundef %2)
  %24 = tail call ptr @mgpuStreamCreate()
  %25 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %26 = tail call i32 %25(ptr noundef %24)
  %27 = icmp eq i32 %26, 0
  br i1 %27, label %31, label %28

28:                                               ; preds = %22
  call void @llvm.lifetime.start.p0(ptr nonnull %8)
  %29 = tail call fastcc ptr @error_name(i32 noundef %26)
  %30 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %8, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.6, ptr noundef %29)
  call fastcc void @fail(ptr noundef nonnull %8)
  unreachable

31:                                               ; preds = %22
  %32 = tail call i32 @__neuro_device_switch(i32 noundef %3)
  %33 = tail call ptr @__neuro_device_alloc(i64 noundef %1, i32 noundef %4)
  %34 = tail call ptr @mgpuStreamCreate()
  %35 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 120), align 8
  %36 = ptrtoint ptr %33 to i64
  %37 = ptrtoint ptr %0 to i64
  %38 = tail call i32 %35(i64 noundef %36, i64 noundef %37, i64 noundef %1, ptr noundef %34)
  %39 = icmp eq i32 %38, 0
  br i1 %39, label %43, label %40

40:                                               ; preds = %31
  call void @llvm.lifetime.start.p0(ptr nonnull %7)
  %41 = tail call fastcc ptr @error_name(i32 noundef %38)
  %42 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %7, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.8, ptr noundef %41)
  call fastcc void @fail(ptr noundef nonnull %7)
  unreachable

43:                                               ; preds = %31
  %44 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @drv, i64 88), align 8
  %45 = tail call i32 %44(ptr noundef %34)
  %46 = icmp eq i32 %45, 0
  br i1 %46, label %50, label %47

47:                                               ; preds = %43
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %48 = tail call fastcc ptr @error_name(i32 noundef %45)
  %49 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %6, i64 noundef 128, ptr noundef nonnull @.str.22, ptr noundef nonnull @.str.6, ptr noundef %48)
  call fastcc void @fail(ptr noundef nonnull %6)
  unreachable

50:                                               ; preds = %43
  tail call void @__neuro_device_free(ptr noundef %0, i32 noundef %2)
  %51 = tail call i32 @__neuro_device_switch(i32 noundef %23)
  br label %52

52:                                               ; preds = %20, %50
  %53 = phi ptr [ %33, %50 ], [ %0, %20 ]
  ret ptr %53
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

define internal fastcc void @fail(ptr noundef %0) unnamed_addr {
  %2 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %3 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 256, ptr noundef nonnull @.str.23, ptr noundef %0)
  call fastcc void @report(ptr noundef nonnull %2, i32 noundef %3)
  unreachable
}

declare void @__neuro_gpu_panic(ptr noundef, i64 noundef) local_unnamed_addr

define internal fastcc void @unusable(ptr noundef %0, ptr noundef %1) unnamed_addr {
  %3 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %4 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef nonnull @.str.26, ptr noundef %0, ptr noundef %1)
  call fastcc void @report(ptr noundef nonnull %3, i32 noundef %4)
  unreachable
}

declare noalias noundef ptr @calloc(i64 noundef, i64 noundef) local_unnamed_addr

declare i32 @llvm.smax.i32(i32, i32)

declare i32 @llvm.umin.i32(i32, i32)

declare i64 @llvm.umax.i64(i64, i64)

