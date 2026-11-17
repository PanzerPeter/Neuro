;
; Number to text: the fast path behind an interpolated float hole, linked into a module
; that renders one. Exact `%.Nf` and `%.16g` digits in 128-bit integer arithmetic, with
; the C library's `snprintf` left for what it does not cover.
;
; Generated from compiler/llvm-backend/src/codegen/text_format.c with clang -O2
; -emit-llvm, then stripped of the target datalayout and triple, attribute groups and
; metadata. Regenerate it with tools/regen_gpu_runtime.sh.
;

@POW10 = internal unnamed_addr constant [20 x i64] [i64 1, i64 10, i64 100, i64 1000, i64 10000, i64 100000, i64 1000000, i64 10000000, i64 100000000, i64 1000000000, i64 10000000000, i64 100000000000, i64 1000000000000, i64 10000000000000, i64 100000000000000, i64 1000000000000000, i64 10000000000000000, i64 100000000000000000, i64 1000000000000000000, i64 -8446744073709551616], align 16

define dso_local i64 @__neuro_format_f64(double noundef %0, i32 noundef %1, i32 noundef %2, ptr noundef %3) local_unnamed_addr {
  %5 = alloca [16 x i8], align 16
  %6 = bitcast double %0 to i64
  %7 = lshr i64 %6, 52
  %8 = trunc nuw nsw i64 %7 to i32
  %9 = and i32 %8, 2047
  %10 = and i64 %6, 4503599627370495
  switch i32 %9, label %11 [
    i32 2047, label %468
    i32 0, label %14
  ]

11:                                               ; preds = %4
  %12 = or disjoint i64 %10, 4503599627370496
  %13 = add nsw i32 %9, -1075
  br label %14

14:                                               ; preds = %4, %11
  %15 = phi i64 [ %12, %11 ], [ %10, %4 ]
  %16 = phi i32 [ %13, %11 ], [ -1074, %4 ]
  %17 = icmp eq i32 %1, -1
  br i1 %17, label %18, label %319

18:                                               ; preds = %14
  %19 = icmp sgt i64 %6, -1
  br i1 %19, label %20, label %22

20:                                               ; preds = %18
  %21 = icmp eq i32 %2, 0
  br i1 %21, label %25, label %22

22:                                               ; preds = %20, %18
  %23 = phi i8 [ 45, %18 ], [ 43, %20 ]
  %24 = getelementptr inbounds nuw i8, ptr %3, i64 1
  store i8 %23, ptr %3, align 1
  br label %25

25:                                               ; preds = %22, %20
  %26 = phi ptr [ %3, %20 ], [ %24, %22 ]
  %27 = icmp eq i64 %15, 0
  br i1 %27, label %28, label %33

28:                                               ; preds = %25
  %29 = getelementptr inbounds nuw i8, ptr %26, i64 1
  store i8 48, ptr %26, align 1
  %30 = ptrtoint ptr %29 to i64
  %31 = ptrtoint ptr %3 to i64
  %32 = sub i64 %30, %31
  br label %468

33:                                               ; preds = %25
  %34 = tail call range(i64 11, 65) i64 @llvm.ctlz.i64(i64 range(i64 0, 9007199254740992) %15, i1 true)
  %35 = trunc nuw nsw i64 %34 to i32
  %36 = xor i32 %35, 63
  %37 = add nsw i32 %36, %16
  %38 = add nsw i32 %37, -54
  %39 = icmp ult i32 %38, -78
  br i1 %39, label %468, label %40

40:                                               ; preds = %33
  %41 = mul nsw i32 %37, 78913
  %42 = ashr i32 %41, 18
  %43 = zext nneg i64 %15 to i128
  %44 = sub nsw i32 0, %16
  %45 = zext nneg i32 %44 to i128
  %46 = zext nneg i32 %16 to i64
  %47 = shl i64 %15, %46
  %48 = zext i64 %47 to i128
  %49 = sub nsw i32 15, %42
  %50 = icmp slt i32 %42, -7
  br i1 %50, label %468, label %51

51:                                               ; preds = %40
  %52 = icmp sgt i32 %16, -1
  br i1 %52, label %63, label %53

53:                                               ; preds = %51
  %54 = icmp samesign ugt i32 %49, 19
  %55 = zext nneg i32 %49 to i64
  %56 = getelementptr [8 x i8], ptr @POW10, i64 %55
  br i1 %54, label %57, label %61

57:                                               ; preds = %53
  %58 = getelementptr i8, ptr %56, i64 -152
  %59 = load i64, ptr %58, align 8
  %60 = mul i64 %59, %15
  br label %71

61:                                               ; preds = %53
  %62 = load i64, ptr %56, align 8
  br label %71

63:                                               ; preds = %51
  %64 = zext nneg i32 %49 to i64
  %65 = getelementptr inbounds nuw [8 x i8], ptr @POW10, i64 %64
  %66 = load i64, ptr %65, align 8
  %67 = zext i64 %66 to i128
  %68 = mul nuw i128 %67, %48
  %69 = icmp ugt i128 %68, 9999999999999999
  %70 = trunc nuw nsw i128 %68 to i64
  br i1 %69, label %102, label %137

71:                                               ; preds = %61, %57
  %72 = phi i64 [ %62, %61 ], [ %60, %57 ]
  %73 = phi i128 [ %43, %61 ], [ 10000000000000000000, %57 ]
  %74 = zext i64 %72 to i128
  %75 = mul nuw i128 %73, %74
  %76 = lshr i128 %75, %45
  %77 = icmp samesign ugt i128 %76, 9999999999999999
  br i1 %77, label %105, label %78

78:                                               ; preds = %120, %71
  %79 = phi i32 [ %42, %71 ], [ %106, %120 ]
  %80 = phi i128 [ %75, %71 ], [ %124, %120 ]
  %81 = phi i128 [ %76, %71 ], [ %125, %120 ]
  %82 = icmp samesign ugt i32 %44, 117
  br i1 %82, label %468, label %83

83:                                               ; preds = %78
  %84 = shl nsw i128 -1, %45
  %85 = xor i128 %84, -1
  %86 = and i128 %80, %85
  %87 = xor i32 %16, -1
  %88 = zext nneg i32 %87 to i128
  %89 = shl nuw nsw i128 1, %88
  %90 = icmp samesign ugt i128 %86, %89
  br i1 %90, label %91, label %93

91:                                               ; preds = %83
  %92 = trunc nuw nsw i128 %81 to i64
  br label %99

93:                                               ; preds = %83
  %94 = icmp ne i128 %86, %89
  %95 = and i128 %81, 1
  %96 = icmp eq i128 %95, 0
  %97 = or i1 %94, %96
  %98 = trunc nuw nsw i128 %81 to i64
  br i1 %97, label %137, label %99

99:                                               ; preds = %93, %91
  %100 = phi i64 [ %92, %91 ], [ %98, %93 ]
  %101 = add nuw nsw i64 %100, 1
  br label %137

102:                                              ; preds = %63
  %103 = add nsw i32 %42, -15
  %104 = icmp ult i32 %103, -23
  br i1 %104, label %468, label %127

105:                                              ; preds = %71
  %106 = add nsw i32 %42, 1
  %107 = add nsw i32 %42, -15
  %108 = icmp ult i32 %107, -23
  br i1 %108, label %468, label %109

109:                                              ; preds = %105
  %110 = sub nsw i32 14, %42
  %111 = icmp samesign ugt i32 %110, 19
  %112 = zext nneg i32 %110 to i64
  %113 = getelementptr [8 x i8], ptr @POW10, i64 %112
  br i1 %111, label %116, label %114

114:                                              ; preds = %109
  %115 = load i64, ptr %113, align 8
  br label %120

116:                                              ; preds = %109
  %117 = getelementptr i8, ptr %113, i64 -152
  %118 = load i64, ptr %117, align 8
  %119 = mul i64 %118, %15
  br label %120

120:                                              ; preds = %116, %114
  %121 = phi i64 [ %115, %114 ], [ %119, %116 ]
  %122 = phi i128 [ %43, %114 ], [ 10000000000000000000, %116 ]
  %123 = zext i64 %121 to i128
  %124 = mul nuw i128 %122, %123
  %125 = lshr i128 %124, %45
  %126 = icmp samesign ugt i128 %125, 9999999999999999
  br i1 %126, label %468, label %78

127:                                              ; preds = %102
  %128 = sub nsw i32 14, %42
  %129 = add nuw nsw i32 %42, 1
  %130 = zext nneg i32 %128 to i64
  %131 = getelementptr inbounds nuw [8 x i8], ptr @POW10, i64 %130
  %132 = load i64, ptr %131, align 8
  %133 = zext i64 %132 to i128
  %134 = mul nuw i128 %133, %48
  %135 = icmp ugt i128 %134, 9999999999999999
  %136 = trunc nuw nsw i128 %134 to i64
  br i1 %135, label %468, label %137

137:                                              ; preds = %127, %99, %93, %63
  %138 = phi i32 [ %79, %93 ], [ %42, %63 ], [ %129, %127 ], [ %79, %99 ]
  %139 = phi i64 [ %98, %93 ], [ %70, %63 ], [ %136, %127 ], [ %101, %99 ]
  switch i64 %139, label %142 [
    i64 0, label %468
    i64 10000000000000000, label %140
  ]

140:                                              ; preds = %137
  %141 = add nsw i32 %138, 1
  br label %142

142:                                              ; preds = %140, %137
  %143 = phi i64 [ 1000000000000000, %140 ], [ %139, %137 ]
  %144 = phi i32 [ %141, %140 ], [ %138, %137 ]
  call void @llvm.lifetime.start.p0(ptr nonnull %5)
  %145 = getelementptr inbounds nuw i8, ptr %5, i64 16
  br label %146

146:                                              ; preds = %146, %142
  %147 = phi i32 [ 0, %142 ], [ %160, %146 ]
  %148 = phi ptr [ %145, %142 ], [ %158, %146 ]
  %149 = phi i64 [ %143, %142 ], [ %159, %146 ]
  %150 = urem i64 %149, 10
  %151 = trunc nuw nsw i64 %150 to i8
  %152 = or disjoint i8 %151, 48
  %153 = getelementptr inbounds i8, ptr %148, i64 -1
  store i8 %152, ptr %153, align 1
  %154 = udiv i64 %149, 10
  %155 = urem i64 %154, 10
  %156 = trunc nuw nsw i64 %155 to i8
  %157 = or disjoint i8 %156, 48
  %158 = getelementptr inbounds i8, ptr %148, i64 -2
  store i8 %157, ptr %158, align 1
  %159 = udiv i64 %149, 100
  %160 = add nuw nsw i32 %147, 2
  %161 = icmp eq i32 %160, 16
  br i1 %161, label %162, label %146

162:                                              ; preds = %146
  %163 = getelementptr inbounds nuw i8, ptr %5, i64 15
  %164 = load i8, ptr %163, align 1
  %165 = icmp eq i8 %164, 48
  br i1 %165, label %166, label %225

166:                                              ; preds = %162
  %167 = getelementptr inbounds nuw i8, ptr %5, i64 14
  %168 = load i8, ptr %167, align 2
  %169 = icmp eq i8 %168, 48
  br i1 %169, label %170, label %225

170:                                              ; preds = %166
  %171 = getelementptr inbounds nuw i8, ptr %5, i64 13
  %172 = load i8, ptr %171, align 1
  %173 = icmp eq i8 %172, 48
  br i1 %173, label %174, label %225

174:                                              ; preds = %170
  %175 = getelementptr inbounds nuw i8, ptr %5, i64 12
  %176 = load i8, ptr %175, align 4
  %177 = icmp eq i8 %176, 48
  br i1 %177, label %178, label %225

178:                                              ; preds = %174
  %179 = getelementptr inbounds nuw i8, ptr %5, i64 11
  %180 = load i8, ptr %179, align 1
  %181 = icmp eq i8 %180, 48
  br i1 %181, label %182, label %225

182:                                              ; preds = %178
  %183 = getelementptr inbounds nuw i8, ptr %5, i64 10
  %184 = load i8, ptr %183, align 2
  %185 = icmp eq i8 %184, 48
  br i1 %185, label %186, label %225

186:                                              ; preds = %182
  %187 = getelementptr inbounds nuw i8, ptr %5, i64 9
  %188 = load i8, ptr %187, align 1
  %189 = icmp eq i8 %188, 48
  br i1 %189, label %190, label %225

190:                                              ; preds = %186
  %191 = getelementptr inbounds nuw i8, ptr %5, i64 8
  %192 = load i8, ptr %191, align 8
  %193 = icmp eq i8 %192, 48
  br i1 %193, label %194, label %225

194:                                              ; preds = %190
  %195 = getelementptr inbounds nuw i8, ptr %5, i64 7
  %196 = load i8, ptr %195, align 1
  %197 = icmp eq i8 %196, 48
  br i1 %197, label %198, label %225

198:                                              ; preds = %194
  %199 = getelementptr inbounds nuw i8, ptr %5, i64 6
  %200 = load i8, ptr %199, align 2
  %201 = icmp eq i8 %200, 48
  br i1 %201, label %202, label %225

202:                                              ; preds = %198
  %203 = getelementptr inbounds nuw i8, ptr %5, i64 5
  %204 = load i8, ptr %203, align 1
  %205 = icmp eq i8 %204, 48
  br i1 %205, label %206, label %225

206:                                              ; preds = %202
  %207 = getelementptr inbounds nuw i8, ptr %5, i64 4
  %208 = load i8, ptr %207, align 4
  %209 = icmp eq i8 %208, 48
  br i1 %209, label %210, label %225

210:                                              ; preds = %206
  %211 = getelementptr inbounds nuw i8, ptr %5, i64 3
  %212 = load i8, ptr %211, align 1
  %213 = icmp eq i8 %212, 48
  br i1 %213, label %214, label %225

214:                                              ; preds = %210
  %215 = getelementptr inbounds nuw i8, ptr %5, i64 2
  %216 = load i8, ptr %215, align 2
  %217 = icmp eq i8 %216, 48
  br i1 %217, label %218, label %225

218:                                              ; preds = %214
  %219 = getelementptr inbounds nuw i8, ptr %5, i64 1
  %220 = load i8, ptr %219, align 1
  %221 = icmp eq i8 %220, 48
  br i1 %221, label %222, label %225

222:                                              ; preds = %218
  %223 = add nsw i32 %144, -16
  %224 = icmp ult i32 %223, -20
  br i1 %224, label %229, label %278

225:                                              ; preds = %218, %214, %210, %206, %202, %198, %194, %190, %186, %182, %178, %174, %170, %166, %162
  %226 = phi i32 [ 16, %162 ], [ 15, %166 ], [ 14, %170 ], [ 13, %174 ], [ 12, %178 ], [ 11, %182 ], [ 10, %186 ], [ 9, %190 ], [ 8, %194 ], [ 7, %198 ], [ 6, %202 ], [ 5, %206 ], [ 4, %210 ], [ 3, %214 ], [ 2, %218 ]
  %227 = add nsw i32 %144, -16
  %228 = icmp ult i32 %227, -20
  br i1 %228, label %232, label %278

229:                                              ; preds = %222
  %230 = load i8, ptr %5, align 16
  %231 = getelementptr inbounds nuw i8, ptr %26, i64 1
  store i8 %230, ptr %26, align 1
  br label %240

232:                                              ; preds = %225
  %233 = load i8, ptr %5, align 16
  %234 = getelementptr inbounds nuw i8, ptr %26, i64 1
  store i8 %233, ptr %26, align 1
  %235 = getelementptr inbounds nuw i8, ptr %26, i64 2
  store i8 46, ptr %234, align 1
  %236 = getelementptr inbounds nuw i8, ptr %5, i64 1
  %237 = add nsw i32 %226, -1
  %238 = zext nneg i32 %237 to i64
  call void @llvm.memcpy.p0.p0.i64(ptr nonnull align 1 %235, ptr nonnull align 1 %236, i64 %238, i1 false)
  %239 = getelementptr inbounds nuw i8, ptr %235, i64 %238
  br label %240

240:                                              ; preds = %232, %229
  %241 = phi ptr [ %239, %232 ], [ %231, %229 ]
  %242 = getelementptr inbounds nuw i8, ptr %241, i64 1
  store i8 101, ptr %241, align 1
  %243 = icmp slt i32 %144, 0
  br i1 %243, label %246, label %244

244:                                              ; preds = %240
  store i8 43, ptr %242, align 1
  %245 = zext nneg i32 %144 to i64
  br label %248

246:                                              ; preds = %240
  store i8 45, ptr %242, align 1
  %247 = sub nsw i32 0, %144
  br label %254

248:                                              ; preds = %248, %244
  %249 = phi i32 [ %252, %248 ], [ 1, %244 ]
  %250 = phi i64 [ %251, %248 ], [ %245, %244 ]
  %251 = udiv i64 %250, 10
  %252 = add nuw nsw i32 %249, 1
  %253 = icmp samesign ugt i64 %250, 99
  br i1 %253, label %248, label %254

254:                                              ; preds = %248, %246
  %255 = phi i32 [ %247, %246 ], [ %144, %248 ]
  %256 = phi i32 [ 2, %246 ], [ %252, %248 ]
  %257 = getelementptr inbounds nuw i8, ptr %241, i64 2
  %258 = sext i32 %256 to i64
  %259 = getelementptr inbounds i8, ptr %257, i64 %258
  %260 = and i32 %256, 1
  %261 = and i32 %256, -2
  br label %262

262:                                              ; preds = %262, %254
  %263 = phi ptr [ %259, %254 ], [ %274, %262 ]
  %264 = phi i32 [ %255, %254 ], [ %275, %262 ]
  %265 = phi i32 [ 0, %254 ], [ %276, %262 ]
  %266 = urem i32 %264, 10
  %267 = trunc nuw nsw i32 %266 to i8
  %268 = or disjoint i8 %267, 48
  %269 = getelementptr inbounds i8, ptr %263, i64 -1
  store i8 %268, ptr %269, align 1
  %270 = udiv i32 %264, 10
  %271 = urem i32 %270, 10
  %272 = trunc nuw nsw i32 %271 to i8
  %273 = or disjoint i8 %272, 48
  %274 = getelementptr inbounds i8, ptr %263, i64 -2
  store i8 %273, ptr %274, align 1
  %275 = udiv i32 %264, 100
  %276 = add nuw i32 %265, 2
  %277 = icmp eq i32 %276, %261
  br i1 %277, label %306, label %262

278:                                              ; preds = %225, %222
  %279 = phi i32 [ 1, %222 ], [ %226, %225 ]
  %280 = icmp slt i32 %144, 0
  br i1 %280, label %281, label %295

281:                                              ; preds = %278
  %282 = getelementptr inbounds nuw i8, ptr %26, i64 1
  store i8 48, ptr %26, align 1
  %283 = getelementptr i8, ptr %26, i64 2
  store i8 46, ptr %282, align 1
  %284 = icmp eq i32 %144, -1
  br i1 %284, label %291, label %285

285:                                              ; preds = %281
  %286 = sub nuw nsw i32 -2, %144
  %287 = zext nneg i32 %286 to i64
  %288 = add nuw nsw i64 %287, 1
  tail call void @llvm.memset.p0.i64(ptr noundef nonnull align 1 dereferenceable(1) %283, i8 48, i64 %288, i1 false)
  %289 = getelementptr i8, ptr %26, i64 %287
  %290 = getelementptr i8, ptr %289, i64 3
  br label %291

291:                                              ; preds = %285, %281
  %292 = phi ptr [ %283, %281 ], [ %290, %285 ]
  %293 = zext nneg i32 %279 to i64
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 1 dereferenceable(1) %292, ptr noundef nonnull align 16 dereferenceable(1) %5, i64 %293, i1 false)
  %294 = getelementptr inbounds nuw i8, ptr %292, i64 %293
  br label %314

