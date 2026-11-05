;
; Text to number: the runtime behind `string.parse::<T>()`, linked into a module that
; calls it. Exact decimal to binary conversion for every float format, so `f16` and
; `bf16` round once and every platform gives the same bits.
;
; Generated from compiler/llvm-backend/src/codegen/text_parse.c with clang -O2
; -emit-llvm, then stripped of the target datalayout and triple, attribute groups and
; metadata. Regenerate it with tools/regen_gpu_runtime.sh.
;

%struct.Big = type { i32, [96 x i32] }

define dso_local range(i32 0, 4) i32 @__neuro_parse_int(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, i64 noundef %2, i64 noundef %3, ptr nofree noundef writeonly captures(none) %4) local_unnamed_addr {
  %6 = icmp eq i64 %1, 0
  br i1 %6, label %49, label %7

7:                                                ; preds = %5
  %8 = load i8, ptr %0, align 1
  %9 = icmp eq i8 %8, 45
  %10 = icmp eq i8 %8, 43
  %11 = or i1 %9, %10
  %12 = zext i1 %11 to i64
  %13 = icmp eq i64 %1, %12
  br i1 %13, label %49, label %14

14:                                               ; preds = %7
  %15 = icmp ugt i64 %1, %12
  br i1 %15, label %16, label %40

16:                                               ; preds = %14, %24
  %17 = phi i32 [ %35, %24 ], [ 0, %14 ]
  %18 = phi i64 [ %32, %24 ], [ 0, %14 ]
  %19 = phi i64 [ %36, %24 ], [ %12, %14 ]
  %20 = getelementptr inbounds nuw i8, ptr %0, i64 %19
  %21 = load i8, ptr %20, align 1
  %22 = add i8 %21, -58
  %23 = icmp ult i8 %22, -10
  br i1 %23, label %49, label %24

24:                                               ; preds = %16
  %25 = tail call { i64, i1 } @llvm.umul.with.overflow.i64(i64 %18, i64 10)
  %26 = extractvalue { i64, i1 } %25, 1
  %27 = extractvalue { i64, i1 } %25, 0
  %28 = zext nneg i8 %21 to i64
  %29 = add nsw i64 %28, -48
  %30 = tail call { i64, i1 } @llvm.uadd.with.overflow.i64(i64 %27, i64 %29)
  %31 = extractvalue { i64, i1 } %30, 1
  %32 = extractvalue { i64, i1 } %30, 0
  %33 = or i1 %26, %31
  %34 = zext i1 %33 to i32
  %35 = or i32 %17, %34
  %36 = add i64 %19, 1
  %37 = icmp eq i64 %36, %1
  br i1 %37, label %38, label %16

38:                                               ; preds = %24
  %39 = icmp ne i32 %35, 0
  br label %40

40:                                               ; preds = %38, %14
  %41 = phi i64 [ 0, %14 ], [ %32, %38 ]
  %42 = phi i1 [ false, %14 ], [ %39, %38 ]
  %43 = select i1 %9, i64 %3, i64 %2
  %44 = icmp ugt i64 %41, %43
  %45 = select i1 %42, i1 true, i1 %44
  br i1 %45, label %49, label %46

46:                                               ; preds = %40
  %47 = sub i64 0, %41
  %48 = select i1 %9, i64 %47, i64 %41
  store i64 %48, ptr %4, align 8
  br label %49

49:                                               ; preds = %16, %7, %40, %46, %5
  %50 = phi i32 [ 1, %5 ], [ 2, %7 ], [ 0, %46 ], [ 3, %40 ], [ 2, %16 ]
  ret i32 %50
}

declare void @llvm.lifetime.start.p0(ptr captures(none))

declare { i64, i1 } @llvm.umul.with.overflow.i64(i64, i64)

declare { i64, i1 } @llvm.uadd.with.overflow.i64(i64, i64)

declare void @llvm.lifetime.end.p0(ptr captures(none))

define dso_local range(i32 0, 3) i32 @__neuro_parse_bool(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, ptr nofree noundef writeonly captures(none) %2) local_unnamed_addr {
  switch i64 %1, label %40 [
    i64 0, label %41
    i64 4, label %4
    i64 5, label %20
  ]

4:                                                ; preds = %3
  %5 = load i8, ptr %0, align 1
  %6 = icmp eq i8 %5, 116
  br i1 %6, label %7, label %40

7:                                                ; preds = %4
  %8 = getelementptr inbounds nuw i8, ptr %0, i64 1
  %9 = load i8, ptr %8, align 1
  %10 = icmp eq i8 %9, 114
  br i1 %10, label %11, label %40

11:                                               ; preds = %7
  %12 = getelementptr inbounds nuw i8, ptr %0, i64 2
  %13 = load i8, ptr %12, align 1
  %14 = icmp eq i8 %13, 117
  br i1 %14, label %15, label %40

15:                                               ; preds = %11
  %16 = getelementptr inbounds nuw i8, ptr %0, i64 3
  %17 = load i8, ptr %16, align 1
  %18 = icmp eq i8 %17, 101
  br i1 %18, label %19, label %40

19:                                               ; preds = %15
  store i64 1, ptr %2, align 8
  br label %41

20:                                               ; preds = %3
  %21 = load i8, ptr %0, align 1
  %22 = icmp eq i8 %21, 102
  br i1 %22, label %23, label %40

23:                                               ; preds = %20
  %24 = getelementptr inbounds nuw i8, ptr %0, i64 1
  %25 = load i8, ptr %24, align 1
  %26 = icmp eq i8 %25, 97
  br i1 %26, label %27, label %40

27:                                               ; preds = %23
  %28 = getelementptr inbounds nuw i8, ptr %0, i64 2
  %29 = load i8, ptr %28, align 1
  %30 = icmp eq i8 %29, 108
  br i1 %30, label %31, label %40

31:                                               ; preds = %27
  %32 = getelementptr inbounds nuw i8, ptr %0, i64 3
  %33 = load i8, ptr %32, align 1
  %34 = icmp eq i8 %33, 115
  br i1 %34, label %35, label %40

35:                                               ; preds = %31
  %36 = getelementptr inbounds nuw i8, ptr %0, i64 4
  %37 = load i8, ptr %36, align 1
  %38 = icmp eq i8 %37, 101
  br i1 %38, label %39, label %40

39:                                               ; preds = %35
  store i64 0, ptr %2, align 8
  br label %41

40:                                               ; preds = %4, %7, %11, %15, %3, %35, %31, %27, %23, %20
  br label %41

41:                                               ; preds = %3, %40, %39, %19
  %42 = phi i32 [ 2, %40 ], [ 0, %19 ], [ 0, %39 ], [ 1, %3 ]
  ret i32 %42
}

