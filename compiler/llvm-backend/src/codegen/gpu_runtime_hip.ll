;
; MLIR's GPU runtime ABI (`mgpu*`) over HIP, linked in place of gpu_runtime.ll when the
; program is built for an AMD GPU. HIP (libamdhip64) is opened with dlopen, so a binary
; without a usable AMD GPU starts, then panics from the first module load (a global
; constructor) saying why, unless every `@gpu` function has a host fallback
; (`__neuro_gpu_fallback`), in which case `__neuro_gpu_usable` sends each call to its
; host body. The `__neuro_device_*` functions move a tensor's buffer to and from the
; GPU for `.to(Device::GPU(n))`, so a program that transfers a tensor links this module
; too. It keeps a context, stream, device arena (`_mlir_memref_to_llvm_alloc` / `_free`)
; and copy of each kernel module per device, acting on the current one. Failures call
; `__neuro_gpu_panic`, which the LLVM backend defines as an ordinary runtime panic.
;
; Generated from compiler/llvm-backend/src/codegen/gpu_runtime.c with clang -O2
; -emit-llvm -DNEURO_HIP, then stripped of the target datalayout and triple, attribute
; groups and metadata. Regenerate it with tools/regen_gpu_runtime.sh.
;

%struct.device = type { ptr, ptr, i64 }

@ready = internal unnamed_addr global i1 false, align 4
@current = internal unnamed_addr global i32 0, align 4
@.str = private unnamed_addr constant [13 x i8] c"hipSetDevice\00", align 1
@.str.1 = private unnamed_addr constant [109 x i8] c"a `@gpu` call's operands live on GPU %d and GPU %d: move them to one device with `.to(Device::GPU(n))` first\00", align 1
@device_count = internal global i32 0, align 4
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
@.str.2 = private unnamed_addr constant [18 x i8] c"hipModuleLoadData\00", align 1
@.str.3 = private unnamed_addr constant [21 x i8] c"hipModuleGetFunction\00", align 1
@.str.4 = private unnamed_addr constant [22 x i8] c"hipModuleLaunchKernel\00", align 1
@devices = internal global [64 x %struct.device] zeroinitializer, align 16
@.str.5 = private unnamed_addr constant [25 x i8] c"hipStreamCreateWithFlags\00", align 1
@.str.6 = private unnamed_addr constant [21 x i8] c"hipStreamSynchronize\00", align 1
@.str.7 = private unnamed_addr constant [8 x i8] c"hipFree\00", align 1
@.str.8 = private unnamed_addr constant [15 x i8] c"hipMemcpyAsync\00", align 1
@ALLOCATION_FAILED = internal constant [77 x i8] c"device memory allocation failed: no GPU is available, or it is out of memory\00", align 16
@TRANSFER = internal constant [14 x i8] c"`Device::GPU`\00", align 1
@.str.9 = private unnamed_addr constant [52 x i8] c"`Device::GPU(%d)` names no GPU: this machine has %d\00", align 1
@.str.10 = private unnamed_addr constant [10 x i8] c"hipMalloc\00", align 1
@probed = internal unnamed_addr global i1 false, align 4
@libraries.rel = internal unnamed_addr constant [4 x i32] [i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.11 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32), i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.12 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32), i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.13 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32), i32 trunc (i64 sub (i64 ptrtoint (ptr @.str.14 to i64), i64 ptrtoint (ptr @libraries.rel to i64)) to i32)], align 4
@NO_LIBRARY = internal constant [53 x i8] c"the HIP runtime (libamdhip64.so) could not be loaded\00", align 16
@unusable_reason = internal unnamed_addr global ptr null, align 8
@TOO_OLD = internal constant [27 x i8] c"the HIP runtime is too old\00", align 16
@NO_DEVICE = internal constant [34 x i8] c"the HIP runtime reports no device\00", align 16
@.str.11 = private unnamed_addr constant [15 x i8] c"libamdhip64.so\00", align 1
@.str.12 = private unnamed_addr constant [17 x i8] c"libamdhip64.so.7\00", align 1
@.str.13 = private unnamed_addr constant [17 x i8] c"libamdhip64.so.6\00", align 1
@.str.14 = private unnamed_addr constant [29 x i8] c"/opt/rocm/lib/libamdhip64.so\00", align 1
@.str.15 = private unnamed_addr constant [8 x i8] c"hipInit\00", align 1
@.str.16 = private unnamed_addr constant [16 x i8] c"hipGetErrorName\00", align 1
@.str.17 = private unnamed_addr constant [18 x i8] c"hipGetDeviceCount\00", align 1
@.str.18 = private unnamed_addr constant [16 x i8] c"hipModuleUnload\00", align 1
@.str.19 = private unnamed_addr constant [17 x i8] c"hipMallocManaged\00", align 1
@.str.20 = private unnamed_addr constant [21 x i8] c"an unknown HIP error\00", align 1
@LAUNCH = internal constant [7 x i8] c"`@gpu`\00", align 1
@.str.21 = private unnamed_addr constant [18 x i8] c"%s failed with %s\00", align 1
@.str.22 = private unnamed_addr constant [14 x i8] c"GPU error: %s\00", align 1
@__neuro_gpu_fallback = external local_unnamed_addr constant i8, align 1
@load_failure = internal global [128 x i8] zeroinitializer, align 16
@.str.23 = private unnamed_addr constant [51 x i8] c"its driver cannot load this program's kernels (%s)\00", align 1
@.str.24 = private unnamed_addr constant [46 x i8] c"no host memory left to record a kernel module\00", align 1
@.str.25 = private unnamed_addr constant [44 x i8] c"%s needs an AMD GPU, and none is usable: %s\00", align 1

