;
; The operating-system runtime behind `std::fs`, `std::env`, `std::io::read_line` and
; `std::time`, linked into a module that calls it. Generated from os_runtime.c by
; tools/regen_gpu_runtime.sh; do not edit by hand.
;

@.str = private unnamed_addr constant [26 x i8] c"the path holds a NUL byte\00", align 1
@.str.1 = private unnamed_addr constant [3 x i8] c"wb\00", align 1
@__neuro_argc = dso_local local_unnamed_addr global i32 0, align 4
@.str.2 = private unnamed_addr constant [1 x i8] zeroinitializer, align 1
@__neuro_argv = dso_local local_unnamed_addr global ptr null, align 8
@stdin_start = internal unnamed_addr global i64 0, align 8
@stdin_end = internal unnamed_addr global i64 0, align 8
@stdin_closed = internal unnamed_addr global i1 false, align 4
@stdin_buffer = internal global [65536 x i8] zeroinitializer, align 16
@.str.3 = private unnamed_addr constant [14 x i8] c"out of memory\00", align 1
@.str.4 = private unnamed_addr constant [28 x i8] c"the line is not valid UTF-8\00", align 1
@__neuro_clock_gettime = dso_local local_unnamed_addr global ptr null, align 8
@__neuro_clock_id = dso_local local_unnamed_addr global i32 0, align 4
@__neuro_query_counter = dso_local local_unnamed_addr global ptr null, align 8
@__neuro_query_frequency = dso_local local_unnamed_addr global ptr null, align 8
@__neuro_errno_location = dso_local local_unnamed_addr global ptr null, align 8
@.str.5 = private unnamed_addr constant [3 x i8] c"rb\00", align 1
@.str.6 = private unnamed_addr constant [28 x i8] c"the file is not valid UTF-8\00", align 1

define dso_local range(i32 0, 5) i32 @__neuro_read_text(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, ptr nofree noundef writeonly captures(none) %2, ptr nofree noundef writeonly captures(none) %3) local_unnamed_addr {
  %5 = tail call fastcc i32 @read_file(ptr noundef %0, i64 noundef %1, i32 noundef 1, ptr noundef %2, ptr noundef %3)
  ret i32 %5
}

define internal fastcc range(i32 0, 5) i32 @read_file(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, i32 noundef range(i32 0, 2) %2, ptr nofree noundef writeonly captures(none) %3, ptr nofree noundef writeonly captures(none) %4) unnamed_addr {
  %6 = tail call fastcc ptr @c_string(ptr noundef %0, i64 noundef %1)
  %7 = icmp eq ptr %6, null
  br i1 %7, label %8, label %10

8:                                                ; preds = %5
  %9 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str, ptr noundef %3, ptr noundef %4)
  br label %59

10:                                               ; preds = %5
  %11 = tail call ptr @fopen(ptr noundef nonnull %6, ptr noundef nonnull @.str.5)
  tail call void @free(ptr noundef nonnull %6)
  %12 = icmp eq ptr %11, null
  br i1 %12, label %13, label %15

13:                                               ; preds = %10
  %14 = tail call fastcc i32 @fail_errno(ptr noundef %3, ptr noundef %4)
  br label %59

15:                                               ; preds = %10
  %16 = tail call ptr @malloc(i64 noundef 65536)
  br label %17

17:                                               ; preds = %29, %15
  %18 = phi i64 [ 0, %15 ], [ %35, %29 ]
  %19 = phi ptr [ %16, %15 ], [ %30, %29 ]
  %20 = phi i64 [ 65536, %15 ], [ %31, %29 ]
  %21 = icmp eq ptr %19, null
  br i1 %21, label %37, label %22

22:                                               ; preds = %17
  %23 = icmp eq i64 %18, %20
  br i1 %23, label %24, label %29

24:                                               ; preds = %22
  %25 = shl i64 %18, 1
  %26 = tail call ptr @realloc(ptr noundef nonnull %19, i64 noundef %25)
  %27 = icmp eq ptr %26, null
  br i1 %27, label %28, label %29

28:                                               ; preds = %24
  tail call void @free(ptr noundef nonnull %19)
  br label %37

29:                                               ; preds = %24, %22
  %30 = phi ptr [ %19, %22 ], [ %26, %24 ]
  %31 = phi i64 [ %20, %22 ], [ %25, %24 ]
  %32 = getelementptr inbounds nuw i8, ptr %30, i64 %18
  %33 = sub i64 %31, %18
  %34 = tail call i64 @fread(ptr noundef nonnull %32, i64 noundef 1, i64 noundef %33, ptr noundef nonnull %11)
  %35 = add i64 %34, %18
  %36 = icmp eq i64 %34, 0
  br i1 %36, label %40, label %17