295:                                              ; preds = %278
  %296 = add nuw nsw i32 %144, 1
  %297 = zext nneg i32 %296 to i64
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 1 dereferenceable(1) %26, ptr noundef nonnull align 16 dereferenceable(1) %5, i64 %297, i1 false)
  %298 = getelementptr inbounds nuw i8, ptr %26, i64 %297
  %299 = icmp samesign ugt i32 %279, %296
  br i1 %299, label %300, label %314

300:                                              ; preds = %295
  %301 = getelementptr inbounds nuw i8, ptr %298, i64 1
  store i8 46, ptr %298, align 1
  %302 = getelementptr inbounds nuw i8, ptr %5, i64 %297
  %303 = sub nuw nsw i32 %279, %296
  %304 = zext nneg i32 %303 to i64
  call void @llvm.memcpy.p0.p0.i64(ptr nonnull align 1 %301, ptr nonnull align 1 %302, i64 %304, i1 false)
  %305 = getelementptr inbounds nuw i8, ptr %301, i64 %304
  br label %314

306:                                              ; preds = %262
  %307 = icmp eq i32 %260, 0
  br i1 %307, label %314, label %308

308:                                              ; preds = %306
  %309 = trunc i32 %256 to i1
  tail call void @llvm.assume(i1 %309)
  %310 = urem i32 %275, 10
  %311 = trunc nuw nsw i32 %310 to i8
  %312 = or disjoint i8 %311, 48
  %313 = getelementptr inbounds i8, ptr %274, i64 -1
  store i8 %312, ptr %313, align 1
  br label %314