define dso_local range(i32 0, 2) i32 @__neuro_gpu_usable() local_unnamed_addr {
  tail call fastcc void @probe()
  %1 = load i1, ptr @ready, align 4
  %2 = zext i1 %1 to i32
  ret i32 %2
}

define internal fastcc void @probe() unnamed_addr {
  %1 = load i1, ptr @probed, align 4
  br i1 %1, label %87, label %2

2:                                                ; preds = %0
  store i1 true, ptr @probed, align 4
  br label %7

3:                                                ; preds = %7
  br i1 %13, label %16, label %4

4:                                                ; preds = %3
  %5 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.15)
  store ptr %5, ptr @drv.0, align 8
  %6 = icmp eq ptr %5, null
  br i1 %6, label %60, label %17

7:                                                ; preds = %2, %7
  %8 = phi i64 [ 0, %2 ], [ %12, %7 ]
  %9 = shl i64 %8, 2
  %10 = call ptr @llvm.load.relative.i64(ptr @libraries.rel, i64 %9)
  %11 = tail call ptr @dlopen(ptr noundef %10, i32 noundef 2)
  %12 = add nuw nsw i64 %8, 1
  %13 = icmp eq ptr %11, null
  %14 = icmp samesign ult i64 %8, 3
  %15 = select i1 %13, i1 %14, i1 false
  br i1 %15, label %7, label %3

16:                                               ; preds = %3
  store ptr @NO_LIBRARY, ptr @unusable_reason, align 8
  br label %87

17:                                               ; preds = %4
  %18 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.16)
  store ptr %18, ptr @drv.1, align 8
  %19 = icmp eq ptr %18, null
  br i1 %19, label %60, label %20

20:                                               ; preds = %17
  %21 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.17)
  store ptr %21, ptr @drv.2, align 8
  %22 = icmp eq ptr %21, null
  br i1 %22, label %60, label %23

23:                                               ; preds = %20
  %24 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str)
  store ptr %24, ptr @drv.3, align 8
  %25 = icmp eq ptr %24, null
  br i1 %25, label %60, label %26

26:                                               ; preds = %23
  %27 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.2)
  store ptr %27, ptr @drv.4, align 8
  %28 = icmp eq ptr %27, null
  br i1 %28, label %60, label %29

29:                                               ; preds = %26
  %30 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.18)
  store ptr %30, ptr @drv.5, align 8
  %31 = icmp eq ptr %30, null
  br i1 %31, label %60, label %32

32:                                               ; preds = %29
  %33 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.3)
  store ptr %33, ptr @drv.6, align 8
  %34 = icmp eq ptr %33, null
  br i1 %34, label %60, label %35

35:                                               ; preds = %32
  %36 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.4)
  store ptr %36, ptr @drv.7, align 8
  %37 = icmp eq ptr %36, null
  br i1 %37, label %60, label %38

38:                                               ; preds = %35
  %39 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.5)
  store ptr %39, ptr @drv.8, align 8
  %40 = icmp eq ptr %39, null
  br i1 %40, label %60, label %41

41:                                               ; preds = %38
  %42 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.6)
  store ptr %42, ptr @drv.9, align 8
  %43 = icmp eq ptr %42, null
  br i1 %43, label %60, label %44

44:                                               ; preds = %41
  %45 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.10)
  store ptr %45, ptr @drv.10, align 8
  %46 = icmp eq ptr %45, null
  br i1 %46, label %60, label %47

47:                                               ; preds = %44
  %48 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.19)
  store ptr %48, ptr @drv.11, align 8
  %49 = icmp eq ptr %48, null
  br i1 %49, label %60, label %50

50:                                               ; preds = %47
  %51 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.7)
  store ptr %51, ptr @drv.12, align 8
  %52 = icmp eq ptr %51, null
  br i1 %52, label %60, label %53

53:                                               ; preds = %50
  %54 = tail call ptr @dlsym(ptr noundef nonnull %11, ptr noundef nonnull @.str.8)
  store ptr %54, ptr @drv.13, align 8
  %55 = icmp eq ptr %54, null
  br i1 %55, label %60, label %56

56:                                               ; preds = %53
  %57 = load ptr, ptr @drv.0, align 8
  %58 = tail call i32 %57(i32 noundef 0)
  %59 = icmp eq i32 %58, 0
  br i1 %59, label %66, label %61