37:                                               ; preds = %17, %28
  %38 = tail call i32 @fclose(ptr noundef nonnull %11)
  %39 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str.3, ptr noundef %3, ptr noundef %4)
  br label %59

40:                                               ; preds = %29
  %41 = tail call i32 @ferror(ptr noundef nonnull %11)
  %42 = icmp eq i32 %41, 0
  br i1 %42, label %46, label %43

43:                                               ; preds = %40
  %44 = tail call fastcc i32 @fail_errno(ptr noundef %3, ptr noundef %4)
  tail call void @free(ptr noundef nonnull %30)
  %45 = tail call i32 @fclose(ptr noundef nonnull %11)
  br label %59

46:                                               ; preds = %40
  %47 = tail call i32 @fclose(ptr noundef nonnull %11)
  %48 = icmp eq i32 %2, 0
  br i1 %48, label %58, label %49

49:                                               ; preds = %46, %52
  %50 = phi i64 [ %55, %52 ], [ 0, %46 ]
  %51 = icmp ult i64 %50, %35
  br i1 %51, label %52, label %58

52:                                               ; preds = %49
  %53 = tail call fastcc i64 @utf8_sequence(ptr noundef nonnull readonly %30, i64 noundef %50, i64 noundef %35)
  %54 = icmp eq i64 %53, 0
  %55 = add i64 %53, %50
  br i1 %54, label %56, label %49

56:                                               ; preds = %52
  tail call void @free(ptr noundef nonnull %30)
  %57 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str.6, ptr noundef %3, ptr noundef %4)
  br label %59

58:                                               ; preds = %49, %46
  store ptr %30, ptr %3, align 8
  store i64 %35, ptr %4, align 8
  br label %59

59:                                               ; preds = %13, %58, %56, %43, %37, %8
  %60 = phi i32 [ 4, %8 ], [ %14, %13 ], [ 4, %37 ], [ %44, %43 ], [ 0, %58 ], [ 3, %56 ]
  ret i32 %60
}

define dso_local range(i32 0, 5) i32 @__neuro_read_bytes(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, ptr nofree noundef writeonly captures(none) %2, ptr nofree noundef writeonly captures(none) %3) local_unnamed_addr {
  %5 = tail call fastcc i32 @read_file(ptr noundef %0, i64 noundef %1, i32 noundef 0, ptr noundef %2, ptr noundef %3)
  ret i32 %5
}

define dso_local range(i32 0, 5) i32 @__neuro_write_file(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, ptr noundef %2, i64 noundef %3, ptr nofree noundef writeonly captures(none) initializes((0, 8)) %4, ptr nofree noundef writeonly captures(none) initializes((0, 8)) %5) local_unnamed_addr {
  store ptr null, ptr %4, align 8
  store i64 0, ptr %5, align 8
  %7 = tail call fastcc ptr @c_string(ptr noundef %0, i64 noundef %1)
  %8 = icmp eq ptr %7, null
  br i1 %8, label %9, label %11

9:                                                ; preds = %6
  %10 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str, ptr noundef nonnull %4, ptr noundef nonnull %5)
  br label %27

11:                                               ; preds = %6
  %12 = tail call ptr @fopen(ptr noundef nonnull %7, ptr noundef nonnull @.str.1)
  tail call void @free(ptr noundef nonnull %7)
  %13 = icmp eq ptr %12, null
  br i1 %13, label %14, label %16

14:                                               ; preds = %11
  %15 = tail call fastcc i32 @fail_errno(ptr noundef nonnull %4, ptr noundef nonnull %5)
  br label %27

16:                                               ; preds = %11
  %17 = tail call i64 @fwrite(ptr noundef %2, i64 noundef 1, i64 noundef %3, ptr noundef nonnull %12)
  %18 = icmp eq i64 %17, %3
  br i1 %18, label %22, label %19

19:                                               ; preds = %16
  %20 = tail call fastcc i32 @fail_errno(ptr noundef nonnull %4, ptr noundef nonnull %5)
  %21 = tail call i32 @fclose(ptr noundef nonnull %12)
  br label %27

22:                                               ; preds = %16
  %23 = tail call i32 @fclose(ptr noundef nonnull %12)
  %24 = icmp eq i32 %23, 0
  br i1 %24, label %27, label %25

25:                                               ; preds = %22
  %26 = tail call fastcc i32 @fail_errno(ptr noundef nonnull %4, ptr noundef nonnull %5)
  br label %27