314:                                              ; preds = %308, %306, %300, %295, %291
  %315 = phi ptr [ %298, %295 ], [ %294, %291 ], [ %305, %300 ], [ %259, %306 ], [ %259, %308 ]
  %316 = ptrtoint ptr %315 to i64
  %317 = ptrtoint ptr %3 to i64
  %318 = sub i64 %316, %317
  call void @llvm.lifetime.end.p0(ptr nonnull %5)
  br label %468

319:                                              ; preds = %14
  %320 = icmp ugt i32 %1, 19
  br i1 %320, label %468, label %321

321:                                              ; preds = %319
  %322 = icmp eq i64 %15, 0
  %323 = select i1 %322, i32 -1, i32 %16
  %324 = icmp sgt i32 %323, -1
  br i1 %324, label %325, label %336

325:                                              ; preds = %321
  %326 = icmp samesign ugt i32 %323, 11
  br i1 %326, label %468, label %327

327:                                              ; preds = %325
  %328 = zext nneg i32 %323 to i64
  %329 = shl nuw i64 %15, %328
  %330 = zext i64 %329 to i128
  %331 = zext nneg i32 %1 to i64
  %332 = getelementptr inbounds nuw [8 x i8], ptr @POW10, i64 %331
  %333 = load i64, ptr %332, align 8
  %334 = zext i64 %333 to i128
  %335 = mul nuw i128 %334, %330
  br label %366