60:                                               ; preds = %53, %50, %47, %44, %41, %38, %35, %32, %29, %26, %23, %20, %17, %4
  store ptr @TOO_OLD, ptr @unusable_reason, align 8
  br label %87

61:                                               ; preds = %56
  %62 = load ptr, ptr @drv.1, align 8
  %63 = tail call ptr %62(i32 noundef range(i32 1, 0) %58)
  %64 = icmp eq ptr %63, null
  %65 = select i1 %64, ptr @.str.20, ptr %63
  store ptr %65, ptr @unusable_reason, align 8
  br label %87

66:                                               ; preds = %56
  %67 = load ptr, ptr @drv.2, align 8
  %68 = tail call i32 %67(ptr noundef nonnull @device_count)
  %69 = icmp ne i32 %68, 0
  %70 = load i32, ptr @device_count, align 4
  %71 = icmp slt i32 %70, 1
  %72 = select i1 %69, i1 true, i1 %71
  br i1 %72, label %73, label %74

73:                                               ; preds = %66
  store ptr @NO_DEVICE, ptr @unusable_reason, align 8
  br label %87

74:                                               ; preds = %66
  %75 = icmp samesign ugt i32 %70, 64
  br i1 %75, label %76, label %77

76:                                               ; preds = %74
  store i32 64, ptr @device_count, align 4
  br label %77

77:                                               ; preds = %76, %74
  %78 = load ptr, ptr @drv.3, align 8
  %79 = tail call i32 %78(i32 noundef 0)
  %80 = icmp eq i32 %79, 0
  br i1 %80, label %86, label %81

81:                                               ; preds = %77
  %82 = load ptr, ptr @drv.1, align 8
  %83 = tail call ptr %82(i32 noundef range(i32 1, 0) %79)
  %84 = icmp eq ptr %83, null
  %85 = select i1 %84, ptr @.str.20, ptr %83
  store ptr %85, ptr @unusable_reason, align 8
  br label %87

86:                                               ; preds = %77
  store i1 true, ptr @ready, align 4
  br label %87

87:                                               ; preds = %60, %16, %86, %81, %73, %61, %0
  ret void
}

define dso_local i32 @__neuro_device_switch(i32 noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = load i32, ptr @current, align 4
  %4 = tail call i32 @llvm.smax.i32(i32 %0, i32 0)
  %5 = icmp eq i32 %4, %3
  br i1 %5, label %21, label %6

6:                                                ; preds = %1
  tail call fastcc void @probe()
  %7 = load i1, ptr @ready, align 4
  br i1 %7, label %10, label %8

8:                                                ; preds = %6
  %9 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %9)
  unreachable

10:                                               ; preds = %6
  %11 = load ptr, ptr @drv.3, align 8
  %12 = tail call i32 %11(i32 noundef %4)
  %13 = icmp eq i32 %12, 0
  br i1 %13, label %20, label %14

14:                                               ; preds = %10
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %15 = load ptr, ptr @drv.1, align 8
  %16 = tail call ptr %15(i32 noundef range(i32 1, 0) %12)
  %17 = icmp eq ptr %16, null
  %18 = select i1 %17, ptr @.str.20, ptr %16
  %19 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str, ptr noundef nonnull %18)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

20:                                               ; preds = %10
  store i32 %4, ptr @current, align 4
  br label %21

21:                                               ; preds = %20, %1
  ret i32 %3
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

define dso_local noalias noundef ptr @mgpuModuleLoad(ptr noundef %0, i64 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define internal fastcc noalias noundef ptr @load(ptr noundef %0) unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = alloca ptr, align 8
  tail call fastcc void @probe()
  %4 = load i1, ptr @ready, align 4
  %5 = load i8, ptr @__neuro_gpu_fallback, align 1
  %6 = icmp eq i8 %5, 0
  %7 = select i1 %4, i1 true, i1 %6
  br i1 %7, label %8, label %43

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
  %22 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) @load_failure, i64 noundef 128, ptr noundef nonnull @.str.23, ptr noundef nonnull %21)
  store ptr @load_failure, ptr @unusable_reason, align 8
  store i1 false, ptr @ready, align 4
  br label %41

23:                                               ; preds = %11
  %24 = icmp eq i32 %13, 0
  br i1 %24, label %31, label %25

25:                                               ; preds = %23
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %26 = load ptr, ptr @drv.1, align 8
  %27 = call ptr %26(i32 noundef range(i32 1, 0) %13)
  %28 = icmp eq ptr %27, null
  %29 = select i1 %28, ptr @.str.20, ptr %27
  %30 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.2, ptr noundef nonnull %29)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

31:                                               ; preds = %23
  %32 = call noalias dereferenceable_or_null(520) ptr @calloc(i64 noundef 1, i64 noundef 520)
  %33 = icmp eq ptr %32, null
  br i1 %33, label %34, label %35