define dso_local range(i32 0, 3) i32 @__neuro_parse_float(ptr nofree noundef readonly captures(none) %0, i64 noundef %1, i32 noundef %2, i32 noundef %3, ptr nofree noundef writeonly captures(none) %4) local_unnamed_addr {
  %6 = alloca %struct.Big, align 4
  %7 = alloca %struct.Big, align 4
  %8 = icmp eq i64 %1, 0
  br i1 %8, label %306, label %9

9:                                                ; preds = %5
  %10 = load i8, ptr %0, align 1
  %11 = icmp eq i8 %10, 45
  %12 = icmp eq i8 %10, 43
  %13 = or i1 %11, %12
  %14 = zext i1 %13 to i64
  %15 = zext i1 %11 to i64
  %16 = add i32 %3, %2
  %17 = zext nneg i32 %16 to i64
  %18 = shl nuw i64 %15, %17
  %19 = zext nneg i32 %3 to i64
  %20 = shl nsw i64 -1, %19
  %21 = xor i64 %20, -1
  %22 = zext nneg i32 %2 to i64
  %23 = shl i64 %21, %22
  %24 = getelementptr inbounds nuw i8, ptr %0, i64 %14
  %25 = sub nuw i64 %1, %14
  %26 = icmp eq i64 %1, %14
  br i1 %26, label %130, label %27

27:                                               ; preds = %9
  %28 = load i8, ptr %24, align 1
  %29 = and i8 %28, -33
  %30 = icmp eq i8 %29, 73
  br i1 %30, label %31, label %47

31:                                               ; preds = %27
  %32 = icmp eq i64 %25, 1
  br i1 %32, label %104, label %33

33:                                               ; preds = %31
  %34 = getelementptr inbounds nuw i8, ptr %24, i64 1
  %35 = load i8, ptr %34, align 1
  %36 = and i8 %35, -33
  %37 = icmp eq i8 %36, 78
  br i1 %37, label %38, label %47

38:                                               ; preds = %33
  %39 = icmp eq i64 %25, 2
  br i1 %39, label %104, label %40

40:                                               ; preds = %38
  %41 = getelementptr inbounds nuw i8, ptr %24, i64 2
  %42 = load i8, ptr %41, align 1
  %43 = and i8 %42, -33
  %44 = icmp eq i8 %43, 70
  %45 = icmp eq i64 %25, 3
  %46 = select i1 %44, i1 %45, i1 false
  br i1 %46, label %102, label %47

47:                                               ; preds = %40, %33, %27
  %48 = load i8, ptr %24, align 1
  %49 = and i8 %48, -33
  %50 = icmp ne i8 %49, 73
  %51 = icmp eq i64 %25, 1
  %52 = select i1 %50, i1 true, i1 %51
  br i1 %52, label %104, label %53

53:                                               ; preds = %47
  %54 = getelementptr inbounds nuw i8, ptr %24, i64 1
  %55 = load i8, ptr %54, align 1
  %56 = and i8 %55, -33
  %57 = icmp ne i8 %56, 78
  %58 = icmp eq i64 %25, 2
  %59 = select i1 %57, i1 true, i1 %58
  br i1 %59, label %104, label %60

60:                                               ; preds = %53
  %61 = getelementptr inbounds nuw i8, ptr %24, i64 2
  %62 = load i8, ptr %61, align 1
  %63 = and i8 %62, -33
  %64 = icmp ne i8 %63, 70
  %65 = icmp eq i64 %25, 3
  %66 = select i1 %64, i1 true, i1 %65
  br i1 %66, label %104, label %67

67:                                               ; preds = %60
  %68 = getelementptr inbounds nuw i8, ptr %24, i64 3
  %69 = load i8, ptr %68, align 1
  %70 = and i8 %69, -33
  %71 = icmp ne i8 %70, 73
  %72 = icmp eq i64 %25, 4
  %73 = select i1 %71, i1 true, i1 %72
  br i1 %73, label %104, label %74

74:                                               ; preds = %67
  %75 = getelementptr inbounds nuw i8, ptr %24, i64 4
  %76 = load i8, ptr %75, align 1
  %77 = and i8 %76, -33
  %78 = icmp ne i8 %77, 78
  %79 = icmp eq i64 %25, 5
  %80 = select i1 %78, i1 true, i1 %79
  br i1 %80, label %104, label %81

81:                                               ; preds = %74
  %82 = getelementptr inbounds nuw i8, ptr %24, i64 5
  %83 = load i8, ptr %82, align 1
  %84 = and i8 %83, -33
  %85 = icmp ne i8 %84, 73
  %86 = icmp eq i64 %25, 6
  %87 = select i1 %85, i1 true, i1 %86
  br i1 %87, label %104, label %88

88:                                               ; preds = %81
  %89 = getelementptr inbounds nuw i8, ptr %24, i64 6
  %90 = load i8, ptr %89, align 1
  %91 = and i8 %90, -33
  %92 = icmp ne i8 %91, 84
  %93 = icmp eq i64 %25, 7
  %94 = select i1 %92, i1 true, i1 %93
  br i1 %94, label %104, label %95

95:                                               ; preds = %88
  %96 = getelementptr inbounds nuw i8, ptr %24, i64 7
  %97 = load i8, ptr %96, align 1
  %98 = and i8 %97, -33
  %99 = icmp eq i8 %98, 89
  %100 = icmp eq i64 %25, 8
  %101 = select i1 %99, i1 %100, i1 false
  br i1 %101, label %102, label %104

102:                                              ; preds = %95, %40
  %103 = or i64 %18, %23
  store i64 %103, ptr %4, align 8
  br label %306

104:                                              ; preds = %38, %31, %95, %88, %81, %74, %67, %60, %53, %47
  %105 = load i8, ptr %24, align 1
  %106 = and i8 %105, -33
  %107 = icmp ne i8 %106, 78
  %108 = icmp eq i64 %25, 1
  %109 = select i1 %107, i1 true, i1 %108
  br i1 %109, label %130, label %110

110:                                              ; preds = %104
  %111 = getelementptr inbounds nuw i8, ptr %24, i64 1
  %112 = load i8, ptr %111, align 1
  %113 = and i8 %112, -33
  %114 = icmp ne i8 %113, 65
  %115 = icmp eq i64 %25, 2
  %116 = select i1 %114, i1 true, i1 %115
  br i1 %116, label %130, label %117

117:                                              ; preds = %110
  %118 = getelementptr inbounds nuw i8, ptr %24, i64 2
  %119 = load i8, ptr %118, align 1
  %120 = and i8 %119, -33
  %121 = icmp eq i8 %120, 78
  %122 = icmp eq i64 %25, 3
  %123 = select i1 %121, i1 %122, i1 false
  br i1 %123, label %124, label %130

124:                                              ; preds = %117
  %125 = add i32 %2, -1
  %126 = zext nneg i32 %125 to i64
  %127 = shl nuw i64 1, %126
  %128 = or i64 %23, %127
  %129 = or i64 %128, %18
  store i64 %129, ptr %4, align 8
  br label %306

130:                                              ; preds = %9, %104, %110, %117
  call void @llvm.lifetime.start.p0(ptr nonnull %6)
  %131 = getelementptr inbounds nuw i8, ptr %6, i64 4
  store i32 0, ptr %131, align 4
  %132 = icmp ugt i64 %1, %14
  br i1 %132, label %133, label %304

133:                                              ; preds = %130, %223
  %134 = phi i64 [ %230, %223 ], [ %14, %130 ]
  %135 = phi i32 [ %229, %223 ], [ 0, %130 ]
  %136 = phi i32 [ %228, %223 ], [ 0, %130 ]
  %137 = phi i32 [ %227, %223 ], [ 0, %130 ]
  %138 = phi i64 [ %226, %223 ], [ 0, %130 ]
  %139 = phi i32 [ %225, %223 ], [ 0, %130 ]
  %140 = phi i32 [ %224, %223 ], [ 0, %130 ]
  %141 = getelementptr inbounds nuw i8, ptr %0, i64 %134
  %142 = load i8, ptr %141, align 1
  %143 = icmp ne i8 %142, 46
  %144 = icmp ne i32 %135, 0
  %145 = select i1 %143, i1 true, i1 %144
  br i1 %145, label %146, label %223

146:                                              ; preds = %133
  %147 = add i8 %142, -58
  %148 = icmp ult i8 %147, -10
  br i1 %148, label %232, label %149

149:                                              ; preds = %146
  %150 = icmp eq i32 %139, 0
  %151 = icmp eq i8 %142, 48
  %152 = and i1 %150, %151
  br i1 %152, label %153, label %156

153:                                              ; preds = %149
  %154 = zext nneg i32 %135 to i64
  %155 = sub nsw i64 %138, %154
  br label %223

156:                                              ; preds = %149
  %157 = icmp ult i32 %139, 800
  br i1 %157, label %158, label %216

158:                                              ; preds = %156
  %159 = and i8 %142, 15
  %160 = zext nneg i8 %159 to i64
  %161 = icmp eq i32 %140, 0
  br i1 %161, label %181, label %162

162:                                              ; preds = %158
  %163 = zext i32 %140 to i64
  %164 = and i64 %163, 1
  %165 = icmp eq i32 %140, 1
  br i1 %165, label %170, label %166

166:                                              ; preds = %162
  %167 = and i64 %163, 4294967294
  br label %184

168:                                              ; preds = %184
  %169 = icmp eq i64 %164, 0
  br i1 %169, label %181, label %170

170:                                              ; preds = %168, %162
  %171 = phi i64 [ 0, %162 ], [ %203, %168 ]
  %172 = phi i64 [ %160, %162 ], [ %202, %168 ]
  %173 = trunc i32 %140 to i1
  tail call void @llvm.assume(i1 %173)
  %174 = getelementptr inbounds nuw [4 x i8], ptr %131, i64 %171
  %175 = load i32, ptr %174, align 4
  %176 = zext i32 %175 to i64
  %177 = mul nuw nsw i64 %176, 10
  %178 = add nuw nsw i64 %177, %172
  %179 = trunc i64 %178 to i32
  store i32 %179, ptr %174, align 4
  %180 = lshr i64 %178, 32
  br label %181

181:                                              ; preds = %170, %168, %158
  %182 = phi i64 [ %160, %158 ], [ %202, %168 ], [ %180, %170 ]
  %183 = icmp eq i64 %182, 0
  br i1 %183, label %211, label %206

184:                                              ; preds = %184, %166
  %185 = phi i64 [ 0, %166 ], [ %203, %184 ]
  %186 = phi i64 [ %160, %166 ], [ %202, %184 ]
  %187 = phi i64 [ 0, %166 ], [ %204, %184 ]
  %188 = getelementptr inbounds nuw [4 x i8], ptr %131, i64 %185
  %189 = load i32, ptr %188, align 4
  %190 = zext i32 %189 to i64
  %191 = mul nuw nsw i64 %190, 10
  %192 = add nuw nsw i64 %191, %186
  %193 = trunc i64 %192 to i32
  store i32 %193, ptr %188, align 4
  %194 = lshr i64 %192, 32
  %195 = getelementptr inbounds nuw [4 x i8], ptr %131, i64 %185
  %196 = getelementptr inbounds nuw i8, ptr %195, i64 4
  %197 = load i32, ptr %196, align 4
  %198 = zext i32 %197 to i64
  %199 = mul nuw nsw i64 %198, 10
  %200 = add nuw nsw i64 %199, %194
  %201 = trunc i64 %200 to i32
  store i32 %201, ptr %196, align 4
  %202 = lshr i64 %200, 32
  %203 = add nuw nsw i64 %185, 2
  %204 = add i64 %187, 2
  %205 = icmp eq i64 %204, %167
  br i1 %205, label %168, label %184

206:                                              ; preds = %181
  %207 = trunc nuw nsw i64 %182 to i32
  %208 = add i32 %140, 1
  %209 = zext i32 %140 to i64
  %210 = getelementptr inbounds nuw [4 x i8], ptr %131, i64 %209
  store i32 %207, ptr %210, align 4
  br label %211

211:                                              ; preds = %181, %206
  %212 = phi i32 [ %140, %181 ], [ %208, %206 ]
  %213 = add nuw nsw i32 %139, 1
  %214 = zext nneg i32 %135 to i64
  %215 = sub nsw i64 %138, %214
  br label %223

216:                                              ; preds = %156
  %217 = icmp ne i8 %142, 48
  %218 = zext i1 %217 to i32
  %219 = or i32 %137, %218
  %220 = xor i1 %144, true
  %221 = zext i1 %220 to i64
  %222 = add nsw i64 %138, %221
  br label %223

223:                                              ; preds = %133, %211, %216, %153
  %224 = phi i32 [ %140, %216 ], [ %212, %211 ], [ %140, %153 ], [ %140, %133 ]
  %225 = phi i32 [ %139, %216 ], [ %213, %211 ], [ 0, %153 ], [ %139, %133 ]
  %226 = phi i64 [ %222, %216 ], [ %215, %211 ], [ %155, %153 ], [ %138, %133 ]
  %227 = phi i32 [ %219, %216 ], [ %137, %211 ], [ %137, %153 ], [ %137, %133 ]
  %228 = phi i32 [ 1, %216 ], [ 1, %211 ], [ 1, %153 ], [ %136, %133 ]
  %229 = phi i32 [ %135, %216 ], [ %135, %211 ], [ %135, %153 ], [ 1, %133 ]
  %230 = add i64 %134, 1
  %231 = icmp eq i64 %230, %1
  br i1 %231, label %234, label %133

232:                                              ; preds = %146
  store i32 %140, ptr %6, align 4
  %233 = icmp eq i32 %136, 0
  br i1 %233, label %304, label %236

234:                                              ; preds = %223
  %235 = icmp eq i32 %228, 0
  store i32 %224, ptr %6, align 4
  br i1 %235, label %304, label %280

236:                                              ; preds = %232
  switch i8 %142, label %276 [
    i8 101, label %237
    i8 69, label %237
  ]

237:                                              ; preds = %236, %236
  %238 = add nuw i64 %134, 1
  %239 = icmp eq i64 %1, %238
  br i1 %239, label %247, label %240

240:                                              ; preds = %237
  %241 = getelementptr inbounds nuw i8, ptr %0, i64 %238
  %242 = load i8, ptr %241, align 1
  %243 = icmp eq i8 %242, 45
  br i1 %243, label %247, label %244

244:                                              ; preds = %240
  %245 = icmp eq i8 %242, 43
  %246 = zext i1 %245 to i64
  br label %247

247:                                              ; preds = %237, %240, %244
  %248 = phi i1 [ true, %244 ], [ false, %240 ], [ true, %237 ]
  %249 = phi i64 [ %246, %244 ], [ 1, %240 ], [ 0, %237 ]
  %250 = add i64 %249, %238
  %251 = icmp eq i64 %250, %1
  br i1 %251, label %304, label %252

252:                                              ; preds = %247
  %253 = icmp ult i64 %250, %1
  br i1 %253, label %254, label %270

254:                                              ; preds = %252, %261
  %255 = phi i64 [ %267, %261 ], [ 0, %252 ]
  %256 = phi i64 [ %268, %261 ], [ %250, %252 ]
  %257 = getelementptr inbounds nuw i8, ptr %0, i64 %256
  %258 = load i8, ptr %257, align 1
  %259 = add i8 %258, -58
  %260 = icmp ult i8 %259, -10
  br i1 %260, label %270, label %261

261:                                              ; preds = %254
  %262 = icmp slt i64 %255, 100000000000
  %263 = mul nsw i64 %255, 10
  %264 = zext nneg i8 %258 to i64
  %265 = add i64 %263, -48
  %266 = add i64 %265, %264
  %267 = select i1 %262, i64 %266, i64 %255
  %268 = add nuw i64 %256, 1
  %269 = icmp ult i64 %268, %1
  br i1 %269, label %254, label %270

270:                                              ; preds = %254, %261, %252
  %271 = phi i64 [ %250, %252 ], [ %1, %261 ], [ %256, %254 ]
  %272 = phi i64 [ 0, %252 ], [ %267, %261 ], [ %255, %254 ]
  %273 = sub nsw i64 0, %272
  %274 = select i1 %248, i64 %272, i64 %273
  %275 = add nsw i64 %274, %138
  br label %276

276:                                              ; preds = %270, %236
  %277 = phi i64 [ %275, %270 ], [ %138, %236 ]
  %278 = phi i64 [ %271, %270 ], [ %134, %236 ]
  %279 = icmp eq i64 %278, %1
  br i1 %279, label %280, label %304

280:                                              ; preds = %234, %276
  %281 = phi i64 [ %277, %276 ], [ %226, %234 ]
  %282 = phi i32 [ %137, %276 ], [ %227, %234 ]
  %283 = phi i32 [ %139, %276 ], [ %225, %234 ]
  %284 = zext i32 %283 to i64
  %285 = add nsw i64 %281, %284
  %286 = icmp eq i32 %283, 0
  %287 = icmp slt i64 %285, -330
  %288 = select i1 %286, i1 true, i1 %287
  br i1 %288, label %289, label %290

289:                                              ; preds = %280
  store i64 %18, ptr %4, align 8
  br label %304

290:                                              ; preds = %280
  %291 = icmp sgt i64 %285, 310
  br i1 %291, label %292, label %294

292:                                              ; preds = %290
  %293 = or i64 %18, %23
  store i64 %293, ptr %4, align 8
  br label %304

294:                                              ; preds = %290
  call void @llvm.lifetime.start.p0(ptr nonnull %7)
  store i32 1, ptr %7, align 4
  %295 = getelementptr inbounds nuw i8, ptr %7, i64 4
  store i32 1, ptr %295, align 4
  %296 = icmp sgt i64 %281, 0
  %297 = trunc i64 %281 to i32
  br i1 %296, label %298, label %299

298:                                              ; preds = %294
  call fastcc void @big_mul_pow5(ptr noundef %6, i32 noundef %297)
  br label %301

299:                                              ; preds = %294
  %300 = sub i32 0, %297
  call fastcc void @big_mul_pow5(ptr noundef %7, i32 noundef %300)
  br label %301

301:                                              ; preds = %299, %298
  %302 = call fastcc i64 @round_fraction(ptr noundef %6, ptr noundef %7, i64 noundef %281, i32 noundef %282, i32 noundef %2, i32 noundef %3)
  %303 = or i64 %302, %18
  store i64 %303, ptr %4, align 8
  call void @llvm.lifetime.end.p0(ptr nonnull %7)
  br label %304

304:                                              ; preds = %130, %247, %234, %289, %292, %301, %276, %232
  %305 = phi i32 [ 2, %234 ], [ 2, %276 ], [ 2, %247 ], [ 2, %232 ], [ 0, %301 ], [ 0, %292 ], [ 0, %289 ], [ 2, %130 ]
  call void @llvm.lifetime.end.p0(ptr nonnull %6)
  br label %306

306:                                              ; preds = %102, %124, %304, %5
  %307 = phi i32 [ 1, %5 ], [ 0, %102 ], [ 0, %124 ], [ %305, %304 ]
  ret i32 %307
}