336:                                              ; preds = %321
  %337 = sub nsw i32 0, %323
  %338 = icmp samesign ugt i32 %337, 117
  br i1 %338, label %339, label %343

339:                                              ; preds = %336
  %340 = zext nneg i32 %1 to i64
  %341 = getelementptr inbounds nuw [8 x i8], ptr @POW10, i64 %340
  %342 = load i64, ptr %341, align 8
  br label %373

343:                                              ; preds = %336
  %344 = zext nneg i64 %15 to i128
  %345 = zext nneg i32 %1 to i64
  %346 = getelementptr inbounds nuw [8 x i8], ptr @POW10, i64 %345
  %347 = load i64, ptr %346, align 8
  %348 = zext i64 %347 to i128
  %349 = mul nuw nsw i128 %348, %344
  %350 = zext nneg i32 %337 to i128
  %351 = lshr i128 %349, %350
  %352 = shl nsw i128 -1, %350
  %353 = xor i128 %352, -1
  %354 = and i128 %349, %353
  %355 = xor i32 %323, -1
  %356 = zext nneg i32 %355 to i128
  %357 = shl nuw nsw i128 1, %356
  %358 = icmp samesign ugt i128 %354, %357
  br i1 %358, label %364, label %359

359:                                              ; preds = %343
  %360 = icmp ne i128 %354, %357
  %361 = and i128 %351, 1
  %362 = icmp eq i128 %361, 0
  %363 = select i1 %360, i1 true, i1 %362
  br i1 %363, label %366, label %364