27:                                               ; preds = %14, %22, %25, %19, %9
  %28 = phi i32 [ 4, %9 ], [ %15, %14 ], [ %20, %19 ], [ %26, %25 ], [ 0, %22 ]
  ret i32 %28
}

declare void @llvm.lifetime.start.p0(ptr captures(none))

define internal fastcc ptr @c_string(ptr nofree noundef readonly captures(none) %0, i64 noundef %1) unnamed_addr {
  %3 = icmp sgt i64 %1, 0
  br i1 %3, label %7, label %12

4:                                                ; preds = %7
  %5 = add nuw nsw i64 %8, 1
  %6 = icmp eq i64 %5, %1
  br i1 %6, label %12, label %7

7:                                                ; preds = %2, %4
  %8 = phi i64 [ %5, %4 ], [ 0, %2 ]
  %9 = getelementptr inbounds nuw i8, ptr %0, i64 %8
  %10 = load i8, ptr %9, align 1
  %11 = icmp eq i8 %10, 0
  br i1 %11, label %27, label %4

12:                                               ; preds = %4, %2
  %13 = add i64 %1, 1
  %14 = tail call ptr @malloc(i64 noundef %13)
  %15 = icmp eq ptr %14, null
  br i1 %15, label %27, label %16

16:                                               ; preds = %12
  %17 = icmp eq i64 %1, 0
  br i1 %17, label %25, label %18

18:                                               ; preds = %16, %18
  %19 = phi i64 [ %23, %18 ], [ 0, %16 ]
  %20 = getelementptr inbounds nuw i8, ptr %0, i64 %19
  %21 = load i8, ptr %20, align 1
  %22 = getelementptr inbounds nuw i8, ptr %14, i64 %19
  store i8 %21, ptr %22, align 1
  %23 = add nuw i64 %19, 1
  %24 = icmp eq i64 %23, %1
  br i1 %24, label %25, label %18

25:                                               ; preds = %18, %16
  %26 = getelementptr inbounds i8, ptr %14, i64 %1
  store i8 0, ptr %26, align 1
  br label %27

27:                                               ; preds = %7, %25, %12
  %28 = phi ptr [ null, %12 ], [ %14, %25 ], [ null, %7 ]
  ret ptr %28
}

declare ptr @fopen(ptr noundef, ptr noundef) local_unnamed_addr

declare void @free(ptr noundef) local_unnamed_addr

define internal fastcc range(i32 1, 5) i32 @fail_errno(ptr nofree noundef writeonly captures(none) %0, ptr nofree noundef writeonly captures(none) %1) unnamed_addr {
  %3 = load ptr, ptr @__neuro_errno_location, align 8
  %4 = tail call ptr %3()
  %5 = load i32, ptr %4, align 4
  switch i32 %5, label %7 [
    i32 2, label %8
    i32 13, label %6
    i32 1, label %6
  ]

6:                                                ; preds = %2, %2
  br label %8

7:                                                ; preds = %2
  br label %8

8:                                                ; preds = %2, %7, %6
  %9 = phi i32 [ 1, %2 ], [ 2, %6 ], [ 4, %7 ]
  %10 = tail call ptr @strerror(i32 noundef %5)
  %11 = tail call fastcc i32 @give_copy(ptr noundef readonly %10, ptr noundef %0, ptr noundef %1)
  ret i32 %9
}

declare i64 @fwrite(ptr noundef, i64 noundef, i64 noundef, ptr noundef) local_unnamed_addr

declare i32 @fclose(ptr noundef) local_unnamed_addr

declare void @llvm.lifetime.end.p0(ptr captures(none))

define dso_local range(i32 0, 5) i32 @__neuro_remove_file(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, ptr nofree noundef writeonly captures(none) initializes((0, 8)) %2, ptr nofree noundef writeonly captures(none) initializes((0, 8)) %3) local_unnamed_addr {
  store ptr null, ptr %2, align 8
  store i64 0, ptr %3, align 8
  %5 = tail call fastcc ptr @c_string(ptr noundef %0, i64 noundef %1)
  %6 = icmp eq ptr %5, null
  br i1 %6, label %7, label %9

7:                                                ; preds = %4
  %8 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str, ptr noundef nonnull %2, ptr noundef nonnull %3)
  br label %14

9:                                                ; preds = %4
  %10 = tail call i32 @remove(ptr noundef nonnull %5)
  tail call void @free(ptr noundef nonnull %5)
  %11 = icmp eq i32 %10, 0
  br i1 %11, label %14, label %12