34:                                               ; preds = %31
  call fastcc void @fail(ptr noundef nonnull @.str.24)
  unreachable

35:                                               ; preds = %31
  store ptr %0, ptr %32, align 8
  %36 = load ptr, ptr %3, align 8
  %37 = getelementptr inbounds nuw i8, ptr %32, i64 8
  %38 = load i32, ptr @current, align 4
  %39 = sext i32 %38 to i64
  %40 = getelementptr inbounds ptr, ptr %37, i64 %39
  store ptr %36, ptr %40, align 8
  br label %41

41:                                               ; preds = %35, %17
  %42 = phi ptr [ null, %17 ], [ %32, %35 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  br label %43

43:                                               ; preds = %1, %41
  %44 = phi ptr [ %42, %41 ], [ null, %1 ]
  ret ptr %44
}

define dso_local noalias noundef ptr @mgpuModuleLoadJIT(ptr noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = tail call fastcc ptr @load(ptr noundef %0)
  ret ptr %3
}

define dso_local void @mgpuModuleUnload(ptr noundef captures(address_is_null) %0) local_unnamed_addr {
  %2 = icmp ne ptr %0, null
  %3 = load i1, ptr @ready, align 4
  %4 = select i1 %2, i1 %3, i1 false
  br i1 %4, label %5, label %33

5:                                                ; preds = %1
  %6 = load i32, ptr @device_count, align 4
  %7 = icmp sgt i32 %6, 0
  br i1 %7, label %8, label %10

8:                                                ; preds = %5
  %9 = getelementptr inbounds nuw i8, ptr %0, i64 8
  br label %14

10:                                               ; preds = %28, %5
  %11 = load i32, ptr @current, align 4
  %12 = load ptr, ptr @drv.3, align 8
  %13 = tail call i32 %12(i32 noundef %11)
  tail call void @free(ptr noundef nonnull %0)
  br label %33

14:                                               ; preds = %8, %28
  %15 = phi i64 [ 0, %8 ], [ %29, %28 ]
  %16 = getelementptr inbounds nuw ptr, ptr %9, i64 %15
  %17 = load ptr, ptr %16, align 8
  %18 = icmp eq ptr %17, null
  br i1 %18, label %28, label %19

19:                                               ; preds = %14
  %20 = load ptr, ptr @drv.3, align 8
  %21 = trunc nuw nsw i64 %15 to i32
  %22 = tail call i32 %20(i32 noundef %21)
  %23 = icmp eq i32 %22, 0
  br i1 %23, label %24, label %28

24:                                               ; preds = %19
  %25 = load ptr, ptr @drv.5, align 8
  %26 = load ptr, ptr %16, align 8
  %27 = tail call i32 %25(ptr noundef %26)
  br label %28

28:                                               ; preds = %14, %19, %24
  %29 = add nuw nsw i64 %15, 1
  %30 = load i32, ptr @device_count, align 4
  %31 = sext i32 %30 to i64
  %32 = icmp slt i64 %29, %31
  br i1 %32, label %14, label %10

33:                                               ; preds = %1, %10
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
  %9 = getelementptr inbounds ptr, ptr %6, i64 %8
  %10 = load ptr, ptr %9, align 8
  %11 = icmp eq ptr %10, null
  br i1 %11, label %12, label %28

12:                                               ; preds = %2
  %13 = load ptr, ptr @drv.4, align 8
  %14 = load ptr, ptr %0, align 8
  %15 = tail call i32 %13(ptr noundef nonnull %9, ptr noundef %14)
  %16 = icmp eq i32 %15, 0
  br i1 %16, label %17, label %22

17:                                               ; preds = %12
  %18 = load i32, ptr @current, align 4
  %19 = sext i32 %18 to i64
  %20 = getelementptr inbounds ptr, ptr %6, i64 %19
  %21 = load ptr, ptr %20, align 8
  br label %28

22:                                               ; preds = %12
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %23 = load ptr, ptr @drv.1, align 8
  %24 = tail call ptr %23(i32 noundef range(i32 1, 0) %15)
  %25 = icmp eq ptr %24, null
  %26 = select i1 %25, ptr @.str.20, ptr %24
  %27 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.2, ptr noundef nonnull %26)
  call fastcc void @fail(ptr noundef nonnull %4)
  unreachable

28:                                               ; preds = %17, %2
  %29 = phi ptr [ %21, %17 ], [ %10, %2 ]
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %30 = load ptr, ptr @drv.6, align 8
  %31 = call i32 %30(ptr noundef nonnull %5, ptr noundef %29, ptr noundef %1)
  %32 = icmp eq i32 %31, 0
  br i1 %32, label %39, label %33

33:                                               ; preds = %28
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %34 = load ptr, ptr @drv.1, align 8
  %35 = call ptr %34(i32 noundef range(i32 1, 0) %31)
  %36 = icmp eq ptr %35, null
  %37 = select i1 %36, ptr @.str.20, ptr %35
  %38 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.3, ptr noundef nonnull %37)
  call fastcc void @fail(ptr noundef nonnull %3)
  unreachable