364:                                              ; preds = %359, %343
  %365 = add nuw nsw i128 %351, 1
  br label %366

366:                                              ; preds = %364, %359, %327
  %367 = phi i64 [ %333, %327 ], [ %347, %359 ], [ %347, %364 ]
  %368 = phi i128 [ %335, %327 ], [ %351, %359 ], [ %365, %364 ]
  %369 = icmp ult i128 %368, 18446744073709551616
  br i1 %369, label %370, label %468

370:                                              ; preds = %366
  %371 = trunc nuw i128 %368 to i64
  %372 = zext nneg i32 %1 to i64
  br label %373

373:                                              ; preds = %370, %339
  %374 = phi i64 [ %372, %370 ], [ %340, %339 ]
  %375 = phi i64 [ %367, %370 ], [ %342, %339 ]
  %376 = phi i64 [ %371, %370 ], [ 0, %339 ]
  %377 = udiv i64 %376, %375
  %378 = urem i64 %376, %375
  %379 = icmp sgt i64 %6, -1
  br i1 %379, label %380, label %382

380:                                              ; preds = %373
  %381 = icmp eq i32 %2, 0
  br i1 %381, label %385, label %382

382:                                              ; preds = %380, %373
  %383 = phi i8 [ 45, %373 ], [ 43, %380 ]
  %384 = getelementptr inbounds nuw i8, ptr %3, i64 1
  store i8 %383, ptr %3, align 1
  br label %385