12:                                               ; preds = %9
  %13 = tail call fastcc i32 @fail_errno(ptr noundef nonnull %2, ptr noundef nonnull %3)
  br label %14

14:                                               ; preds = %12, %9, %7
  %15 = phi i32 [ 4, %7 ], [ %13, %12 ], [ 0, %9 ]
  ret i32 %15
}

declare i32 @remove(ptr noundef) local_unnamed_addr

define dso_local range(i64 -2147483648, 2147483648) i64 @__neuro_arg_count() local_unnamed_addr {
  %1 = load i32, ptr @__neuro_argc, align 4
  %2 = sext i32 %1 to i64
  ret i64 %2
}

define dso_local range(i32 0, 5) i32 @__neuro_arg(i64 noundef %0, ptr nofree noundef writeonly captures(none) %1, ptr nofree noundef writeonly captures(none) %2) local_unnamed_addr {
  %4 = icmp sgt i64 %0, -1
  %5 = load i32, ptr @__neuro_argc, align 4
  %6 = sext i32 %5 to i64
  %7 = icmp slt i64 %0, %6
  %8 = select i1 %4, i1 %7, i1 false
  br i1 %8, label %11, label %9

9:                                                ; preds = %3
  %10 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str.2, ptr noundef %1, ptr noundef %2)
  br label %16

11:                                               ; preds = %3
  %12 = load ptr, ptr @__neuro_argv, align 8
  %13 = getelementptr inbounds nuw [8 x i8], ptr %12, i64 %0
  %14 = load ptr, ptr %13, align 8
  %15 = tail call fastcc i32 @give_lossy(ptr noundef %14, ptr noundef %1, ptr noundef %2)
  br label %16

16:                                               ; preds = %11, %9
  %17 = phi i32 [ %10, %9 ], [ %15, %11 ]
  ret i32 %17
}

define internal fastcc range(i32 0, 5) i32 @give_copy(ptr nofree noundef readonly captures(none) %0, ptr nofree noundef writeonly captures(none) %1, ptr nofree noundef writeonly captures(none) %2) unnamed_addr {
  br label %4

4:                                                ; preds = %4, %3
  %5 = phi i64 [ 0, %3 ], [ %9, %4 ]
  %6 = getelementptr inbounds nuw i8, ptr %0, i64 %5
  %7 = load i8, ptr %6, align 1
  %8 = icmp eq i8 %7, 0
  %9 = add i64 %5, 1
  br i1 %8, label %10, label %4

10:                                               ; preds = %4
  %11 = tail call ptr @malloc(i64 noundef %5)
  %12 = icmp eq ptr %11, null
  %13 = icmp ne i64 %5, 0
  %14 = and i1 %13, %12
  br i1 %14, label %25, label %15

15:                                               ; preds = %10
  %16 = icmp eq i64 %5, 0
  br i1 %16, label %24, label %17

17:                                               ; preds = %15, %17
  %18 = phi i64 [ %22, %17 ], [ 0, %15 ]
  %19 = getelementptr inbounds nuw i8, ptr %0, i64 %18
  %20 = load i8, ptr %19, align 1
  %21 = getelementptr inbounds nuw i8, ptr %11, i64 %18
  store i8 %20, ptr %21, align 1
  %22 = add nuw i64 %18, 1
  %23 = icmp eq i64 %22, %5
  br i1 %23, label %24, label %17

24:                                               ; preds = %17, %15
  store ptr %11, ptr %1, align 8
  store i64 %5, ptr %2, align 8
  br label %25

25:                                               ; preds = %10, %24
  %26 = phi i32 [ 0, %24 ], [ 4, %10 ]
  ret i32 %26
}