39:                                               ; preds = %28
  %40 = load ptr, ptr %5, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %5)
  ret ptr %40
}

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
  %28 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %13, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.4, ptr noundef nonnull %27)
  call fastcc void @fail(ptr noundef nonnull %13)
  unreachable

29:                                               ; preds = %12
  ret void
}

define dso_local ptr @mgpuStreamCreate() local_unnamed_addr {
  %1 = alloca [128 x i8], align 16
  %2 = load i32, ptr @current, align 4
  %3 = sext i32 %2 to i64
  %4 = getelementptr inbounds %struct.device, ptr @devices, i64 %3
  %5 = load ptr, ptr %4, align 8
  %6 = icmp eq ptr %5, null
  br i1 %6, label %7, label %23

7:                                                ; preds = %0
  tail call fastcc void @probe()
  %8 = load i1, ptr @ready, align 4
  br i1 %8, label %11, label %9

9:                                                ; preds = %7
  %10 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %10)
  unreachable

11:                                               ; preds = %7
  %12 = load ptr, ptr @drv.8, align 8
  %13 = tail call i32 %12(ptr noundef nonnull %4, i32 noundef 1)
  %14 = icmp eq i32 %13, 0
  br i1 %14, label %15, label %17

15:                                               ; preds = %11
  %16 = load ptr, ptr %4, align 8
  br label %23

17:                                               ; preds = %11
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  %18 = load ptr, ptr @drv.1, align 8
  %19 = tail call ptr %18(i32 noundef range(i32 1, 0) %13)
  %20 = icmp eq ptr %19, null
  %21 = select i1 %20, ptr @.str.20, ptr %19
  %22 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %1, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.5, ptr noundef nonnull %21)
  call fastcc void @fail(ptr noundef nonnull %1)
  unreachable

23:                                               ; preds = %15, %0
  %24 = phi ptr [ %16, %15 ], [ %5, %0 ]
  ret ptr %24
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
  %11 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.6, ptr noundef nonnull %10)
  call fastcc void @fail(ptr noundef nonnull %2)
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
  %12 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.7, ptr noundef nonnull %11)
  call fastcc void @fail(ptr noundef nonnull %3)
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
  %14 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.8, ptr noundef nonnull %13)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

15:                                               ; preds = %4
  ret void
}

define dso_local nonnull ptr @_mlir_memref_to_llvm_alloc(i64 noundef %0) local_unnamed_addr {
  %2 = alloca ptr, align 8
  %3 = alloca ptr, align 8
  %4 = load i32, ptr @current, align 4
  %5 = sext i32 %4 to i64
  %6 = getelementptr inbounds %struct.device, ptr @devices, i64 %5
  %7 = getelementptr inbounds nuw i8, ptr %6, i64 8
  %8 = load ptr, ptr %7, align 8
  %9 = icmp eq ptr %8, null
  br i1 %9, label %10, label %20

10:                                               ; preds = %1
  tail call fastcc void @probe()
  %11 = load i1, ptr @ready, align 4
  br i1 %11, label %14, label %12

12:                                               ; preds = %10
  %13 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %13)
  unreachable

14:                                               ; preds = %10
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  store ptr null, ptr %3, align 8
  %15 = load ptr, ptr @drv.10, align 8
  %16 = call i32 %15(ptr noundef nonnull %3, i64 noundef 67108864)
  %17 = icmp eq i32 %16, 0
  %18 = load ptr, ptr %3, align 8
  %19 = select i1 %17, ptr %18, ptr null
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  store ptr %19, ptr %7, align 8
  br label %20

20:                                               ; preds = %14, %1
  %21 = phi ptr [ %19, %14 ], [ %8, %1 ]
  %22 = ptrtoint ptr %21 to i64
  %23 = getelementptr inbounds nuw i8, ptr %6, i64 16
  %24 = load i64, ptr %23, align 8
  %25 = add i64 %22, 255
  %26 = add i64 %25, %24
  %27 = and i64 %26, -256
  %28 = sub i64 %27, %22
  %29 = icmp eq ptr %21, null
  %30 = icmp ugt i64 %28, 67108864
  %31 = select i1 %29, i1 true, i1 %30
  %32 = sub nuw nsw i64 67108864, %28
  %33 = icmp ugt i64 %0, %32
  %34 = select i1 %31, i1 true, i1 %33
  br i1 %34, label %38, label %35

35:                                               ; preds = %20
  %36 = add nuw nsw i64 %28, %0
  store i64 %36, ptr %23, align 8
  %37 = inttoptr i64 %27 to ptr
  br label %49

38:                                               ; preds = %20
  call fastcc void @probe()
  %39 = load i1, ptr @ready, align 4
  br i1 %39, label %42, label %40

40:                                               ; preds = %38
  %41 = load ptr, ptr @unusable_reason, align 8
  call fastcc void @unusable(ptr noundef nonnull @LAUNCH, ptr noundef %41)
  unreachable

