;
; MLIR's GPU runtime ABI (`mgpu*`) over the CUDA driver API, linked into every module
; that runs `@gpu` bodies. The CUDA driver is opened with dlopen, so a binary without a
; usable NVIDIA GPU starts, then panics from the first module load (a global
; constructor) saying why. Failures call `__neuro_gpu_panic`, which the LLVM backend
; defines as an ordinary runtime panic.
;
; Generated from compiler/llvm-backend/src/codegen/gpu_runtime.c with clang -O2
; -emit-llvm (the exact command is in that file), then stripped of the target
; datalayout and triple, attribute groups and metadata. Regenerate it the same way.
;
%struct.anon = type { ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr }
%struct.anon.0 = type { ptr, ptr }

@ready = internal unnamed_addr global i1 false, align 4
@cu = internal global %struct.anon zeroinitializer, align 8
@.str = private unnamed_addr constant [20 x i8] c"cuModuleGetFunction\00", align 1
@.str.1 = private unnamed_addr constant [15 x i8] c"cuLaunchKernel\00", align 1
@shared_stream = internal global ptr null, align 8
@.str.2 = private unnamed_addr constant [15 x i8] c"cuStreamCreate\00", align 1
@.str.3 = private unnamed_addr constant [20 x i8] c"cuStreamSynchronize\00", align 1
@.str.4 = private unnamed_addr constant [10 x i8] c"cuMemFree\00", align 1
@.str.5 = private unnamed_addr constant [14 x i8] c"cuMemcpyAsync\00", align 1
@.str.6 = private unnamed_addr constant [17 x i8] c"cuModuleLoadData\00", align 1
@.str.7 = private unnamed_addr constant [18 x i8] c"%s failed with %s\00", align 1
@.str.8 = private unnamed_addr constant [14 x i8] c"GPU error: %s\00", align 1
@.str.9 = private unnamed_addr constant [22 x i8] c"an unknown CUDA error\00", align 1
@.str.10 = private unnamed_addr constant [13 x i8] c"libcuda.so.1\00", align 1
@.str.11 = private unnamed_addr constant [51 x i8] c"the CUDA driver (libcuda.so.1) could not be loaded\00", align 1
@entry_points = internal unnamed_addr constant [16 x %struct.anon.0] [%struct.anon.0 { ptr @.str.18, ptr @cu }, %struct.anon.0 { ptr @.str.19, ptr getelementptr (i8, ptr @cu, i64 8) }, %struct.anon.0 { ptr @.str.20, ptr getelementptr (i8, ptr @cu, i64 16) }, %struct.anon.0 { ptr @.str.14, ptr getelementptr (i8, ptr @cu, i64 24) }, %struct.anon.0 { ptr @.str.15, ptr getelementptr (i8, ptr @cu, i64 32) }, %struct.anon.0 { ptr @.str.16, ptr getelementptr (i8, ptr @cu, i64 40) }, %struct.anon.0 { ptr @.str.6, ptr getelementptr (i8, ptr @cu, i64 48) }, %struct.anon.0 { ptr @.str.21, ptr getelementptr (i8, ptr @cu, i64 56) }, %struct.anon.0 { ptr @.str, ptr getelementptr (i8, ptr @cu, i64 64) }, %struct.anon.0 { ptr @.str.1, ptr getelementptr (i8, ptr @cu, i64 72) }, %struct.anon.0 { ptr @.str.2, ptr getelementptr (i8, ptr @cu, i64 80) }, %struct.anon.0 { ptr @.str.3, ptr getelementptr (i8, ptr @cu, i64 88) }, %struct.anon.0 { ptr @.str.22, ptr getelementptr (i8, ptr @cu, i64 96) }, %struct.anon.0 { ptr @.str.23, ptr getelementptr (i8, ptr @cu, i64 104) }, %struct.anon.0 { ptr @.str.24, ptr getelementptr (i8, ptr @cu, i64 112) }, %struct.anon.0 { ptr @.str.5, ptr getelementptr (i8, ptr @cu, i64 120) }], align 16
@.str.12 = private unnamed_addr constant [27 x i8] c"the CUDA driver is too old\00", align 1
@.str.13 = private unnamed_addr constant [34 x i8] c"the CUDA driver reports no device\00", align 1
@.str.14 = private unnamed_addr constant [12 x i8] c"cuDeviceGet\00", align 1
@.str.15 = private unnamed_addr constant [25 x i8] c"cuDevicePrimaryCtxRetain\00", align 1
@.str.16 = private unnamed_addr constant [16 x i8] c"cuCtxSetCurrent\00", align 1
@.str.17 = private unnamed_addr constant [51 x i8] c"`@gpu` needs an NVIDIA GPU, and none is usable: %s\00", align 1
@.str.18 = private unnamed_addr constant [7 x i8] c"cuInit\00", align 1
@.str.19 = private unnamed_addr constant [15 x i8] c"cuGetErrorName\00", align 1
@.str.20 = private unnamed_addr constant [17 x i8] c"cuDeviceGetCount\00", align 1
@.str.21 = private unnamed_addr constant [15 x i8] c"cuModuleUnload\00", align 1
@.str.22 = private unnamed_addr constant [14 x i8] c"cuMemAlloc_v2\00", align 1
@.str.23 = private unnamed_addr constant [18 x i8] c"cuMemAllocManaged\00", align 1
@.str.24 = private unnamed_addr constant [13 x i8] c"cuMemFree_v2\00", align 1