define internal fastcc range(i32 0, 5) i32 @give_lossy(ptr nofree noundef readonly captures(none) %0, ptr nofree noundef writeonly captures(none) %1, ptr nofree noundef writeonly captures(none) %2) unnamed_addr {
  br label %4

4:                                                ; preds = %4, %3
  %5 = phi i64 [ 0, %3 ], [ %9, %4 ]
  %6 = getelementptr inbounds nuw i8, ptr %0, i64 %5
  %7 = load i8, ptr %6, align 1
  %8 = icmp eq i8 %7, 0
  %9 = add i64 %5, 1
  br i1 %8, label %10, label %4

10:                                               ; preds = %4
  %11 = mul i64 %5, 3
  %12 = tail call ptr @malloc(i64 noundef %11)
  %13 = icmp eq ptr %12, null
  %14 = icmp ne i64 %5, 0
  %15 = and i1 %14, %13
  br i1 %15, label %47, label %16

16:                                               ; preds = %10
  %17 = icmp eq i64 %5, 0
  br i1 %17, label %45, label %18

18:                                               ; preds = %16, %41
  %19 = phi i64 [ %43, %41 ], [ 0, %16 ]
  %20 = phi i64 [ %42, %41 ], [ 0, %16 ]
  %21 = tail call fastcc i64 @utf8_sequence(ptr noundef %0, i64 noundef %19, i64 noundef %5)
  %22 = icmp eq i64 %21, 0
  %23 = getelementptr inbounds nuw i8, ptr %12, i64 %20
  br i1 %22, label %24, label %29

24:                                               ; preds = %18
  store i8 -17, ptr %23, align 1
  %25 = getelementptr i8, ptr %23, i64 1
  store i8 -65, ptr %25, align 1
  %26 = add i64 %20, 3
  %27 = getelementptr i8, ptr %23, i64 2
  store i8 -67, ptr %27, align 1
  %28 = add nuw i64 %19, 1
  br label %41

29:                                               ; preds = %18
  %30 = getelementptr inbounds nuw i8, ptr %0, i64 %19
  br label %31

31:                                               ; preds = %29, %31
  %32 = phi i64 [ %36, %31 ], [ 0, %29 ]
  %33 = getelementptr inbounds nuw i8, ptr %30, i64 %32
  %34 = load i8, ptr %33, align 1
  %35 = getelementptr inbounds nuw i8, ptr %23, i64 %32
  store i8 %34, ptr %35, align 1
  %36 = add nuw nsw i64 %32, 1
  %37 = icmp eq i64 %36, %21
  br i1 %37, label %38, label %31

38:                                               ; preds = %31
  %39 = add i64 %21, %20
  %40 = add i64 %21, %19
  br label %41

41:                                               ; preds = %38, %24
  %42 = phi i64 [ %26, %24 ], [ %39, %38 ]
  %43 = phi i64 [ %28, %24 ], [ %40, %38 ]
  %44 = icmp ult i64 %43, %5
  br i1 %44, label %18, label %45

45:                                               ; preds = %41, %16
  %46 = phi i64 [ 0, %16 ], [ %42, %41 ]
  store ptr %12, ptr %1, align 8
  store i64 %46, ptr %2, align 8
  br label %47

47:                                               ; preds = %10, %45
  %48 = phi i32 [ 0, %45 ], [ 4, %10 ]
  ret i32 %48
}

define dso_local range(i32 0, 6) i32 @__neuro_env_var(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, ptr nofree noundef writeonly captures(none) initializes((0, 8)) %2, ptr nofree noundef writeonly captures(none) initializes((0, 8)) %3) local_unnamed_addr {
  store ptr null, ptr %2, align 8
  store i64 0, ptr %3, align 8
  %5 = icmp sgt i64 %1, 0
  br i1 %5, label %9, label %14

6:                                                ; preds = %9
  %7 = add nuw nsw i64 %10, 1
  %8 = icmp eq i64 %7, %1
  br i1 %8, label %14, label %9

9:                                                ; preds = %4, %6
  %10 = phi i64 [ %7, %6 ], [ 0, %4 ]
  %11 = getelementptr inbounds nuw i8, ptr %0, i64 %10
  %12 = load i8, ptr %11, align 1
  %13 = icmp eq i8 %12, 61
  br i1 %13, label %25, label %6

14:                                               ; preds = %6, %4
  %15 = tail call fastcc ptr @c_string(ptr noundef %0, i64 noundef %1)
  %16 = icmp eq ptr %15, null
  %17 = icmp eq i64 %1, 0
  %18 = or i1 %17, %16
  br i1 %18, label %19, label %20

19:                                               ; preds = %14
  tail call void @free(ptr noundef %15)
  br label %25

20:                                               ; preds = %14
  %21 = tail call ptr @getenv(ptr noundef nonnull %15)
  tail call void @free(ptr noundef nonnull %15)
  %22 = icmp eq ptr %21, null
  br i1 %22, label %25, label %23

23:                                               ; preds = %20
  %24 = tail call fastcc i32 @give_lossy(ptr noundef nonnull %21, ptr noundef nonnull %2, ptr noundef nonnull %3)
  br label %25

25:                                               ; preds = %9, %19, %20, %23
  %26 = phi i32 [ 5, %20 ], [ 5, %19 ], [ %24, %23 ], [ 5, %9 ]
  ret i32 %26
}

declare ptr @getenv(ptr noundef) local_unnamed_addr