42:                                               ; preds = %38
  %43 = icmp eq i64 %0, 0
  br i1 %43, label %52, label %44

44:                                               ; preds = %42
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  store ptr null, ptr %2, align 8
  %45 = load ptr, ptr @drv.10, align 8
  %46 = call i32 %45(ptr noundef nonnull %2, i64 noundef %0)
  %47 = icmp eq i32 %46, 0
  %48 = load ptr, ptr %2, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  br i1 %47, label %49, label %52

49:                                               ; preds = %44, %35
  %50 = phi ptr [ %37, %35 ], [ %48, %44 ]
  %51 = icmp eq ptr %50, null
  br i1 %51, label %52, label %53

52:                                               ; preds = %42, %44, %49
  call fastcc void @report(ptr noundef nonnull @ALLOCATION_FAILED, i32 noundef 76)
  unreachable

53:                                               ; preds = %49
  ret ptr %50
}

define dso_local void @_mlir_memref_to_llvm_free(ptr noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = icmp eq ptr %0, null
  br i1 %3, label %26, label %4

4:                                                ; preds = %1
  %5 = load i32, ptr @current, align 4
  %6 = sext i32 %5 to i64
  %7 = getelementptr inbounds %struct.device, ptr @devices, i64 %6
  %8 = getelementptr inbounds nuw i8, ptr %7, i64 8
  %9 = load ptr, ptr %8, align 8
  %10 = ptrtoint ptr %9 to i64
  %11 = icmp ne ptr %9, null
  %12 = ptrtoint ptr %0 to i64
  %13 = sub i64 %12, %10
  %14 = icmp ult i64 %13, 67108864
  %15 = and i1 %11, %14
  br i1 %15, label %26, label %16

16:                                               ; preds = %4
  %17 = load ptr, ptr @drv.12, align 8
  %18 = tail call i32 %17(ptr noundef nonnull %0)
  %19 = icmp eq i32 %18, 0
  br i1 %19, label %26, label %20

20:                                               ; preds = %16
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %21 = load ptr, ptr @drv.1, align 8
  %22 = tail call ptr %21(i32 noundef range(i32 1, 0) %18)
  %23 = icmp eq ptr %22, null
  %24 = select i1 %23, ptr @.str.20, ptr %22
  %25 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.7, ptr noundef nonnull %24)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

26:                                               ; preds = %16, %1, %4
  ret void
}

define dso_local i64 @__neuro_device_mark() local_unnamed_addr {
  %1 = load i32, ptr @current, align 4
  %2 = sext i32 %1 to i64
  %3 = getelementptr inbounds %struct.device, ptr @devices, i64 %2
  %4 = getelementptr inbounds nuw i8, ptr %3, i64 16
  %5 = load i64, ptr %4, align 8
  ret i64 %5
}

define dso_local void @__neuro_device_restore(i64 noundef %0) local_unnamed_addr {
  %2 = load i32, ptr @current, align 4
  %3 = sext i32 %2 to i64
  %4 = getelementptr inbounds %struct.device, ptr @devices, i64 %3
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
  %17 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.10, ptr noundef nonnull %16)
  call fastcc void @fail(ptr noundef nonnull %2)
  unreachable

18:                                               ; preds = %7
  %19 = load ptr, ptr %3, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  ret ptr %19
}

define dso_local ptr @__neuro_device_upload(ptr noundef %0, i64 noundef %1, i32 noundef %2) local_unnamed_addr {
  %4 = alloca [128 x i8], align 16
  %5 = alloca [128 x i8], align 16
  %6 = alloca [256 x i8], align 16
  tail call fastcc void @probe()
  %7 = load i1, ptr @ready, align 4
  br i1 %7, label %10, label %8

8:                                                ; preds = %3
  %9 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %9)
  unreachable

10:                                               ; preds = %3
  %11 = icmp sgt i32 %2, -1
  %12 = load i32, ptr @device_count, align 4
  %13 = icmp slt i32 %2, %12
  %14 = select i1 %11, i1 %13, i1 false
  br i1 %14, label %17, label %15

15:                                               ; preds = %10
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %16 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %6, i64 noundef 256, ptr noundef nonnull @.str.9, i32 noundef %2, i32 noundef %12)
  call fastcc void @report(ptr noundef nonnull %6, i32 noundef %16)
  unreachable

17:                                               ; preds = %10
  %18 = tail call i32 @__neuro_device_switch(i32 noundef %2)
  %19 = tail call ptr @__neuro_device_alloc(i64 noundef %1)
  %20 = tail call ptr @mgpuStreamCreate()
  %21 = load ptr, ptr @drv.13, align 8
  %22 = tail call i32 %21(ptr noundef %19, ptr noundef %0, i64 noundef %1, i32 noundef 4, ptr noundef %20)
  %23 = icmp eq i32 %22, 0
  br i1 %23, label %30, label %24