385:                                              ; preds = %382, %380
  %386 = phi ptr [ %3, %380 ], [ %384, %382 ]
  %387 = icmp ugt i64 %377, 9
  br i1 %387, label %388, label %394

388:                                              ; preds = %385, %388
  %389 = phi i32 [ %392, %388 ], [ 1, %385 ]
  %390 = phi i64 [ %391, %388 ], [ %377, %385 ]
  %391 = udiv i64 %390, 10
  %392 = add nuw nsw i32 %389, 1
  %393 = icmp ugt i64 %390, 99
  br i1 %393, label %388, label %394

394:                                              ; preds = %388, %385
  %395 = phi i32 [ 1, %385 ], [ %392, %388 ]
  %396 = sext i32 %395 to i64
  %397 = getelementptr inbounds i8, ptr %386, i64 %396
  %398 = and i32 %395, 1
  %399 = icmp eq i32 %395, 1
  br i1 %399, label %420, label %400

400:                                              ; preds = %394
  %401 = and i32 %395, -2
  br label %402

402:                                              ; preds = %402, %400
  %403 = phi ptr [ %397, %400 ], [ %414, %402 ]
  %404 = phi i64 [ %377, %400 ], [ %415, %402 ]
  %405 = phi i32 [ 0, %400 ], [ %416, %402 ]
  %406 = urem i64 %404, 10
  %407 = trunc nuw nsw i64 %406 to i8
  %408 = or disjoint i8 %407, 48
  %409 = getelementptr inbounds i8, ptr %403, i64 -1
  store i8 %408, ptr %409, align 1
  %410 = udiv i64 %404, 10
  %411 = urem i64 %410, 10
  %412 = trunc nuw nsw i64 %411 to i8
  %413 = or disjoint i8 %412, 48
  %414 = getelementptr inbounds i8, ptr %403, i64 -2
  store i8 %413, ptr %414, align 1
  %415 = udiv i64 %404, 100
  %416 = add i32 %405, 2
  %417 = icmp eq i32 %416, %401
  br i1 %417, label %418, label %402