define dso_local range(i32 0, 6) i32 @__neuro_read_line(ptr nofree noundef writeonly captures(none) initializes((0, 8)) %0, ptr nofree noundef writeonly captures(none) initializes((0, 8)) %1) local_unnamed_addr {
  store ptr null, ptr %0, align 8
  store i64 0, ptr %1, align 8
  %3 = load i64, ptr @stdin_start, align 8
  %4 = load i64, ptr @stdin_end, align 8
  br label %8

5:                                                ; preds = %66
  %6 = add nsw i64 %40, 1
  store i64 %6, ptr @stdin_start, align 8
  %7 = icmp eq i64 %42, 0
  br i1 %7, label %81, label %72

8:                                                ; preds = %66, %2
  %9 = phi i64 [ %4, %2 ], [ %67, %66 ]
  %10 = phi i64 [ %3, %2 ], [ %40, %66 ]
  %11 = phi i64 [ 0, %2 ], [ %55, %66 ]
  %12 = phi i64 [ 0, %2 ], [ %42, %66 ]
  %13 = phi ptr [ null, %2 ], [ %54, %66 ]
  %14 = icmp eq i64 %10, %9
  br i1 %14, label %15, label %27

15:                                               ; preds = %8
  %16 = load i1, ptr @stdin_closed, align 4
  br i1 %16, label %69, label %17

17:                                               ; preds = %15
  %18 = tail call i32 @read(i32 noundef 0, ptr noundef nonnull @stdin_buffer, i64 noundef 65536)
  %19 = icmp slt i32 %18, 0
  br i1 %19, label %20, label %22

20:                                               ; preds = %17
  tail call void @free(ptr noundef %13)
  %21 = tail call fastcc i32 @fail_errno(ptr noundef nonnull %0, ptr noundef nonnull %1)
  br label %94

22:                                               ; preds = %17
  %23 = icmp eq i32 %18, 0
  br i1 %23, label %26, label %24

24:                                               ; preds = %22
  store i64 0, ptr @stdin_start, align 8
  %25 = zext nneg i32 %18 to i64
  store i64 %25, ptr @stdin_end, align 8
  br label %27

26:                                               ; preds = %22
  store i1 true, ptr @stdin_closed, align 4
  br label %69

27:                                               ; preds = %24, %8
  %28 = phi i64 [ %25, %24 ], [ %9, %8 ]
  %29 = phi i64 [ 0, %24 ], [ %10, %8 ]
  %30 = icmp slt i64 %29, %28
  br i1 %30, label %31, label %39

31:                                               ; preds = %27, %36
  %32 = phi i64 [ %37, %36 ], [ %29, %27 ]
  %33 = getelementptr inbounds i8, ptr @stdin_buffer, i64 %32
  %34 = load i8, ptr %33, align 1
  %35 = icmp eq i8 %34, 10
  br i1 %35, label %39, label %36

36:                                               ; preds = %31
  %37 = add i64 %32, 1
  %38 = icmp eq i64 %37, %28
  br i1 %38, label %39, label %31

39:                                               ; preds = %31, %36, %27
  %40 = phi i64 [ %29, %27 ], [ %28, %36 ], [ %32, %31 ]
  %41 = sub nsw i64 %40, %29
  %42 = add i64 %41, %12
  %43 = icmp ugt i64 %42, %11
  br i1 %43, label %44, label %52

44:                                               ; preds = %39
  %45 = shl i64 %42, 1
  %46 = tail call ptr @realloc(ptr noundef %13, i64 noundef %45)
  %47 = icmp eq ptr %46, null
  br i1 %47, label %50, label %48

48:                                               ; preds = %44
  %49 = load i64, ptr @stdin_start, align 8
  br label %52

50:                                               ; preds = %44
  tail call void @free(ptr noundef %13)
  %51 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str.3, ptr noundef nonnull %0, ptr noundef nonnull %1)
  br label %94

52:                                               ; preds = %48, %39
  %53 = phi i64 [ %29, %39 ], [ %49, %48 ]
  %54 = phi ptr [ %13, %39 ], [ %46, %48 ]
  %55 = phi i64 [ %11, %39 ], [ %45, %48 ]
  %56 = getelementptr inbounds nuw i8, ptr %54, i64 %12
  %57 = getelementptr inbounds i8, ptr @stdin_buffer, i64 %53
  %58 = icmp eq i64 %40, %29
  br i1 %58, label %66, label %59