24:                                               ; preds = %17
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %25 = load ptr, ptr @drv.1, align 8
  %26 = tail call ptr %25(i32 noundef range(i32 1, 0) %22)
  %27 = icmp eq ptr %26, null
  %28 = select i1 %27, ptr @.str.20, ptr %26
  %29 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.8, ptr noundef nonnull %28)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

30:                                               ; preds = %17
  %31 = load ptr, ptr @drv.9, align 8
  %32 = tail call i32 %31(ptr noundef %20)
  %33 = icmp eq i32 %32, 0
  br i1 %33, label %40, label %34

34:                                               ; preds = %30
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %35 = load ptr, ptr @drv.1, align 8
  %36 = tail call ptr %35(i32 noundef range(i32 1, 0) %32)
  %37 = icmp eq ptr %36, null
  %38 = select i1 %37, ptr @.str.20, ptr %36
  %39 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.6, ptr noundef nonnull %38)
  call fastcc void @fail(ptr noundef nonnull %4)
  unreachable

40:                                               ; preds = %30
  %41 = tail call i32 @__neuro_device_switch(i32 noundef %18)
  ret ptr %19
}

define dso_local void @__neuro_device_download(ptr noundef %0, ptr noundef %1, i64 noundef %2, i32 noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = alloca [128 x i8], align 16
  %7 = tail call i32 @__neuro_device_switch(i32 noundef %3)
  %8 = tail call ptr @mgpuStreamCreate()
  %9 = load ptr, ptr @drv.13, align 8
  %10 = tail call i32 %9(ptr noundef %0, ptr noundef %1, i64 noundef %2, i32 noundef 4, ptr noundef %8)
  %11 = icmp eq i32 %10, 0
  br i1 %11, label %18, label %12

12:                                               ; preds = %4
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %13 = load ptr, ptr @drv.1, align 8
  %14 = tail call ptr %13(i32 noundef range(i32 1, 0) %10)
  %15 = icmp eq ptr %14, null
  %16 = select i1 %15, ptr @.str.20, ptr %14
  %17 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %6, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.8, ptr noundef nonnull %16)
  call fastcc void @fail(ptr noundef nonnull %6)
  unreachable

18:                                               ; preds = %4
  %19 = load ptr, ptr @drv.9, align 8
  %20 = tail call i32 %19(ptr noundef %8)
  %21 = icmp eq i32 %20, 0
  br i1 %21, label %28, label %22

22:                                               ; preds = %18
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %23 = load ptr, ptr @drv.1, align 8
  %24 = tail call ptr %23(i32 noundef range(i32 1, 0) %20)
  %25 = icmp eq ptr %24, null
  %26 = select i1 %25, ptr @.str.20, ptr %24
  %27 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.6, ptr noundef nonnull %26)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

28:                                               ; preds = %18
  %29 = tail call i32 @__neuro_device_switch(i32 noundef %7)
  ret void
}

define dso_local void @__neuro_device_free(ptr noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca [128 x i8], align 16
  %5 = tail call i32 @__neuro_device_switch(i32 noundef %1)
  %6 = tail call ptr @mgpuStreamCreate()
  %7 = load ptr, ptr @drv.9, align 8
  %8 = tail call i32 %7(ptr noundef %6)
  %9 = icmp eq i32 %8, 0
  br i1 %9, label %16, label %10

10:                                               ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %11 = load ptr, ptr @drv.1, align 8
  %12 = tail call ptr %11(i32 noundef range(i32 1, 0) %8)
  %13 = icmp eq ptr %12, null
  %14 = select i1 %13, ptr @.str.20, ptr %12
  %15 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %4, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.6, ptr noundef nonnull %14)
  call fastcc void @fail(ptr noundef nonnull %4)
  unreachable

16:                                               ; preds = %2
  %17 = load ptr, ptr @drv.12, align 8
  %18 = tail call i32 %17(ptr noundef %0)
  %19 = icmp eq i32 %18, 0
  br i1 %19, label %26, label %20

20:                                               ; preds = %16
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %21 = load ptr, ptr @drv.1, align 8
  %22 = tail call ptr %21(i32 noundef range(i32 1, 0) %18)
  %23 = icmp eq ptr %22, null
  %24 = select i1 %23, ptr @.str.20, ptr %22
  %25 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.7, ptr noundef nonnull %24)
  call fastcc void @fail(ptr noundef nonnull %3)
  unreachable

26:                                               ; preds = %16
  %27 = tail call i32 @__neuro_device_switch(i32 noundef %5)
  ret void
}