define dso_local ptr @mgpuModuleLoad(ptr noundef %0, i64 noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca ptr, align 8
  tail call fastcc void @ensure_ready()
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %5 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 48), align 8
  %6 = call i32 %5(ptr noundef nonnull %4, ptr noundef %0)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.6, ptr noundef %9)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %3)
  unreachable

11:                                               ; preds = %2
  %12 = load ptr, ptr %4, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  ret ptr %12
}

define dso_local ptr @mgpuModuleLoadJIT(ptr noundef %0, i32 noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca ptr, align 8
  tail call fastcc void @ensure_ready()
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %5 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 48), align 8
  %6 = call i32 %5(ptr noundef nonnull %4, ptr noundef %0)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.6, ptr noundef %9)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %3)
  unreachable

11:                                               ; preds = %2
  %12 = load ptr, ptr %4, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  ret ptr %12
}

define dso_local void @mgpuModuleUnload(ptr noundef %0) local_unnamed_addr {
  %2 = load i1, ptr @ready, align 4
  br i1 %2, label %3, label %6

3:                                                ; preds = %1
  %4 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 56), align 8
  %5 = tail call i32 %4(ptr noundef %0)
  br label %6

6:                                                ; preds = %3, %1
  ret void
}

define dso_local ptr @mgpuModuleGetFunction(ptr noundef %0, ptr noundef %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = alloca ptr, align 8
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  %5 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 64), align 8
  %6 = call i32 %5(ptr noundef nonnull %4, ptr noundef %0, ptr noundef %1)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str, ptr noundef %9)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %3)
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
  %14 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 72), align 8
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
  %25 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %13, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.1, ptr noundef %24)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %13)
  unreachable

26:                                               ; preds = %12
  ret void
}

define dso_local ptr @mgpuStreamCreate() local_unnamed_addr {
  %1 = alloca [128 x i8], align 16
  %2 = load ptr, ptr @shared_stream, align 8
  %3 = icmp eq ptr %2, null
  br i1 %3, label %4, label %13

4:                                                ; preds = %0
  tail call fastcc void @ensure_ready()
  %5 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 80), align 8
  %6 = tail call i32 %5(ptr noundef nonnull @shared_stream, i32 noundef 1)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %8, label %10

8:                                                ; preds = %4
  %9 = load ptr, ptr @shared_stream, align 8
  br label %13

10:                                               ; preds = %4
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  %11 = tail call fastcc ptr @error_name(i32 noundef %6)
  %12 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %1, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.2, ptr noundef %11)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %1)
  unreachable

13:                                               ; preds = %8, %0
  %14 = phi ptr [ %9, %8 ], [ %2, %0 ]
  ret ptr %14
}