59:                                               ; preds = %52, %59
  %60 = phi i64 [ %64, %59 ], [ 0, %52 ]
  %61 = getelementptr inbounds nuw i8, ptr %57, i64 %60
  %62 = load i8, ptr %61, align 1
  %63 = getelementptr inbounds nuw i8, ptr %56, i64 %60
  store i8 %62, ptr %63, align 1
  %64 = add nuw i64 %60, 1
  %65 = icmp eq i64 %64, %41
  br i1 %65, label %66, label %59

66:                                               ; preds = %59, %52
  store i64 %40, ptr @stdin_start, align 8
  %67 = load i64, ptr @stdin_end, align 8
  %68 = icmp slt i64 %40, %67
  br i1 %68, label %5, label %8

69:                                               ; preds = %15, %26
  %70 = icmp eq i64 %12, 0
  br i1 %70, label %71, label %72

71:                                               ; preds = %69
  tail call void @free(ptr noundef %13)
  br label %94

72:                                               ; preds = %69, %5
  %73 = phi ptr [ %13, %69 ], [ %54, %5 ]
  %74 = phi i64 [ %12, %69 ], [ %42, %5 ]
  %75 = getelementptr i8, ptr %73, i64 %74
  %76 = getelementptr i8, ptr %75, i64 -1
  %77 = load i8, ptr %76, align 1
  %78 = icmp eq i8 %77, 13
  %79 = sext i1 %78 to i64
  %80 = add i64 %74, %79
  br label %81

81:                                               ; preds = %5, %72
  %82 = phi ptr [ %54, %5 ], [ %73, %72 ]
  %83 = phi i64 [ 0, %5 ], [ %80, %72 ]
  br label %84

84:                                               ; preds = %87, %81
  %85 = phi i64 [ 0, %81 ], [ %90, %87 ]
  %86 = icmp ult i64 %85, %83
  br i1 %86, label %87, label %93

87:                                               ; preds = %84
  %88 = tail call fastcc i64 @utf8_sequence(ptr noundef readonly %82, i64 noundef %85, i64 noundef %83)
  %89 = icmp eq i64 %88, 0
  %90 = add i64 %88, %85
  br i1 %89, label %91, label %84

91:                                               ; preds = %87
  tail call void @free(ptr noundef %82)
  %92 = tail call fastcc i32 @give_copy(ptr noundef nonnull @.str.4, ptr noundef nonnull %0, ptr noundef nonnull %1)
  br label %94

93:                                               ; preds = %84
  store ptr %82, ptr %0, align 8
  store i64 %83, ptr %1, align 8
  br label %94

94:                                               ; preds = %50, %20, %93, %91, %71
  %95 = phi i32 [ 5, %71 ], [ 0, %93 ], [ 3, %91 ], [ %21, %20 ], [ 4, %50 ]
  ret i32 %95
}

declare i32 @read(i32 noundef, ptr noundef, i64 noundef) local_unnamed_addr

declare ptr @realloc(ptr noundef, i64 noundef) local_unnamed_addr

define dso_local i64 @__neuro_monotonic_nanos() local_unnamed_addr {
  %1 = alloca [2 x i64], align 16
  %2 = alloca i64, align 8
  %3 = alloca i64, align 8
  %4 = load ptr, ptr @__neuro_clock_gettime, align 8
  %5 = icmp eq ptr %4, null
  br i1 %5, label %14, label %6

6:                                                ; preds = %0
  call void @llvm.lifetime.start.p0(ptr nonnull %1)
  call void @llvm.memset.p0.i64(ptr noundef nonnull align 16 dereferenceable(16) %1, i8 0, i64 16, i1 false)
  %7 = load i32, ptr @__neuro_clock_id, align 4
  %8 = call i32 %4(i32 noundef %7, ptr noundef nonnull %1)
  %9 = load i64, ptr %1, align 16
  %10 = mul nsw i64 %9, 1000000000
  %11 = getelementptr inbounds nuw i8, ptr %1, i64 8
  %12 = load i64, ptr %11, align 8
  %13 = add nsw i64 %10, %12
  call void @llvm.lifetime.end.p0(ptr nonnull %1)
  br label %27

14:                                               ; preds = %0
  call void @llvm.lifetime.start.p0(ptr nonnull %2)
  store i64 0, ptr %2, align 8
  call void @llvm.lifetime.start.p0(ptr nonnull %3)
  store i64 1, ptr %3, align 8
  %15 = load ptr, ptr @__neuro_query_counter, align 8
  %16 = call i32 %15(ptr noundef nonnull %2)
  %17 = load ptr, ptr @__neuro_query_frequency, align 8
  %18 = call i32 %17(ptr noundef nonnull %3)
  %19 = load i64, ptr %2, align 8
  %20 = load i64, ptr %3, align 8
  %21 = sdiv i64 %19, %20
  %22 = mul nsw i64 %21, 1000000000
  %23 = srem i64 %19, %20
  %24 = mul nsw i64 %23, 1000000000
  %25 = sdiv i64 %24, %20
  %26 = add nsw i64 %25, %22
  call void @llvm.lifetime.end.p0(ptr nonnull %3)
  call void @llvm.lifetime.end.p0(ptr nonnull %2)
  br label %27

27:                                               ; preds = %14, %6
  %28 = phi i64 [ %13, %6 ], [ %26, %14 ]
  ret i64 %28
}

