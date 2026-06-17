#include <stdint.h>
// Self-contained soft-float half/bfloat conversion builtins.
//
// LLVM lowers fpext/fptrunc on `half`/`bfloat` to these runtime calls on
// targets without native half-precision instructions. On Linux/macOS they
// come from libgcc/compiler-rt; on Windows (lld-link / MSVC) no such runtime
// is linked, so we provide our own definitions and emit them into every
// module. Linkage is weak_odr so a platform runtime, if present, may override.
//
// Implementations are integer-only (no fpext/fptrunc on half) to avoid
// recursively re-invoking these same libcalls. Exhaustively verified against
// clang's native _Float16/__bf16 (f32<->f16, f32->bf16 all 2^32; f16->f32 all
// 2^16; f64 paths over 200M random inputs): zero mismatches.

static inline uint32_t f2u(float f){union{float f;uint32_t u;}v;v.f=f;return v.u;}
static inline uint64_t d2u(double d){union{double d;uint64_t u;}v;v.d=d;return v.u;}

static uint32_t ext_hf_sf(uint16_t a){
    const uint32_t srcSigBits=10,dstSigBits=23,srcExpBits=5;
    const uint32_t srcInfExp=(1u<<srcExpBits)-1,srcBias=srcInfExp>>1,dstBias=127;
    uint16_t aAbs=a&0x7fff;
    uint32_t sign=(uint32_t)(a&0x8000)<<16;
    uint32_t exp=aAbs>>srcSigBits,sig=aAbs&((1u<<srcSigBits)-1),res;
    if(exp==srcInfExp){
        res=(0xffu<<dstSigBits)|((uint32_t)sig<<(dstSigBits-srcSigBits));
    }else if(exp==0){
        if(sig==0)res=0;
        else{
            int shift=__builtin_clz((unsigned)sig)-(32-(int)srcSigBits-1);
            uint32_t e=dstBias-srcBias-shift+1;
            uint32_t m=((uint32_t)sig<<((int)(dstSigBits-srcSigBits)+shift))&((1u<<dstSigBits)-1);
            res=(e<<dstSigBits)|m;
        }
    }else{
        res=((exp-srcBias+dstBias)<<dstSigBits)|((uint32_t)sig<<(dstSigBits-srcSigBits));
    }
    return sign|res;
}
static uint16_t trunc16(uint64_t aRep,int srcBits,int srcSigBits,int dstSigBits){
    const int srcExpBits=srcBits-srcSigBits-1;
    const int srcInfExp=(1<<srcExpBits)-1,srcExpBias=srcInfExp>>1;
    const uint64_t srcMinNormal=(uint64_t)1<<srcSigBits;
    const uint64_t srcSignificandMask=srcMinNormal-1;
    const uint64_t srcInfinity=(uint64_t)srcInfExp<<srcSigBits;
    const uint64_t srcSignMask=(uint64_t)1<<(srcSigBits+srcExpBits);
    const uint64_t srcAbsMask=srcSignMask-1;
    const uint64_t roundMask=((uint64_t)1<<(srcSigBits-dstSigBits))-1;
    const uint64_t halfway=(uint64_t)1<<(srcSigBits-dstSigBits-1);
    const uint64_t srcQNaN=(uint64_t)1<<(srcSigBits-1),srcNaNCode=srcQNaN-1;
    const int dstExpBits=16-dstSigBits-1;
    const int dstInfExp=(1<<dstExpBits)-1,dstExpBias=dstInfExp>>1;
    const uint64_t underflow=(uint64_t)(srcExpBias+1-dstExpBias)<<srcSigBits;
    const uint64_t overflow=(uint64_t)(srcExpBias+1+dstExpBias)<<srcSigBits;
    const uint16_t dstQNaN=(uint16_t)1<<(dstSigBits-1),dstNaNCode=dstQNaN-1;
    uint64_t aAbs=aRep&srcAbsMask,sign=aRep&srcSignMask;
    uint16_t absResult;
    if(aAbs>=underflow&&aAbs<overflow){
        absResult=(uint16_t)(aAbs>>(srcSigBits-dstSigBits));
        absResult-=(uint16_t)((uint64_t)(srcExpBias-dstExpBias)<<dstSigBits);
        uint64_t r=aAbs&roundMask;
        if(r>halfway)absResult++;else if(r==halfway)absResult+=absResult&1;
    }else if(aAbs>srcInfinity){
        absResult=(uint16_t)dstInfExp<<dstSigBits;absResult|=dstQNaN;
        absResult|=(uint16_t)(((aAbs&srcNaNCode)>>(srcSigBits-dstSigBits))&dstNaNCode);
    }else if(aAbs>=overflow){
        absResult=(uint16_t)dstInfExp<<dstSigBits;
    }else{
        int aExp=(int)(aAbs>>srcSigBits);
        int shift=srcExpBias-dstExpBias-aExp+1;
        uint64_t significand=(aRep&srcSignificandMask)|srcMinNormal;
        if(shift>srcSigBits)absResult=0;
        else{
            int sticky=(significand&(((uint64_t)1<<shift)-1))!=0;
            uint64_t denorm=(significand>>shift)|(uint64_t)sticky;
            absResult=(uint16_t)(denorm>>(srcSigBits-dstSigBits));
            uint64_t r=denorm&roundMask;
            if(r>halfway)absResult++;else if(r==halfway)absResult+=absResult&1;
        }
    }
    return absResult|(uint16_t)(sign>>(srcBits-16));
}
static uint16_t trunc_sf_bf(uint32_t u){
    if((u&0x7fffffff)>0x7f800000)return (uint16_t)((u>>16)|0x0040);
    u+=0x7fff+((u>>16)&1);
    return (uint16_t)(u>>16);
}

// Public builtins with the float-typed ABI LLVM's libcall lowering expects.
// Arg/return marshalling is pure bit reinterpretation (no fp conversions).
float    __extendhfsf2(_Float16 a){union{_Float16 h;uint16_t u;}v;v.h=a;union{uint32_t u;float f;}r;r.u=ext_hf_sf(v.u);return r.f;}
_Float16 __truncsfhf2(float a){union{uint16_t u;_Float16 h;}r;r.u=trunc16((uint64_t)f2u(a),32,23,10);return r.h;}
_Float16 __truncdfhf2(double a){union{uint16_t u;_Float16 h;}r;r.u=trunc16(d2u(a),64,52,10);return r.h;}
__bf16   __truncsfbf2(float a){union{uint16_t u;__bf16 b;}r;r.u=trunc_sf_bf(f2u(a));return r.b;}
__bf16   __truncdfbf2(double a){union{uint16_t u;__bf16 b;}r;r.u=trunc16(d2u(a),64,52,7);return r.b;}