define internal fastcc void @big_mul_pow5(ptr nofree noundef nonnull captures(none) %0, i32 noundef %1) unnamed_addr {
  %3 = icmp ugt i32 %1, 12
  br i1 %3, label %4, label %7

4:                                                ; preds = %2
  %5 = getelementptr inbounds nuw i8, ptr %0, i64 4
  %6 = load i32, ptr %0, align 4
  br label %13

7:                                                ; preds = %65, %2
  %8 = phi i32 [ %1, %2 ], [ %67, %65 ]
  %9 = icmp eq i32 %8, 0
  br i1 %9, label %125, label %10

10:                                               ; preds = %7
  %11 = getelementptr inbounds nuw i8, ptr %0, i64 4
  %12 = load i32, ptr %0, align 4
  br label %69

13:                                               ; preds = %4, %65
  %14 = phi i32 [ %6, %4 ], [ %66, %65 ]
  %15 = phi i32 [ %1, %4 ], [ %67, %65 ]
  %16 = icmp eq i32 %14, 0
  br i1 %16, label %65, label %17

17:                                               ; preds = %13
  %18 = zext i32 %14 to i64
  %19 = and i64 %18, 1
  %20 = icmp eq i32 %14, 1
  br i1 %20, label %25, label %21

21:                                               ; preds = %17
  %22 = and i64 %18, 4294967294
  br label %39

23:                                               ; preds = %39
  %24 = icmp eq i64 %19, 0
  br i1 %24, label %36, label %25

25:                                               ; preds = %23, %17
  %26 = phi i64 [ 0, %17 ], [ %58, %23 ]
  %27 = phi i64 [ 0, %17 ], [ %57, %23 ]
  %28 = trunc i32 %14 to i1
  tail call void @llvm.assume(i1 %28)
  %29 = getelementptr inbounds nuw [4 x i8], ptr %5, i64 %26
  %30 = load i32, ptr %29, align 4
  %31 = zext i32 %30 to i64
  %32 = mul nuw nsw i64 %31, 1220703125
  %33 = add nuw nsw i64 %32, %27
  %34 = trunc i64 %33 to i32
  store i32 %34, ptr %29, align 4
  %35 = lshr i64 %33, 32
  br label %36

36:                                               ; preds = %23, %25
  %37 = phi i64 [ %57, %23 ], [ %35, %25 ]
  %38 = icmp eq i64 %37, 0
  br i1 %38, label %65, label %61

39:                                               ; preds = %39, %21
  %40 = phi i64 [ 0, %21 ], [ %58, %39 ]
  %41 = phi i64 [ 0, %21 ], [ %57, %39 ]
  %42 = phi i64 [ 0, %21 ], [ %59, %39 ]
  %43 = getelementptr inbounds nuw [4 x i8], ptr %5, i64 %40
  %44 = load i32, ptr %43, align 4
  %45 = zext i32 %44 to i64
  %46 = mul nuw nsw i64 %45, 1220703125
  %47 = add nuw nsw i64 %46, %41
  %48 = trunc i64 %47 to i32
  store i32 %48, ptr %43, align 4
  %49 = lshr i64 %47, 32
  %50 = getelementptr inbounds nuw [4 x i8], ptr %5, i64 %40
  %51 = getelementptr inbounds nuw i8, ptr %50, i64 4
  %52 = load i32, ptr %51, align 4
  %53 = zext i32 %52 to i64
  %54 = mul nuw nsw i64 %53, 1220703125
  %55 = add nuw nsw i64 %54, %49
  %56 = trunc i64 %55 to i32
  store i32 %56, ptr %51, align 4
  %57 = lshr i64 %55, 32
  %58 = add nuw nsw i64 %40, 2
  %59 = add i64 %42, 2
  %60 = icmp eq i64 %59, %22
  br i1 %60, label %23, label %39

61:                                               ; preds = %36
  %62 = trunc nuw nsw i64 %37 to i32
  %63 = add i32 %14, 1
  store i32 %63, ptr %0, align 4
  %64 = getelementptr inbounds nuw [4 x i8], ptr %5, i64 %18
  store i32 %62, ptr %64, align 4
  br label %65

65:                                               ; preds = %13, %36, %61
  %66 = phi i32 [ 0, %13 ], [ %14, %36 ], [ %63, %61 ]
  %67 = add i32 %15, -13
  %68 = icmp ugt i32 %67, 12
  br i1 %68, label %13, label %7

69:                                               ; preds = %10, %121
  %70 = phi i32 [ %12, %10 ], [ %122, %121 ]
  %71 = phi i32 [ %8, %10 ], [ %123, %121 ]
  %72 = icmp eq i32 %70, 0
  br i1 %72, label %121, label %73

73:                                               ; preds = %69
  %74 = zext i32 %70 to i64
  %75 = and i64 %74, 1
  %76 = icmp eq i32 %70, 1
  br i1 %76, label %81, label %77

77:                                               ; preds = %73
  %78 = and i64 %74, 4294967294
  br label %95

79:                                               ; preds = %95
  %80 = icmp eq i64 %75, 0
  br i1 %80, label %92, label %81

81:                                               ; preds = %79, %73
  %82 = phi i64 [ 0, %73 ], [ %114, %79 ]
  %83 = phi i64 [ 0, %73 ], [ %113, %79 ]
  %84 = trunc i32 %70 to i1
  tail call void @llvm.assume(i1 %84)
  %85 = getelementptr inbounds nuw [4 x i8], ptr %11, i64 %82
  %86 = load i32, ptr %85, align 4
  %87 = zext i32 %86 to i64
  %88 = mul nuw nsw i64 %87, 5
  %89 = add nuw nsw i64 %88, %83
  %90 = trunc i64 %89 to i32
  store i32 %90, ptr %85, align 4
  %91 = lshr i64 %89, 32
  br label %92

92:                                               ; preds = %79, %81
  %93 = phi i64 [ %113, %79 ], [ %91, %81 ]
  %94 = icmp eq i64 %93, 0
  br i1 %94, label %121, label %117

95:                                               ; preds = %95, %77
  %96 = phi i64 [ 0, %77 ], [ %114, %95 ]
  %97 = phi i64 [ 0, %77 ], [ %113, %95 ]
  %98 = phi i64 [ 0, %77 ], [ %115, %95 ]
  %99 = getelementptr inbounds nuw [4 x i8], ptr %11, i64 %96
  %100 = load i32, ptr %99, align 4
  %101 = zext i32 %100 to i64
  %102 = mul nuw nsw i64 %101, 5
  %103 = add nuw nsw i64 %102, %97
  %104 = trunc i64 %103 to i32
  store i32 %104, ptr %99, align 4
  %105 = lshr i64 %103, 32
  %106 = getelementptr inbounds nuw [4 x i8], ptr %11, i64 %96
  %107 = getelementptr inbounds nuw i8, ptr %106, i64 4
  %108 = load i32, ptr %107, align 4
  %109 = zext i32 %108 to i64
  %110 = mul nuw nsw i64 %109, 5
  %111 = add nuw nsw i64 %110, %105
  %112 = trunc i64 %111 to i32
  store i32 %112, ptr %107, align 4
  %113 = lshr i64 %111, 32
  %114 = add nuw nsw i64 %96, 2
  %115 = add i64 %98, 2
  %116 = icmp eq i64 %115, %78
  br i1 %116, label %79, label %95

117:                                              ; preds = %92
  %118 = trunc nuw nsw i64 %93 to i32
  %119 = add i32 %70, 1
  store i32 %119, ptr %0, align 4
  %120 = getelementptr inbounds nuw [4 x i8], ptr %11, i64 %74
  store i32 %118, ptr %120, align 4
  br label %121

121:                                              ; preds = %69, %92, %117
  %122 = phi i32 [ 0, %69 ], [ %70, %92 ], [ %119, %117 ]
  %123 = add nsw i32 %71, -1
  %124 = icmp eq i32 %123, 0
  br i1 %124, label %125, label %69

125:                                              ; preds = %121, %7
  ret void
}