declare void @llvm.memset.p0.i64(ptr writeonly captures(none), i8, i64, i1 immarg)

declare ptr @malloc(i64 noundef) local_unnamed_addr

declare i64 @fread(ptr noundef, i64 noundef, i64 noundef, ptr noundef) local_unnamed_addr

declare i32 @ferror(ptr noundef) local_unnamed_addr

declare ptr @strerror(i32 noundef) local_unnamed_addr

define internal fastcc range(i64 0, 5) i64 @utf8_sequence(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, i64 noundef %2) unnamed_addr {
  %4 = getelementptr inbounds nuw i8, ptr %0, i64 %1
  %5 = load i8, ptr %4, align 1
  %6 = icmp sgt i8 %5, -1
  br i1 %6, label %47, label %7

7:                                                ; preds = %3
  %8 = add nsw i8 %5, 62
  %9 = icmp ult i8 %8, 30
  br i1 %9, label %22, label %10

10:                                               ; preds = %7
  %11 = and i8 %5, -16
  %12 = icmp eq i8 %11, -32
  br i1 %12, label %13, label %15

13:                                               ; preds = %10
  switch i8 %5, label %20 [
    i8 -32, label %22
    i8 -19, label %14
  ]

14:                                               ; preds = %13
  br label %22

15:                                               ; preds = %10
  %16 = add nsw i8 %5, 16
  %17 = icmp ult i8 %16, 5
  br i1 %17, label %18, label %47

18:                                               ; preds = %15
  switch i8 %5, label %21 [
    i8 -16, label %22
    i8 -12, label %19
  ]

19:                                               ; preds = %18
  br label %22

20:                                               ; preds = %13
  br label %22

21:                                               ; preds = %18
  br label %22

22:                                               ; preds = %18, %21, %13, %20, %7, %14, %19
  %23 = phi i64 [ 4, %18 ], [ 2, %7 ], [ 3, %14 ], [ 3, %13 ], [ 3, %20 ], [ 4, %19 ], [ 4, %21 ]
  %24 = phi i32 [ 144, %18 ], [ 128, %7 ], [ 128, %14 ], [ 160, %13 ], [ 128, %20 ], [ 128, %19 ], [ 128, %21 ]
  %25 = phi i32 [ 191, %18 ], [ 191, %7 ], [ 159, %14 ], [ 191, %13 ], [ 191, %20 ], [ 143, %19 ], [ 191, %21 ]
  %26 = sub i64 %2, %1
  %27 = icmp ult i64 %26, %23
  br i1 %27, label %47, label %28

28:                                               ; preds = %22
  %29 = getelementptr i8, ptr %4, i64 1
  %30 = load i8, ptr %29, align 1
  %31 = zext i8 %30 to i32
  %32 = icmp samesign ugt i32 %24, %31
  %33 = icmp samesign ult i32 %25, %31
  %34 = select i1 %32, i1 true, i1 %33
  %35 = or i1 %34, %9
  %36 = select i1 %34, i64 0, i64 %23
  br i1 %35, label %47, label %37

37:                                               ; preds = %28
  %38 = tail call i64 @llvm.umax.i64(i64 %23, i64 3)
  br label %42

39:                                               ; preds = %42
  %40 = add nuw nsw i64 %43, 1
  %41 = icmp eq i64 %40, %38
  br i1 %41, label %47, label %42

42:                                               ; preds = %37, %39
  %43 = phi i64 [ %40, %39 ], [ 2, %37 ]
  %44 = getelementptr i8, ptr %4, i64 %43
  %45 = load i8, ptr %44, align 1
  %46 = icmp sgt i8 %45, -65
  br i1 %46, label %47, label %39

47:                                               ; preds = %42, %39, %28, %15, %22, %3
  %48 = phi i64 [ 1, %3 ], [ 0, %15 ], [ 0, %22 ], [ %36, %28 ], [ 0, %42 ], [ %23, %39 ]
  ret i64 %48
}

declare i64 @llvm.umax.i64(i64, i64)