define internal fastcc void @ensure_ready() unnamed_addr {
  %1 = alloca [128 x i8], align 16
  %2 = alloca [128 x i8], align 16
  %3 = alloca [128 x i8], align 16
  %4 = alloca i32, align 4
  %5 = alloca i32, align 4
  %6 = alloca ptr, align 8
  %7 = load i1, ptr @ready, align 4
  br i1 %7, label %62, label %8

8:                                                ; preds = %0
  %9 = tail call ptr @dlopen(ptr noundef nonnull @.str.10, i32 noundef 2)
  %10 = icmp eq ptr %9, null
  br i1 %10, label %11, label %19

11:                                               ; preds = %8
  tail call fastcc void @unusable(ptr noundef nonnull @.str.11)
  unreachable

12:                                               ; preds = %19
  %13 = add nuw nsw i64 %20, 1
  %14 = icmp eq i64 %13, 16
  br i1 %14, label %15, label %19

15:                                               ; preds = %12
  %16 = load ptr, ptr @cu, align 8
  %17 = tail call i32 %16(i32 noundef 0)
  %18 = icmp eq i32 %17, 0
  br i1 %18, label %30, label %28

19:                                               ; preds = %8, %12
  %20 = phi i64 [ %13, %12 ], [ 0, %8 ]
  %21 = getelementptr inbounds nuw %struct.anon.0, ptr @entry_points, i64 %20
  %22 = load ptr, ptr %21, align 16
  %23 = tail call ptr @dlsym(ptr noundef nonnull %9, ptr noundef %22)
  %24 = getelementptr inbounds nuw i8, ptr %21, i64 8
  %25 = load ptr, ptr %24, align 8
  store ptr %23, ptr %25, align 8
  %26 = icmp eq ptr %23, null
  br i1 %26, label %27, label %12

27:                                               ; preds = %19
  tail call fastcc void @unusable(ptr noundef nonnull @.str.12)
  unreachable

28:                                               ; preds = %15
  %29 = tail call fastcc ptr @error_name(i32 noundef %17)
  tail call fastcc void @unusable(ptr noundef %29)
  unreachable

30:                                               ; preds = %15
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  store i32 0, ptr %4, align 4
  %31 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 16), align 8
  %32 = call i32 %31(ptr noundef nonnull %4)
  %33 = icmp ne i32 %32, 0
  %34 = load i32, ptr %4, align 4
  %35 = icmp eq i32 %34, 0
  %36 = select i1 %33, i1 true, i1 %35
  br i1 %36, label %37, label %38

37:                                               ; preds = %30
  call fastcc void @unusable(ptr noundef nonnull @.str.13)
  unreachable

38:                                               ; preds = %30
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %39 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 24), align 8
  %40 = call i32 %39(ptr noundef nonnull %5, i32 noundef 0)
  %41 = icmp eq i32 %40, 0
  br i1 %41, label %45, label %42

42:                                               ; preds = %38
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %43 = call fastcc ptr @error_name(i32 noundef %40)
  %44 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.14, ptr noundef %43)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %3)
  unreachable

45:                                               ; preds = %38
  %46 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 32), align 8
  %47 = load i32, ptr %5, align 4
  %48 = call i32 %46(ptr noundef nonnull %6, i32 noundef %47)
  %49 = icmp eq i32 %48, 0
  br i1 %49, label %53, label %50

50:                                               ; preds = %45
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %51 = call fastcc ptr @error_name(i32 noundef %48)
  %52 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.15, ptr noundef %51)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %2)
  unreachable

53:                                               ; preds = %45
  %54 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 40), align 8
  %55 = load ptr, ptr %6, align 8
  %56 = call i32 %54(ptr noundef %55)
  %57 = icmp eq i32 %56, 0
  br i1 %57, label %61, label %58

58:                                               ; preds = %53
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  %59 = call fastcc ptr @error_name(i32 noundef %56)
  %60 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %1, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.16, ptr noundef %59)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %1)
  unreachable

61:                                               ; preds = %53
  store i1 true, ptr @ready, align 4
  call void @llvm.lifetime.end.p0(ptr nonnull %6)
  call void @llvm.lifetime.end.p0(ptr nonnull %5)
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  br label %62

62:                                               ; preds = %0, %61
  ret void
}

define dso_local void @mgpuStreamSynchronize(ptr noundef %0) local_unnamed_addr {
  %2 = alloca [128 x i8], align 16
  %3 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 88), align 8
  %4 = tail call i32 %3(ptr noundef %0)
  %5 = icmp eq i32 %4, 0
  br i1 %5, label %9, label %6

6:                                                ; preds = %1
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  %7 = tail call fastcc ptr @error_name(i32 noundef %4)
  %8 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %2, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.3, ptr noundef %7)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %2)
  unreachable

9:                                                ; preds = %1
  ret void
}

define dso_local void @mgpuStreamDestroy(ptr noundef readnone captures(none) %0) local_unnamed_addr {
  ret void
}

define dso_local ptr @mgpuMemAlloc(i64 noundef %0, ptr noundef readnone captures(none) %1, i8 noundef zeroext %2) local_unnamed_addr {
  %4 = alloca i64, align 8
  tail call fastcc void @ensure_ready()
  %5 = icmp eq i64 %0, 0
  br i1 %5, label %20, label %6

6:                                                ; preds = %3
  call void @llvm.lifetime.start.p0(ptr nonnull %4)
  store i64 0, ptr %4, align 8
  %7 = icmp eq i8 %2, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %6
  %9 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 104), align 8
  %10 = call i32 %9(ptr noundef nonnull %4, i64 noundef %0, i32 noundef 1)
  br label %14