define internal fastcc i64 @round_fraction(ptr nofree noundef nonnull captures(none) %0, ptr nofree noundef nonnull captures(none) %1, i64 noundef %2, i32 noundef range(i32 0, 2) %3, i32 noundef %4, i32 noundef %5) unnamed_addr {
  %7 = load i32, ptr %1, align 4
  %8 = icmp eq i32 %7, 0
  br i1 %8, label %22, label %9

9:                                                ; preds = %6
  %10 = add i32 %7, -1
  %11 = zext i32 %10 to i64
  %12 = shl nuw nsw i64 %11, 5
  %13 = add nuw nsw i64 %12, 32
  %14 = getelementptr inbounds nuw i8, ptr %1, i64 4
  %15 = getelementptr inbounds nuw [4 x i8], ptr %14, i64 %11
  %16 = load i32, ptr %15, align 4
  %17 = tail call range(i32 0, 33) i32 @llvm.ctlz.i32(i32 %16, i1 true)
  %18 = zext nneg i32 %17 to i64
  %19 = sub nuw nsw i64 %13, %18
  %20 = load i32, ptr %0, align 4
  %21 = icmp eq i32 %20, 0
  br i1 %21, label %250, label %25

22:                                               ; preds = %6
  %23 = load i32, ptr %0, align 4
  %24 = icmp eq i32 %23, 0
  br i1 %24, label %255, label %25

25:                                               ; preds = %9, %22
  %26 = phi i32 [ %23, %22 ], [ %20, %9 ]
  %27 = phi i64 [ 0, %22 ], [ %19, %9 ]
  %28 = add i32 %26, -1
  %29 = zext i32 %28 to i64
  %30 = getelementptr inbounds nuw i8, ptr %0, i64 4
  %31 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %29
  %32 = load i32, ptr %31, align 4
  %33 = tail call range(i32 0, 33) i32 @llvm.ctlz.i32(i32 %32, i1 true)
  %34 = zext nneg i32 %33 to i64
  %35 = add nsw i64 %27, -32
  %36 = shl nuw nsw i64 %29, 5
  %37 = sub nsw i64 %35, %36
  %38 = add nsw i64 %37, %34
  %39 = icmp sgt i64 %38, 0
  %40 = trunc i64 %38 to i32
  br i1 %39, label %41, label %138

41:                                               ; preds = %25
  %42 = lshr i32 %40, 5
  %43 = and i32 %40, 31
  %44 = add i32 %42, %26
  %45 = icmp eq i32 %43, 0
  %46 = sub nuw nsw i32 32, %43
  %47 = lshr i32 %32, %46
  %48 = select i1 %45, i32 0, i32 %47
  %49 = zext i32 %44 to i64
  %50 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %49
  store i32 %48, ptr %50, align 4
  %51 = icmp eq i32 %28, 0
  br i1 %51, label %77, label %52

52:                                               ; preds = %41
  %53 = sub nuw nsw i32 32, %43
  %54 = and i32 %28, 1
  %55 = icmp eq i32 %54, 0
  br i1 %55, label %73, label %56

56:                                               ; preds = %52
  br i1 %45, label %63, label %57

57:                                               ; preds = %56
  %58 = add i32 %26, -2
  %59 = zext i32 %58 to i64
  %60 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %59
  %61 = load i32, ptr %60, align 4
  %62 = lshr i32 %61, %53
  br label %63

63:                                               ; preds = %57, %56
  %64 = phi i32 [ %62, %57 ], [ 0, %56 ]
  %65 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %29
  %66 = load i32, ptr %65, align 4
  %67 = shl i32 %66, %43
  %68 = or i32 %67, %64
  %69 = add i32 %42, %28
  %70 = zext i32 %69 to i64
  %71 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %70
  store i32 %68, ptr %71, align 4
  %72 = add nsw i64 %29, -1
  br label %73

73:                                               ; preds = %63, %52
  %74 = phi i64 [ %29, %52 ], [ %72, %63 ]
  %75 = phi i32 [ %26, %52 ], [ %28, %63 ]
  %76 = icmp eq i32 %26, 2
  br i1 %76, label %77, label %86

77:                                               ; preds = %73, %116, %41
  %78 = load i32, ptr %30, align 4
  %79 = shl i32 %78, %43
  %80 = zext nneg i32 %42 to i64
  %81 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %80
  store i32 %79, ptr %81, align 4
  %82 = icmp eq i32 %42, 0
  br i1 %82, label %130, label %83

83:                                               ; preds = %77
  %84 = lshr i64 %38, 3
  %85 = and i64 %84, 536870908
  tail call void @llvm.memset.p0.i64(ptr nonnull align 4 %30, i8 0, i64 %85, i1 false)
  br label %130

86:                                               ; preds = %73, %116
  %87 = phi i64 [ %127, %116 ], [ %74, %73 ]
  %88 = phi i32 [ %123, %116 ], [ %75, %73 ]
  br i1 %45, label %108, label %89

89:                                               ; preds = %86
  %90 = add i32 %88, -2
  %91 = zext i32 %90 to i64
  %92 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %91
  %93 = load i32, ptr %92, align 4
  %94 = lshr i32 %93, %53
  %95 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %87
  %96 = load i32, ptr %95, align 4
  %97 = shl i32 %96, %43
  %98 = or i32 %97, %94
  %99 = trunc nuw i64 %87 to i32
  %100 = add i32 %42, %99
  %101 = zext i32 %100 to i64
  %102 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %101
  store i32 %98, ptr %102, align 4
  %103 = add i64 %87, 4294967294
  %104 = and i64 %103, 4294967295
  %105 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %104
  %106 = load i32, ptr %105, align 4
  %107 = lshr i32 %106, %53
  br label %116

108:                                              ; preds = %86
  %109 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %87
  %110 = load i32, ptr %109, align 4
  %111 = shl i32 %110, %43
  %112 = trunc nuw i64 %87 to i32
  %113 = add i32 %42, %112
  %114 = zext i32 %113 to i64
  %115 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %114
  store i32 %111, ptr %115, align 4
  br label %116

116:                                              ; preds = %108, %89
  %117 = phi i32 [ %107, %89 ], [ 0, %108 ]
  %118 = getelementptr [4 x i8], ptr %0, i64 %87
  %119 = load i32, ptr %118, align 4
  %120 = shl i32 %119, %43
  %121 = or i32 %120, %117
  %122 = trunc i64 %87 to i32
  %123 = add i32 %122, -1
  %124 = add i32 %42, %123
  %125 = zext i32 %124 to i64
  %126 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %125
  store i32 %121, ptr %126, align 4
  %127 = add nsw i64 %87, -2
  %128 = and i64 %127, 4294967295
  %129 = icmp eq i64 %128, 0
  br i1 %129, label %77, label %86

130:                                              ; preds = %83, %77
  %131 = zext i32 %44 to i64
  %132 = getelementptr inbounds nuw [4 x i8], ptr %30, i64 %131
  %133 = load i32, ptr %132, align 4
  %134 = icmp ne i32 %133, 0
  %135 = zext i1 %134 to i32
  %136 = add i32 %44, %135
  store i32 %136, ptr %0, align 4
  %137 = load i32, ptr %1, align 4
  br label %250

138:                                              ; preds = %25
  %139 = sub i32 0, %40
  %140 = lshr i32 %139, 5
  %141 = and i32 %139, 31
  br i1 %8, label %340, label %142

142:                                              ; preds = %138
  %143 = add i32 %140, %7
  %144 = icmp eq i32 %141, 0
  %145 = getelementptr inbounds nuw i8, ptr %1, i64 4
  br i1 %144, label %155, label %146

146:                                              ; preds = %142
  %147 = add i32 %7, -1
  %148 = zext i32 %147 to i64
  %149 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %148
  %150 = load i32, ptr %149, align 4
  %151 = sub nuw nsw i32 32, %141
  %152 = lshr i32 %150, %151
  %153 = zext i32 %143 to i64
  %154 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %153
  store i32 %152, ptr %154, align 4
  br label %159

155:                                              ; preds = %142
  %156 = zext i32 %143 to i64
  %157 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %156
  store i32 0, ptr %157, align 4
  %158 = add i32 %7, -1
  br label %159

159:                                              ; preds = %155, %146
  %160 = phi i32 [ %158, %155 ], [ %147, %146 ]
  %161 = icmp eq i32 %160, 0
  br i1 %161, label %188, label %162

162:                                              ; preds = %159
  %163 = sub nuw nsw i32 32, %141
  %164 = zext i32 %160 to i64
  %165 = and i32 %160, 1
  %166 = icmp eq i32 %165, 0
  br i1 %166, label %184, label %167

167:                                              ; preds = %162
  br i1 %144, label %174, label %168

168:                                              ; preds = %167
  %169 = add i32 %7, -2
  %170 = zext i32 %169 to i64
  %171 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %170
  %172 = load i32, ptr %171, align 4
  %173 = lshr i32 %172, %163
  br label %174

174:                                              ; preds = %168, %167
  %175 = phi i32 [ %173, %168 ], [ 0, %167 ]
  %176 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %164
  %177 = load i32, ptr %176, align 4
  %178 = shl i32 %177, %141
  %179 = or i32 %178, %175
  %180 = add i32 %140, %160
  %181 = zext i32 %180 to i64
  %182 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %181
  store i32 %179, ptr %182, align 4
  %183 = add nsw i64 %164, -1
  br label %184

184:                                              ; preds = %174, %162
  %185 = phi i64 [ %164, %162 ], [ %183, %174 ]
  %186 = phi i32 [ %7, %162 ], [ %160, %174 ]
  %187 = icmp eq i32 %7, 2
  br i1 %187, label %188, label %198

188:                                              ; preds = %184, %228, %159
  %189 = load i32, ptr %145, align 4
  %190 = shl i32 %189, %141
  %191 = zext nneg i32 %140 to i64
  %192 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %191
  store i32 %190, ptr %192, align 4
  %193 = icmp eq i32 %140, 0
  br i1 %193, label %242, label %194

194:                                              ; preds = %188
  %195 = lshr i32 %139, 3
  %196 = and i32 %195, 536870908
  %197 = zext nneg i32 %196 to i64
  tail call void @llvm.memset.p0.i64(ptr nonnull align 4 %145, i8 0, i64 %197, i1 false)
  br label %242

198:                                              ; preds = %184, %228
  %199 = phi i64 [ %239, %228 ], [ %185, %184 ]
  %200 = phi i32 [ %235, %228 ], [ %186, %184 ]
  br i1 %144, label %220, label %201

201:                                              ; preds = %198
  %202 = add i32 %200, -2
  %203 = zext i32 %202 to i64
  %204 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %203
  %205 = load i32, ptr %204, align 4
  %206 = lshr i32 %205, %163
  %207 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %199
  %208 = load i32, ptr %207, align 4
  %209 = shl i32 %208, %141
  %210 = or i32 %209, %206
  %211 = trunc nuw i64 %199 to i32
  %212 = add i32 %140, %211
  %213 = zext i32 %212 to i64
  %214 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %213
  store i32 %210, ptr %214, align 4
  %215 = add i64 %199, 4294967294
  %216 = and i64 %215, 4294967295
  %217 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %216
  %218 = load i32, ptr %217, align 4
  %219 = lshr i32 %218, %163
  br label %228

220:                                              ; preds = %198
  %221 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %199
  %222 = load i32, ptr %221, align 4
  %223 = shl i32 %222, %141
  %224 = trunc nuw i64 %199 to i32
  %225 = add i32 %140, %224
  %226 = zext i32 %225 to i64
  %227 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %226
  store i32 %223, ptr %227, align 4
  br label %228

228:                                              ; preds = %220, %201
  %229 = phi i32 [ %219, %201 ], [ 0, %220 ]
  %230 = getelementptr [4 x i8], ptr %1, i64 %199
  %231 = load i32, ptr %230, align 4
  %232 = shl i32 %231, %141
  %233 = or i32 %232, %229
  %234 = trunc i64 %199 to i32
  %235 = add i32 %234, -1
  %236 = add i32 %140, %235
  %237 = zext i32 %236 to i64
  %238 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %237
  store i32 %233, ptr %238, align 4
  %239 = add nsw i64 %199, -2
  %240 = and i64 %239, 4294967295
  %241 = icmp eq i64 %240, 0
  br i1 %241, label %188, label %198

242:                                              ; preds = %194, %188
  %243 = zext i32 %143 to i64
  %244 = getelementptr inbounds nuw [4 x i8], ptr %145, i64 %243
  %245 = load i32, ptr %244, align 4
  %246 = icmp ne i32 %245, 0
  %247 = zext i1 %246 to i32
  %248 = add i32 %143, %247
  store i32 %248, ptr %1, align 4
  %249 = load i32, ptr %0, align 4
  br label %250

250:                                              ; preds = %242, %9, %130
  %251 = phi i32 [ %137, %130 ], [ %248, %242 ], [ %7, %9 ]
  %252 = phi i32 [ %136, %130 ], [ %249, %242 ], [ 0, %9 ]
  %253 = phi i64 [ %38, %130 ], [ %38, %242 ], [ %19, %9 ]
  %254 = icmp eq i32 %252, %251
  br i1 %254, label %255, label %262

255:                                              ; preds = %22, %250
  %256 = phi i64 [ %253, %250 ], [ 0, %22 ]
  %257 = phi i32 [ %252, %250 ], [ 0, %22 ]
  %258 = phi i32 [ %251, %250 ], [ 0, %22 ]
  %259 = icmp eq i32 %258, 0
  br i1 %259, label %340, label %260

260:                                              ; preds = %255
  %261 = zext i32 %258 to i64
  br label %267

262:                                              ; preds = %250
  %263 = icmp ult i32 %252, %251
  br i1 %263, label %276, label %340

264:                                              ; preds = %267
  %265 = add nsw i64 %268, -1
  %266 = icmp eq i64 %265, 0
  br i1 %266, label %340, label %267

267:                                              ; preds = %260, %264
  %268 = phi i64 [ %261, %260 ], [ %265, %264 ]
  %269 = getelementptr [4 x i8], ptr %0, i64 %268
  %270 = load i32, ptr %269, align 4
  %271 = getelementptr [4 x i8], ptr %1, i64 %268
  %272 = load i32, ptr %271, align 4
  %273 = icmp eq i32 %270, %272
  br i1 %273, label %264, label %274

274:                                              ; preds = %267
  %275 = icmp ult i32 %270, %272
  br i1 %275, label %276, label %340

276:                                              ; preds = %262, %274
  %277 = phi i32 [ %252, %262 ], [ %257, %274 ]
  %278 = phi i64 [ %253, %262 ], [ %256, %274 ]
  %279 = icmp eq i32 %277, 0
  br i1 %279, label %337, label %280

280:                                              ; preds = %276
  %281 = getelementptr inbounds nuw i8, ptr %0, i64 4
  %282 = add i32 %277, -1
  %283 = zext i32 %282 to i64
  %284 = getelementptr inbounds nuw [4 x i8], ptr %281, i64 %283
  %285 = load i32, ptr %284, align 4
  %286 = lshr i32 %285, 31
  %287 = zext i32 %277 to i64
  %288 = getelementptr inbounds nuw [4 x i8], ptr %281, i64 %287
  store i32 %286, ptr %288, align 4
  %289 = icmp eq i32 %282, 0
  br i1 %289, label %308, label %290

290:                                              ; preds = %280
  %291 = and i32 %282, 1
  %292 = icmp eq i32 %291, 0
  br i1 %292, label %302, label %293

293:                                              ; preds = %290
  %294 = add i32 %277, -2
  %295 = zext i32 %294 to i64
  %296 = getelementptr inbounds nuw [4 x i8], ptr %281, i64 %295
  %297 = load i32, ptr %296, align 4
  %298 = getelementptr inbounds nuw [4 x i8], ptr %281, i64 %283
  %299 = load i32, ptr %298, align 4
  %300 = tail call i32 @llvm.fshl.i32(i32 %299, i32 %297, i32 1)
  store i32 %300, ptr %298, align 4
  %301 = add nsw i64 %283, -1
  br label %302

302:                                              ; preds = %293, %290
  %303 = phi i64 [ %283, %290 ], [ %301, %293 ]
  %304 = phi i32 [ %277, %290 ], [ %282, %293 ]
  %305 = icmp eq i32 %277, 2
  br i1 %305, label %306, label %315

306:                                              ; preds = %315, %302
  %307 = load i32, ptr %288, align 4
  br label %308

308:                                              ; preds = %306, %280
  %309 = phi i32 [ %307, %306 ], [ %286, %280 ]
  %310 = load i32, ptr %281, align 4
  %311 = shl i32 %310, 1
  store i32 %311, ptr %281, align 4
  %312 = icmp ne i32 %309, 0
  %313 = zext i1 %312 to i32
  %314 = add i32 %277, %313
  store i32 %314, ptr %0, align 4
  br label %337

315:                                              ; preds = %302, %315
  %316 = phi i64 [ %334, %315 ], [ %303, %302 ]
  %317 = phi i32 [ %333, %315 ], [ %304, %302 ]
  %318 = add i32 %317, -2
  %319 = zext i32 %318 to i64
  %320 = getelementptr inbounds nuw [4 x i8], ptr %281, i64 %319
  %321 = load i32, ptr %320, align 4
  %322 = getelementptr inbounds nuw [4 x i8], ptr %281, i64 %316
  %323 = load i32, ptr %322, align 4
  %324 = tail call i32 @llvm.fshl.i32(i32 %323, i32 %321, i32 1)
  store i32 %324, ptr %322, align 4
  %325 = add i64 %316, 4294967294
  %326 = and i64 %325, 4294967295
  %327 = getelementptr inbounds nuw [4 x i8], ptr %281, i64 %326
  %328 = load i32, ptr %327, align 4
  %329 = getelementptr [4 x i8], ptr %0, i64 %316
  %330 = load i32, ptr %329, align 4
  %331 = tail call i32 @llvm.fshl.i32(i32 %330, i32 %328, i32 1)
  %332 = trunc i64 %316 to i32
  %333 = add i32 %332, -1
  store i32 %331, ptr %329, align 4
  %334 = add nsw i64 %316, -2
  %335 = and i64 %334, 4294967295
  %336 = icmp eq i64 %335, 0
  br i1 %336, label %306, label %315

337:                                              ; preds = %276, %308
  %338 = phi i32 [ 0, %276 ], [ %314, %308 ]
  %339 = add nsw i64 %278, 1
  br label %340

340:                                              ; preds = %264, %255, %138, %262, %337, %274
  %341 = phi i32 [ %338, %337 ], [ %257, %274 ], [ %252, %262 ], [ %26, %138 ], [ %258, %255 ], [ %258, %264 ]
  %342 = phi i64 [ %339, %337 ], [ %256, %274 ], [ %253, %262 ], [ %38, %138 ], [ %256, %255 ], [ %256, %264 ]
  %343 = sub nsw i64 %2, %342
  %344 = add nsw i32 %4, 1
  %345 = add i32 %4, 3
  %346 = icmp sgt i32 %4, -3
  br i1 %346, label %347, label %351

347:                                              ; preds = %340
  %348 = getelementptr inbounds nuw i8, ptr %1, i64 4
  %349 = getelementptr inbounds nuw i8, ptr %0, i64 4
  %350 = tail call i32 @llvm.smax.i32(i32 %345, i32 1)
  br label %369

351:                                              ; preds = %548, %340
  %352 = phi i32 [ %341, %340 ], [ %549, %548 ]
  %353 = phi i64 [ 0, %340 ], [ %490, %548 ]
  %354 = icmp ne i32 %352, 0
  %355 = zext i1 %354 to i32
  %356 = or i32 %3, %355
  %357 = add i32 %5, -1
  %358 = zext nneg i32 %357 to i64
  %359 = shl nsw i64 -1, %358
  %360 = xor i64 %359, -1
  %361 = add nsw i64 %359, 2
  %362 = zext nneg i32 %5 to i64
  %363 = shl nsw i64 -1, %362
  %364 = xor i64 %363, -1
  %365 = zext i32 %4 to i64
  %366 = shl i64 %364, %365
  %367 = sext i32 %344 to i64
  %368 = icmp slt i64 %343, %361
  br i1 %368, label %553, label %557

369:                                              ; preds = %347, %548
  %370 = phi i32 [ %341, %347 ], [ %549, %548 ]
  %371 = phi i32 [ %341, %347 ], [ %550, %548 ]
  %372 = phi i64 [ 0, %347 ], [ %490, %548 ]
  %373 = phi i32 [ 0, %347 ], [ %551, %548 ]
  %374 = shl i64 %372, 1
  %375 = load i32, ptr %1, align 4
  %376 = icmp eq i32 %371, %375
  br i1 %376, label %377, label %381

377:                                              ; preds = %369
  %378 = icmp eq i32 %371, 0
  br i1 %378, label %483, label %379

379:                                              ; preds = %377
  %380 = zext i32 %371 to i64
  br label %386

381:                                              ; preds = %369
  %382 = icmp ult i32 %371, %375
  br i1 %382, label %487, label %395

383:                                              ; preds = %386
  %384 = add nsw i64 %387, -1
  %385 = icmp eq i64 %384, 0
  br i1 %385, label %395, label %386

386:                                              ; preds = %379, %383
  %387 = phi i64 [ %380, %379 ], [ %384, %383 ]
  %388 = getelementptr [4 x i8], ptr %0, i64 %387
  %389 = load i32, ptr %388, align 4
  %390 = getelementptr [4 x i8], ptr %1, i64 %387
  %391 = load i32, ptr %390, align 4
  %392 = icmp eq i32 %389, %391
  br i1 %392, label %383, label %393

393:                                              ; preds = %386
  %394 = icmp ult i32 %389, %391
  br i1 %394, label %487, label %395

395:                                              ; preds = %383, %381, %393
  %396 = icmp eq i32 %371, 0
  br i1 %396, label %483, label %397

397:                                              ; preds = %395
  %398 = zext i32 %371 to i64
  %399 = and i64 %398, 1
  %400 = icmp eq i32 %371, 1
  br i1 %400, label %447, label %401

401:                                              ; preds = %397
  %402 = and i64 %398, 4294967294
  br label %403

403:                                              ; preds = %432, %401
  %404 = phi i64 [ 0, %401 ], [ %442, %432 ]
  %405 = phi i64 [ 0, %401 ], [ %439, %432 ]
  %406 = phi i64 [ 0, %401 ], [ %443, %432 ]
  %407 = load i32, ptr %1, align 4
  %408 = zext i32 %407 to i64
  %409 = icmp samesign ult i64 %404, %408
  br i1 %409, label %410, label %414

410:                                              ; preds = %403
  %411 = getelementptr inbounds nuw [4 x i8], ptr %348, i64 %404
  %412 = load i32, ptr %411, align 4
  %413 = zext i32 %412 to i64
  br label %414

414:                                              ; preds = %410, %403
  %415 = phi i64 [ %413, %410 ], [ 0, %403 ]
  %416 = add nuw nsw i64 %415, %405
  %417 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %404
  %418 = load i32, ptr %417, align 4
  %419 = zext i32 %418 to i64
  %420 = icmp samesign ugt i64 %416, %419
  %421 = zext i1 %420 to i64
  %422 = trunc i64 %416 to i32
  %423 = sub i32 %418, %422
  store i32 %423, ptr %417, align 4
  %424 = or disjoint i64 %404, 1
  %425 = load i32, ptr %1, align 4
  %426 = zext i32 %425 to i64
  %427 = icmp samesign ult i64 %424, %426
  br i1 %427, label %428, label %432

428:                                              ; preds = %414
  %429 = getelementptr inbounds nuw [4 x i8], ptr %348, i64 %424
  %430 = load i32, ptr %429, align 4
  %431 = zext i32 %430 to i64
  br label %432

432:                                              ; preds = %428, %414
  %433 = phi i64 [ %431, %428 ], [ 0, %414 ]
  %434 = add nuw nsw i64 %433, %421
  %435 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %424
  %436 = load i32, ptr %435, align 4
  %437 = zext i32 %436 to i64
  %438 = icmp samesign ugt i64 %434, %437
  %439 = zext i1 %438 to i64
  %440 = trunc i64 %434 to i32
  %441 = sub i32 %436, %440
  store i32 %441, ptr %435, align 4
  %442 = add nuw nsw i64 %404, 2
  %443 = add i64 %406, 2
  %444 = icmp eq i64 %443, %402
  br i1 %444, label %445, label %403

445:                                              ; preds = %432
  %446 = icmp eq i64 %399, 0
  br i1 %446, label %465, label %447

447:                                              ; preds = %445, %397
  %448 = phi i64 [ 0, %397 ], [ %442, %445 ]
  %449 = phi i64 [ 0, %397 ], [ %439, %445 ]
  %450 = trunc i32 %371 to i1
  tail call void @llvm.assume(i1 %450)
  %451 = load i32, ptr %1, align 4
  %452 = zext i32 %451 to i64
  %453 = icmp samesign ult i64 %448, %452
  br i1 %453, label %454, label %458

454:                                              ; preds = %447
  %455 = getelementptr inbounds nuw [4 x i8], ptr %348, i64 %448
  %456 = load i32, ptr %455, align 4
  %457 = zext i32 %456 to i64
  br label %458

458:                                              ; preds = %454, %447
  %459 = phi i64 [ %457, %454 ], [ 0, %447 ]
  %460 = add nuw nsw i64 %459, %449
  %461 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %448
  %462 = load i32, ptr %461, align 4
  %463 = trunc i64 %460 to i32
  %464 = sub i32 %462, %463
  store i32 %464, ptr %461, align 4
  br label %465

465:                                              ; preds = %445, %458
  %466 = add nsw i64 %398, -1
  %467 = and i64 %466, 4294967295
  %468 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %467
  %469 = load i32, ptr %468, align 4
  %470 = icmp eq i32 %469, 0
  br i1 %470, label %477, label %483

471:                                              ; preds = %477
  %472 = add nsw i64 %478, -1
  %473 = and i64 %472, 4294967295
  %474 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %473
  %475 = load i32, ptr %474, align 4
  %476 = icmp eq i32 %475, 0
  br i1 %476, label %477, label %482

477:                                              ; preds = %465, %471
  %478 = phi i64 [ %472, %471 ], [ %466, %465 ]
  %479 = trunc i64 %478 to i32
  %480 = icmp eq i32 %479, 0
  br i1 %480, label %481, label %471

481:                                              ; preds = %477
  store i32 %479, ptr %0, align 4
  br label %483

482:                                              ; preds = %471
  store i32 %479, ptr %0, align 4
  br label %483

483:                                              ; preds = %482, %377, %465, %481, %395
  %484 = phi i32 [ %370, %395 ], [ %370, %377 ], [ 0, %481 ], [ %370, %465 ], [ %479, %482 ]
  %485 = phi i32 [ 0, %395 ], [ 0, %377 ], [ 0, %481 ], [ %371, %465 ], [ %479, %482 ]
  %486 = or disjoint i64 %374, 1
  br label %487

487:                                              ; preds = %381, %483, %393
  %488 = phi i32 [ %370, %393 ], [ %484, %483 ], [ %370, %381 ]
  %489 = phi i32 [ %371, %393 ], [ %485, %483 ], [ %371, %381 ]
  %490 = phi i64 [ %374, %393 ], [ %486, %483 ], [ %374, %381 ]
  %491 = icmp eq i32 %489, 0
  br i1 %491, label %548, label %492

492:                                              ; preds = %487
  %493 = add i32 %489, -1
  %494 = zext i32 %493 to i64
  %495 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %494
  %496 = load i32, ptr %495, align 4
  %497 = lshr i32 %496, 31
  %498 = zext i32 %489 to i64
  %499 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %498
  store i32 %497, ptr %499, align 4
  %500 = icmp eq i32 %493, 0
  br i1 %500, label %519, label %501

501:                                              ; preds = %492
  %502 = and i32 %493, 1
  %503 = icmp eq i32 %502, 0
  br i1 %503, label %513, label %504

504:                                              ; preds = %501
  %505 = add i32 %489, -2
  %506 = zext i32 %505 to i64
  %507 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %506
  %508 = load i32, ptr %507, align 4
  %509 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %494
  %510 = load i32, ptr %509, align 4
  %511 = tail call i32 @llvm.fshl.i32(i32 %510, i32 %508, i32 1)
  store i32 %511, ptr %509, align 4
  %512 = add nsw i64 %494, -1
  br label %513

513:                                              ; preds = %504, %501
  %514 = phi i64 [ %494, %501 ], [ %512, %504 ]
  %515 = phi i32 [ %489, %501 ], [ %493, %504 ]
  %516 = icmp eq i32 %489, 2
  br i1 %516, label %517, label %526

517:                                              ; preds = %526, %513
  %518 = load i32, ptr %499, align 4
  br label %519

519:                                              ; preds = %517, %492
  %520 = phi i32 [ %518, %517 ], [ %497, %492 ]
  %521 = load i32, ptr %349, align 4
  %522 = shl i32 %521, 1
  store i32 %522, ptr %349, align 4
  %523 = icmp ne i32 %520, 0
  %524 = zext i1 %523 to i32
  %525 = add i32 %489, %524
  store i32 %525, ptr %0, align 4
  br label %548

526:                                              ; preds = %513, %526
  %527 = phi i64 [ %545, %526 ], [ %514, %513 ]
  %528 = phi i32 [ %544, %526 ], [ %515, %513 ]
  %529 = add i32 %528, -2
  %530 = zext i32 %529 to i64
  %531 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %530
  %532 = load i32, ptr %531, align 4
  %533 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %527
  %534 = load i32, ptr %533, align 4
  %535 = tail call i32 @llvm.fshl.i32(i32 %534, i32 %532, i32 1)
  store i32 %535, ptr %533, align 4
  %536 = add i64 %527, 4294967294
  %537 = and i64 %536, 4294967295
  %538 = getelementptr inbounds nuw [4 x i8], ptr %349, i64 %537
  %539 = load i32, ptr %538, align 4
  %540 = getelementptr [4 x i8], ptr %0, i64 %527
  %541 = load i32, ptr %540, align 4
  %542 = tail call i32 @llvm.fshl.i32(i32 %541, i32 %539, i32 1)
  %543 = trunc i64 %527 to i32
  %544 = add i32 %543, -1
  store i32 %542, ptr %540, align 4
  %545 = add nsw i64 %527, -2
  %546 = and i64 %545, 4294967295
  %547 = icmp eq i64 %546, 0
  br i1 %547, label %517, label %526

548:                                              ; preds = %487, %519
  %549 = phi i32 [ %488, %487 ], [ %525, %519 ]
  %550 = phi i32 [ 0, %487 ], [ %525, %519 ]
  %551 = add nuw nsw i32 %373, 1
  %552 = icmp eq i32 %551, %350
  br i1 %552, label %351, label %369

553:                                              ; preds = %351
  %554 = sub i64 %343, %361
  %555 = add i64 %554, %367
  %556 = icmp slt i64 %555, 0
  br i1 %556, label %596, label %557

557:                                              ; preds = %553, %351
  %558 = phi i64 [ %555, %553 ], [ %367, %351 ]
  %559 = trunc i64 %558 to i32
  %560 = sub i32 %345, %559
  %561 = zext i32 %560 to i64
  %562 = lshr i64 %353, %561
  %563 = shl nsw i64 -1, %561
  %564 = xor i64 %563, -1
  %565 = and i64 %353, %564
  %566 = add i32 %560, -1
  %567 = zext nneg i32 %566 to i64
  %568 = shl nuw i64 1, %567
  %569 = icmp ugt i64 %565, %568
  br i1 %569, label %577, label %570

570:                                              ; preds = %557
  %571 = icmp eq i64 %565, %568
  br i1 %571, label %572, label %579

572:                                              ; preds = %570
  %573 = icmp eq i32 %356, 0
  %574 = and i64 %562, 1
  %575 = icmp eq i64 %574, 0
  %576 = select i1 %573, i1 %575, i1 false
  br i1 %576, label %579, label %577

577:                                              ; preds = %572, %557
  %578 = add i64 %562, 1
  br label %579

579:                                              ; preds = %572, %577, %570
  %580 = phi i64 [ %578, %577 ], [ %562, %572 ], [ %562, %570 ]
  br i1 %368, label %596, label %581

581:                                              ; preds = %579
  %582 = zext nneg i32 %344 to i64
  %583 = lshr i64 %580, %582
  %584 = icmp ne i64 %583, 0
  %585 = zext i1 %584 to i64
  %586 = add nsw i64 %343, %585
  %587 = icmp sgt i64 %586, %360
  br i1 %587, label %596, label %588

588:                                              ; preds = %581
  %589 = lshr i64 %580, %585
  %590 = shl nsw i64 -1, %365
  %591 = xor i64 %590, -1
  %592 = and i64 %589, %591
  %593 = add nsw i64 %586, %360
  %594 = shl i64 %593, %365
  %595 = or i64 %594, %592
  br label %596

596:                                              ; preds = %588, %579, %581, %553
  %597 = phi i64 [ 0, %553 ], [ %595, %588 ], [ %580, %579 ], [ %366, %581 ]
  ret i64 %597
}

declare i32 @llvm.ctlz.i32(i32, i1 immarg)

declare void @llvm.memset.p0.i64(ptr writeonly captures(none), i8, i64, i1 immarg)

declare i32 @llvm.fshl.i32(i32, i32, i32)

declare i32 @llvm.smax.i32(i32, i32)

declare void @llvm.assume(i1 noundef)

