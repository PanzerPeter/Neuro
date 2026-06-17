;
; Self-contained soft-float half/bfloat conversion builtins.
;
; LLVM lowers fpext/fptrunc on `half`/`bfloat` (and f16/bf16 comparisons, which
; widen to f32 first) to these runtime calls on targets without native
; half-precision instructions. On Linux/macOS the definitions come from
; libgcc/compiler-rt; on Windows no such runtime is linked (lld-link / MSVC),
; so we emit our own and link them into the module. Linkage is `weak_odr` so a
; platform runtime, if present, may still override.
;
; Implementations are integer-only (no fpext/fptrunc on half, which would
; recursively re-invoke these libcalls). Exhaustively verified against clang's
; native _Float16/__bf16: f32<->f16 and f32->bf16 over all 2^32 inputs,
; f16->f32 over all 2^16, and the f64 paths over 200M random inputs — zero
; mismatches.
;
; Generated from compiler/llvm-backend/src/softfloat/reference.c via clang
; -O2 -emit-llvm, then stripped of target-specific datalayout/triple/attributes
; and marked weak_odr. Regenerate with that command if LLVM's IR syntax changes.
;
; Function Attrs: mustprogress nofree norecurse nosync nounwind sspstrong willreturn memory(none)
define weak_odr float @__extendhfsf2(half %0) {
  %2 = tail call half @llvm.fabs.f16(half %0)
  %3 = bitcast half %2 to i16
  %4 = zext nneg i16 %3 to i32
  %5 = lshr i32 %4, 10
  %6 = and i32 %4, 1023
  switch i32 %5, label %20 [
    i32 31, label %7
    i32 0, label %10
  ]

7:                                                ; preds = %1
  %8 = shl nuw nsw i32 %4, 13
  %9 = or i32 %8, 2139095040
  br label %25

10:                                               ; preds = %1
  %11 = icmp eq i32 %6, 0
  br i1 %11, label %25, label %12

12:                                               ; preds = %10
  %13 = tail call range(i32 22, 33) i32 @llvm.ctlz.i32(i32 %6, i1 true)
  %14 = add nsw i32 %13, -8
  %15 = shl i32 %4, %14
  %16 = and i32 %15, 8388607
  %17 = shl nuw nsw i32 %13, 23
  %18 = sub nsw i32 %16, %17
  %19 = add nsw i32 %18, 1124073472
  br label %25

20:                                               ; preds = %1
  %21 = shl nuw nsw i32 %5, 23
  %22 = add nuw nsw i32 %21, 939524096
  %23 = shl nuw nsw i32 %6, 13
  %24 = or disjoint i32 %22, %23
  br label %25

25:                                               ; preds = %7, %10, %12, %20
  %26 = phi i32 [ %9, %7 ], [ %24, %20 ], [ %19, %12 ], [ 0, %10 ]
  %27 = bitcast half %0 to i16
  %28 = and i16 %27, -32768
  %29 = zext i16 %28 to i32
  %30 = shl nuw i32 %29, 16
  %31 = or i32 %26, %30
  %32 = bitcast i32 %31 to float
  ret float %32
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind sspstrong willreturn memory(none)
define weak_odr half @__truncsfhf2(float %0) {
  %2 = bitcast float %0 to i32
  %3 = zext i32 %2 to i64
  %4 = and i64 %3, 2147483647
  %5 = icmp samesign ult i64 %4, 1199570944
  %6 = add nsw i64 %4, -947912704
  %7 = icmp ult i64 %6, 251658240
  br i1 %7, label %8, label %21

8:                                                ; preds = %1
  %9 = lshr i32 %2, 13
  %10 = trunc i32 %9 to i16
  %11 = add nsw i16 %10, 16384
  %12 = and i64 %3, 8191
  %13 = icmp samesign ugt i64 %12, 4096
  br i1 %13, label %14, label %16

14:                                               ; preds = %8
  %15 = add nsw i16 %10, 16385
  br label %55

16:                                               ; preds = %8
  %17 = icmp eq i64 %12, 4096
  br i1 %17, label %18, label %55

18:                                               ; preds = %16
  %19 = and i16 %10, 1
  %20 = add nuw nsw i16 %11, %19
  br label %55

21:                                               ; preds = %1
  %22 = icmp samesign ugt i64 %4, 2139095040
  br i1 %22, label %23, label %28

23:                                               ; preds = %21
  %24 = lshr i32 %2, 13
  %25 = trunc i32 %24 to i16
  %26 = and i16 %25, 511
  %27 = or disjoint i16 %26, 32256
  br label %55

28:                                               ; preds = %21
  br i1 %5, label %29, label %55

29:                                               ; preds = %28
  %30 = icmp samesign ugt i64 %4, 754974719
  br i1 %30, label %31, label %55

31:                                               ; preds = %29
  %32 = lshr i64 %4, 23
  %33 = sub nsw i64 113, %32
  %34 = and i64 %3, 8388607
  %35 = or disjoint i64 %34, 8388608
  %36 = and i64 %33, 4294967295
  %37 = shl nsw i64 -1, %36
  %38 = xor i64 %37, -1
  %39 = and i64 %35, %38
  %40 = icmp ne i64 %39, 0
  %41 = lshr i64 %35, %36
  %42 = zext i1 %40 to i64
  %43 = lshr i64 %41, 13
  %44 = trunc nuw nsw i64 %43 to i16
  %45 = and i64 %41, 8191
  %46 = or i64 %45, %42
  %47 = icmp samesign ugt i64 %46, 4096
  br i1 %47, label %48, label %50

48:                                               ; preds = %31
  %49 = add nuw nsw i16 %44, 1
  br label %55

50:                                               ; preds = %31
  %51 = icmp eq i64 %46, 4096
  br i1 %51, label %52, label %55

52:                                               ; preds = %50
  %53 = and i16 %44, 1
  %54 = add nuw nsw i16 %53, %44
  br label %55

55:                                               ; preds = %28, %14, %16, %18, %23, %29, %48, %50, %52
  %56 = phi i16 [ %11, %16 ], [ %27, %23 ], [ %44, %50 ], [ %15, %14 ], [ %20, %18 ], [ 0, %29 ], [ %49, %48 ], [ %54, %52 ], [ 31744, %28 ]
  %57 = lshr i32 %2, 16
  %58 = trunc nuw i32 %57 to i16
  %59 = and i16 %58, -32768
  %60 = or i16 %56, %59
  %61 = bitcast i16 %60 to half
  ret half %61
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind sspstrong willreturn memory(none)
define weak_odr half @__truncdfhf2(double %0) {
  %2 = bitcast double %0 to i64
  %3 = tail call double @llvm.fabs.f64(double %0)
  %4 = bitcast double %3 to i64
  %5 = icmp samesign ult i64 %4, 4679240012837945344
  %6 = add nsw i64 %4, -4544132024016830464
  %7 = icmp ult i64 %6, 135107988821114880
  br i1 %7, label %8, label %21

8:                                                ; preds = %1
  %9 = lshr i64 %4, 42
  %10 = trunc i64 %9 to i16
  %11 = add nsw i16 %10, 16384
  %12 = and i64 %4, 4398046511103
  %13 = icmp samesign ugt i64 %12, 2199023255552
  br i1 %13, label %14, label %16

14:                                               ; preds = %8
  %15 = add nsw i16 %10, 16385
  br label %55

16:                                               ; preds = %8
  %17 = icmp eq i64 %12, 2199023255552
  br i1 %17, label %18, label %55

18:                                               ; preds = %16
  %19 = and i16 %10, 1
  %20 = add nuw nsw i16 %11, %19
  br label %55

21:                                               ; preds = %1
  %22 = icmp samesign ugt i64 %4, 9218868437227405312
  br i1 %22, label %23, label %28

23:                                               ; preds = %21
  %24 = lshr i64 %4, 42
  %25 = trunc i64 %24 to i16
  %26 = and i16 %25, 511
  %27 = or disjoint i16 %26, 32256
  br label %55

28:                                               ; preds = %21
  br i1 %5, label %29, label %55

29:                                               ; preds = %28
  %30 = icmp samesign ugt i64 %4, 4309944843393564671
  br i1 %30, label %31, label %55

31:                                               ; preds = %29
  %32 = lshr i64 %4, 52
  %33 = sub nsw i64 1009, %32
  %34 = and i64 %2, 4503599627370495
  %35 = or disjoint i64 %34, 4503599627370496
  %36 = and i64 %33, 4294967295
  %37 = shl nsw i64 -1, %36
  %38 = xor i64 %37, -1
  %39 = and i64 %35, %38
  %40 = icmp ne i64 %39, 0
  %41 = lshr i64 %35, %36
  %42 = zext i1 %40 to i64
  %43 = lshr i64 %41, 42
  %44 = trunc nuw nsw i64 %43 to i16
  %45 = and i64 %41, 4398046511103
  %46 = or i64 %45, %42
  %47 = icmp samesign ugt i64 %46, 2199023255552
  br i1 %47, label %48, label %50

48:                                               ; preds = %31
  %49 = add nuw nsw i16 %44, 1
  br label %55

50:                                               ; preds = %31
  %51 = icmp eq i64 %46, 2199023255552
  br i1 %51, label %52, label %55

52:                                               ; preds = %50
  %53 = and i16 %44, 1
  %54 = add nuw nsw i16 %53, %44
  br label %55

55:                                               ; preds = %28, %14, %16, %18, %23, %29, %48, %50, %52
  %56 = phi i16 [ %11, %16 ], [ %27, %23 ], [ %44, %50 ], [ %15, %14 ], [ %20, %18 ], [ 0, %29 ], [ %49, %48 ], [ %54, %52 ], [ 31744, %28 ]
  %57 = lshr i64 %2, 48
  %58 = trunc nuw i64 %57 to i16
  %59 = and i16 %58, -32768
  %60 = or i16 %56, %59
  %61 = bitcast i16 %60 to half
  ret half %61
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind sspstrong willreturn memory(none)
define weak_odr bfloat @__truncsfbf2(float %0) {
  %2 = bitcast float %0 to i32
  %3 = tail call float @llvm.fabs.f32(float %0)
  %4 = bitcast float %3 to i32
  %5 = icmp samesign ugt i32 %4, 2139095040
  %6 = lshr i32 %2, 16
  br i1 %5, label %7, label %10

7:                                                ; preds = %1
  %8 = trunc nuw i32 %6 to i16
  %9 = or i16 %8, 64
  br label %16

10:                                               ; preds = %1
  %11 = and i32 %6, 1
  %12 = add i32 %2, 32767
  %13 = add i32 %12, %11
  %14 = lshr i32 %13, 16
  %15 = trunc nuw i32 %14 to i16
  br label %16

16:                                               ; preds = %7, %10
  %17 = phi i16 [ %9, %7 ], [ %15, %10 ]
  %18 = bitcast i16 %17 to bfloat
  ret bfloat %18
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind sspstrong willreturn memory(none)
define weak_odr bfloat @__truncdfbf2(double %0) {
  %2 = bitcast double %0 to i64
  %3 = tail call double @llvm.fabs.f64(double %0)
  %4 = bitcast double %3 to i64
  %5 = icmp samesign ult i64 %4, 5183643171103440896
  %6 = add nsw i64 %4, -4039728865751334912
  %7 = icmp ult i64 %6, 1143914305352105984
  br i1 %7, label %8, label %21

8:                                                ; preds = %1
  %9 = lshr i64 %4, 45
  %10 = trunc i64 %9 to i16
  %11 = add nsw i16 %10, 16384
  %12 = and i64 %4, 35184372088831
  %13 = icmp samesign ugt i64 %12, 17592186044416
  br i1 %13, label %14, label %16

14:                                               ; preds = %8
  %15 = add nsw i16 %10, 16385
  br label %55

16:                                               ; preds = %8
  %17 = icmp eq i64 %12, 17592186044416
  br i1 %17, label %18, label %55

18:                                               ; preds = %16
  %19 = and i16 %10, 1
  %20 = add nuw nsw i16 %11, %19
  br label %55

21:                                               ; preds = %1
  %22 = icmp samesign ugt i64 %4, 9218868437227405312
  br i1 %22, label %23, label %28

23:                                               ; preds = %21
  %24 = lshr i64 %4, 45
  %25 = trunc i64 %24 to i16
  %26 = and i16 %25, 63
  %27 = or disjoint i16 %26, 32704
  br label %55

28:                                               ; preds = %21
  br i1 %5, label %29, label %55

29:                                               ; preds = %28
  %30 = icmp samesign ugt i64 %4, 3805541685128069119
  br i1 %30, label %31, label %55

31:                                               ; preds = %29
  %32 = lshr i64 %4, 52
  %33 = sub nsw i64 897, %32
  %34 = and i64 %2, 4503599627370495
  %35 = or disjoint i64 %34, 4503599627370496
  %36 = and i64 %33, 4294967295
  %37 = shl nsw i64 -1, %36
  %38 = xor i64 %37, -1
  %39 = and i64 %35, %38
  %40 = icmp ne i64 %39, 0
  %41 = lshr i64 %35, %36
  %42 = zext i1 %40 to i64
  %43 = lshr i64 %41, 45
  %44 = trunc nuw nsw i64 %43 to i16
  %45 = and i64 %41, 35184372088831
  %46 = or i64 %45, %42
  %47 = icmp samesign ugt i64 %46, 17592186044416
  br i1 %47, label %48, label %50

48:                                               ; preds = %31
  %49 = add nuw nsw i16 %44, 1
  br label %55

50:                                               ; preds = %31
  %51 = icmp eq i64 %46, 17592186044416
  br i1 %51, label %52, label %55

52:                                               ; preds = %50
  %53 = and i16 %44, 1
  %54 = add nuw nsw i16 %53, %44
  br label %55

55:                                               ; preds = %28, %14, %16, %18, %23, %29, %48, %50, %52
  %56 = phi i16 [ %11, %16 ], [ %27, %23 ], [ %44, %50 ], [ %15, %14 ], [ %20, %18 ], [ 0, %29 ], [ %49, %48 ], [ %54, %52 ], [ 32640, %28 ]
  %57 = lshr i64 %2, 48
  %58 = trunc nuw i64 %57 to i16
  %59 = and i16 %58, -32768
  %60 = or i16 %56, %59
  %61 = bitcast i16 %60 to bfloat
  ret bfloat %61
}

; Function Attrs: mustprogress nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare i32 @llvm.ctlz.i32(i32, i1 immarg)

; Function Attrs: nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none)
declare half @llvm.fabs.f16(half)

; Function Attrs: nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none)
declare double @llvm.fabs.f64(double)

; Function Attrs: nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none)
declare float @llvm.fabs.f32(float)