11:                                               ; preds = %6
  %12 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 96), align 8
  %13 = call i32 %12(ptr noundef nonnull %4, i64 noundef %0)
  br label %14

14:                                               ; preds = %11, %8
  %15 = phi i32 [ %10, %8 ], [ %13, %11 ]
  %16 = icmp eq i32 %15, 0
  %17 = load i64, ptr %4, align 8
  %18 = inttoptr i64 %17 to ptr
  %19 = select i1 %16, ptr %18, ptr null
  call void @llvm.lifetime.end.p0(ptr nonnull %4)
  br label %20

20:                                               ; preds = %3, %14
  %21 = phi ptr [ %19, %14 ], [ null, %3 ]
  ret ptr %21
}

define dso_local void @mgpuMemFree(ptr noundef %0, ptr noundef readnone captures(none) %1) local_unnamed_addr {
  %3 = alloca [128 x i8], align 16
  %4 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 112), align 8
  %5 = ptrtoint ptr %0 to i64
  %6 = tail call i32 %4(i64 noundef %5)
  %7 = icmp eq i32 %6, 0
  br i1 %7, label %11, label %8

8:                                                ; preds = %2
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %9 = tail call fastcc ptr @error_name(i32 noundef %6)
  %10 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.4, ptr noundef %9)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %3)
  unreachable

11:                                               ; preds = %2
  ret void
}

define dso_local void @mgpuMemcpy(ptr noundef %0, ptr noundef %1, i64 noundef %2, ptr noundef %3) local_unnamed_addr {
  %5 = alloca [128 x i8], align 16
  %6 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 120), align 8
  %7 = ptrtoint ptr %0 to i64
  %8 = ptrtoint ptr %1 to i64
  %9 = tail call i32 %6(i64 noundef %7, i64 noundef %8, i64 noundef %2, ptr noundef %3)
  %10 = icmp eq i32 %9, 0
  br i1 %10, label %14, label %11

11:                                               ; preds = %4
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %12 = tail call fastcc ptr @error_name(i32 noundef %9)
  %13 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %5, i64 noundef 128, ptr noundef nonnull @.str.7, ptr noundef nonnull @.str.5, ptr noundef %12)
  call fastcc void @fail(ptr noundef nonnull @.str.8, ptr noundef nonnull %5)
  unreachable

14:                                               ; preds = %4
  ret void
}

declare noundef i32 @snprintf(ptr noalias noundef writeonly captures(none), i64 noundef, ptr noundef readonly captures(none), ...) local_unnamed_addr

define internal fastcc ptr @error_name(i32 noundef range(i32 1, 0) %0) unnamed_addr {
  %2 = alloca ptr, align 8
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  store ptr null, ptr %2, align 8
  %3 = load ptr, ptr getelementptr inbounds nuw (i8, ptr @cu, i64 8), align 8
  %4 = call i32 %3(i32 noundef %0, ptr noundef nonnull %2)
  %5 = icmp ne i32 %4, 0
  %6 = load ptr, ptr %2, align 8
  %7 = icmp eq ptr %6, null
  %8 = select i1 %5, i1 true, i1 %7
  %9 = select i1 %8, ptr @.str.9, ptr %6
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  ret ptr %9
}

define internal fastcc void @fail(ptr noundef readonly captures(none) %0, ptr noundef %1) unnamed_addr {
  %3 = alloca [256 x i8], align 16
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  %4 = call i32 (ptr, i64, ptr, ...) @snprintf(ptr noundef nonnull dereferenceable(1) %3, i64 noundef 256, ptr noundef %0, ptr noundef %1)
  %5 = tail call i32 @llvm.smax.i32(i32 %4, i32 0)
  %6 = tail call i32 @llvm.umin.i32(i32 %5, i32 255)
  %7 = zext nneg i32 %6 to i64
  call void @__neuro_gpu_panic(ptr noundef nonnull %3, i64 noundef %7)
  unreachable
}

declare void @__neuro_gpu_panic(ptr noundef, i64 noundef) local_unnamed_addr

declare ptr @dlopen(ptr noundef, i32 noundef) local_unnamed_addr

define internal fastcc void @unusable(ptr noundef %0) unnamed_addr {
  tail call fastcc void @fail(ptr noundef nonnull @.str.17, ptr noundef %0)
  unreachable
}

declare ptr @dlsym(ptr noundef, ptr noundef) local_unnamed_addr

declare i32 @llvm.smax.i32(i32, i32)

declare i32 @llvm.umin.i32(i32, i32)