define dso_local ptr @__neuro_device_move(ptr noundef %0, i64 noundef %1, i32 noundef %2, i32 noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = alloca [128 x i8], align 16
  %7 = alloca [128 x i8], align 16
  %8 = alloca [256 x i8], align 16
  tail call fastcc void @probe()
  %9 = load i1, ptr @ready, align 4
  br i1 %9, label %12, label %10

10:                                               ; preds = %4
  %11 = load ptr, ptr @unusable_reason, align 8
  tail call fastcc void @unusable(ptr noundef nonnull @TRANSFER, ptr noundef %11)
  unreachable

12:                                               ; preds = %4
  %13 = icmp sgt i32 %3, -1
  %14 = load i32, ptr @device_count, align 4
  %15 = icmp slt i32 %3, %14
  %16 = select i1 %13, i1 %15, i1 false
  br i1 %16, label %19, label %17

17:                                               ; preds = %12
  call void @llvm.lifetime.start.p0(ptr nonnull %8)
  %18 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %8, i64 noundef 256, ptr noundef nonnull @.str.9, i32 noundef %3, i32 noundef %14)
  call fastcc void @report(ptr noundef nonnull %8, i32 noundef %18)
  unreachable

19:                                               ; preds = %12
  %20 = icmp eq i32 %2, %3
  br i1 %20, label %58, label %21

21:                                               ; preds = %19
  %22 = tail call i32 @__neuro_device_switch(i32 noundef %2)
  %23 = tail call ptr @mgpuStreamCreate()
  %24 = load ptr, ptr @drv.9, align 8
  %25 = tail call i32 %24(ptr noundef %23)
  %26 = icmp eq i32 %25, 0
  br i1 %26, label %33, label %27

27:                                               ; preds = %21
  call void @llvm.lifetime.start.p0(ptr nonnull %7)
  %28 = load ptr, ptr @drv.1, align 8
  %29 = tail call ptr %28(i32 noundef range(i32 1, 0) %25)
  %30 = icmp eq ptr %29, null
  %31 = select i1 %30, ptr @.str.20, ptr %29
  %32 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %7, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.6, ptr noundef nonnull %31)
  call fastcc void @fail(ptr noundef nonnull %7)
  unreachable

33:                                               ; preds = %21
  %34 = tail call i32 @__neuro_device_switch(i32 noundef %3)
  %35 = tail call ptr @__neuro_device_alloc(i64 noundef %1)
  %36 = tail call ptr @mgpuStreamCreate()
  %37 = load ptr, ptr @drv.13, align 8
  %38 = tail call i32 %37(ptr noundef %35, ptr noundef %0, i64 noundef %1, i32 noundef 4, ptr noundef %36)
  %39 = icmp eq i32 %38, 0
  br i1 %39, label %46, label %40

40:                                               ; preds = %33
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %41 = load ptr, ptr @drv.1, align 8
  %42 = tail call ptr %41(i32 noundef range(i32 1, 0) %38)
  %43 = icmp eq ptr %42, null
  %44 = select i1 %43, ptr @.str.20, ptr %42
  %45 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %6, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.8, ptr noundef nonnull %44)
  call fastcc void @fail(ptr noundef nonnull %6)
  unreachable

46:                                               ; preds = %33
  %47 = load ptr, ptr @drv.9, align 8
  %48 = tail call i32 %47(ptr noundef %36)
  %49 = icmp eq i32 %48, 0
  br i1 %49, label %56, label %50

50:                                               ; preds = %46
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %51 = load ptr, ptr @drv.1, align 8
  %52 = tail call ptr %51(i32 noundef range(i32 1, 0) %48)
  %53 = icmp eq ptr %52, null
  %54 = select i1 %53, ptr @.str.20, ptr %52
  %55 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.21, ptr noundef nonnull @.str.6, ptr noundef nonnull %54)
  call fastcc void @fail(ptr noundef nonnull %5)
  unreachable

56:                                               ; preds = %46
  tail call void @__neuro_device_free(ptr noundef %0, i32 noundef %2)
  %57 = tail call i32 @__neuro_device_switch(i32 noundef %22)
  br label %58

58:                                               ; preds = %19, %56
  %59 = phi ptr [ %35, %56 ], [ %0, %19 ]
  ret ptr %59
}

declare ptr @dlopen(ptr noundef, i32 noundef) local_unnamed_addr

declare ptr @dlsym(ptr noundef, ptr noundef) local_unnamed_addr

define internal fastcc void @fail(ptr noundef %0) unnamed_addr {
  %2 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %3 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 256, ptr noundef nonnull @.str.22, ptr noundef %0)
  call fastcc void @report(ptr noundef nonnull %2, i32 noundef %3)
  unreachable
}

declare void @__neuro_gpu_panic(ptr noundef, i64 noundef) local_unnamed_addr

define internal fastcc void @unusable(ptr noundef %0, ptr noundef %1) unnamed_addr {
  %3 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %4 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef nonnull @.str.25, ptr noundef %0, ptr noundef %1)
  call fastcc void @report(ptr noundef nonnull %3, i32 noundef %4)
  unreachable
}

declare noalias noundef ptr @calloc(i64 noundef, i64 noundef) local_unnamed_addr

declare i32 @llvm.smax.i32(i32, i32)

declare i32 @llvm.umin.i32(i32, i32)

declare i64 @llvm.umax.i64(i64, i64)

declare ptr @llvm.load.relative.i64(ptr, i64)