418:                                              ; preds = %402
  %419 = icmp eq i32 %398, 0
  br i1 %419, label %428, label %420

420:                                              ; preds = %418, %394
  %421 = phi ptr [ %397, %394 ], [ %414, %418 ]
  %422 = phi i64 [ %377, %394 ], [ %415, %418 ]
  %423 = trunc i32 %395 to i1
  tail call void @llvm.assume(i1 %423)
  %424 = urem i64 %422, 10
  %425 = trunc nuw nsw i64 %424 to i8
  %426 = or disjoint i8 %425, 48
  %427 = getelementptr inbounds i8, ptr %421, i64 -1
  store i8 %426, ptr %427, align 1
  br label %428

428:                                              ; preds = %418, %420
  %429 = icmp eq i32 %1, 0
  br i1 %429, label %463, label %430

430:                                              ; preds = %428
  %431 = getelementptr inbounds nuw i8, ptr %397, i64 1
  store i8 46, ptr %397, align 1
  %432 = getelementptr inbounds nuw i8, ptr %431, i64 %374
  %433 = and i32 %1, 1
  %434 = icmp eq i32 %1, 1
  br i1 %434, label %455, label %435

435:                                              ; preds = %430
  %436 = and i32 %1, 30
  br label %437

437:                                              ; preds = %437, %435
  %438 = phi ptr [ %432, %435 ], [ %449, %437 ]
  %439 = phi i64 [ %378, %435 ], [ %450, %437 ]
  %440 = phi i32 [ 0, %435 ], [ %451, %437 ]
  %441 = urem i64 %439, 10
  %442 = trunc nuw nsw i64 %441 to i8
  %443 = or disjoint i8 %442, 48
  %444 = getelementptr inbounds i8, ptr %438, i64 -1
  store i8 %443, ptr %444, align 1
  %445 = udiv i64 %439, 10
  %446 = urem i64 %445, 10
  %447 = trunc nuw nsw i64 %446 to i8
  %448 = or disjoint i8 %447, 48
  %449 = getelementptr inbounds i8, ptr %438, i64 -2
  store i8 %448, ptr %449, align 1
  %450 = udiv i64 %439, 100
  %451 = add i32 %440, 2
  %452 = icmp eq i32 %451, %436
  br i1 %452, label %453, label %437

453:                                              ; preds = %437
  %454 = icmp eq i32 %433, 0
  br i1 %454, label %463, label %455

455:                                              ; preds = %453, %430
  %456 = phi ptr [ %432, %430 ], [ %449, %453 ]
  %457 = phi i64 [ %378, %430 ], [ %450, %453 ]
  %458 = trunc i32 %1 to i1
  tail call void @llvm.assume(i1 %458)
  %459 = urem i64 %457, 10
  %460 = trunc nuw nsw i64 %459 to i8
  %461 = or disjoint i8 %460, 48
  %462 = getelementptr inbounds i8, ptr %456, i64 -1
  store i8 %461, ptr %462, align 1
  br label %463

463:                                              ; preds = %455, %453, %428
  %464 = phi ptr [ %397, %428 ], [ %432, %453 ], [ %432, %455 ]
  %465 = ptrtoint ptr %464 to i64
  %466 = ptrtoint ptr %3 to i64
  %467 = sub i64 %465, %466
  br label %468

468:                                              ; preds = %463, %366, %325, %314, %137, %127, %120, %105, %102, %78, %40, %33, %28, %319, %4
  %469 = phi i64 [ -1, %4 ], [ -1, %319 ], [ -1, %120 ], [ %32, %28 ], [ -1, %33 ], [ -1, %137 ], [ %318, %314 ], [ -1, %40 ], [ -1, %105 ], [ -1, %102 ], [ -1, %78 ], [ -1, %127 ], [ %467, %463 ], [ -1, %325 ], [ -1, %366 ]
  ret i64 %469
}

declare void @llvm.lifetime.start.p0(ptr captures(none))

declare void @llvm.lifetime.end.p0(ptr captures(none))

declare i64 @llvm.ctlz.i64(i64, i1 immarg)

declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly captures(none), ptr noalias readonly captures(none), i64, i1 immarg)

declare void @llvm.memset.p0.i64(ptr writeonly captures(none), i8, i64, i1 immarg)

declare void @llvm.assume(i1 noundef)

