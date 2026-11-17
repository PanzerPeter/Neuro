// Number to text: the fast path behind an interpolated float hole.
//
// `__neuro_format_f64` renders `value` as C's `%.{precision}f` (for `precision` from 0 to
// 19) or as `%.16g` (for a `precision` of -1), with a leading `+` on a value whose sign bit
// is clear when `plus` is nonzero. It writes at most FORMAT_F64_MAX_BYTES bytes to `out` and
// returns their count, or returns -1 without writing for anything it does not cover: a NaN
// or an infinity, any other precision, a fixed rendering whose digits do not fit 64 bits,
// and a `%.16g` magnitude outside [1e-7, 1e16). The backend renders those through
// `snprintf`, so this path only has to be exact where it answers.
//
// It is exact: a finite double is m * 2^e, and every value it prints is m * 2^e * 10^s
// rounded to an integer, which fits 128 bits wherever this path answers. The rounding
// is to nearest with ties to even, on the exact product, which is what glibc, the macOS
// libc and the Windows CRT all do in the default rounding mode. Neuro never changes the
// rounding mode.
//
// Verified against glibc `snprintf` over random bit patterns, decimal values, exact ties,
// powers of ten and their neighbours, at every precision it takes, with zero mismatches.
//
// Regenerate text_format.ll with tools/regen_gpu_runtime.sh.

#include <stdint.h>

typedef unsigned __int128 u128;

#define FORMAT_F64_MAX_BYTES 48
#define GENERAL_PRECISION 16
#define MAX_FIXED_PRECISION 19

static const uint64_t POW10[20] = {
    1ull,
    10ull,
    100ull,
    1000ull,
    10000ull,
    100000ull,
    1000000ull,
    10000000ull,
    100000000ull,
    1000000000ull,
    10000000000ull,
    100000000000ull,
    1000000000000ull,
    10000000000000ull,
    100000000000000ull,
    1000000000000000ull,
    10000000000000000ull,
    100000000000000000ull,
    1000000000000000000ull,
    10000000000000000000ull,
};

// `m * 10^s` for m < 2^53 and s <= 22, which is below 2^127.
static u128 scale(uint64_t m, int s) {
    if (s > MAX_FIXED_PRECISION) {
        return (u128)(m * POW10[s - MAX_FIXED_PRECISION]) * POW10[MAX_FIXED_PRECISION];
    }
    return (u128)m * POW10[s];
}

// `product / 2^k`, rounded to nearest with ties to even, for a product from `scale` and
// 0 < k. Past k = 117 the product is under half of 2^k, so the quotient rounds to zero.
static u128 round_shift(u128 product, int k) {
    if (k > 117) {
        return 0;
    }
    u128 quotient = product >> k;
    u128 rest = product & (((u128)1 << k) - 1);
    u128 half = (u128)1 << (k - 1);
    if (rest > half || (rest == half && (quotient & 1))) {
        quotient += 1;
    }
    return quotient;
}

// Write `value`'s decimal digits, `width` of them with leading zeros, ending just before
// `end`. Returns where the first digit went.
static uint8_t *digits_before(uint8_t *end, uint64_t value, int width) {
    for (int i = 0; i < width; i++) {
        *--end = (uint8_t)('0' + value % 10);
        value /= 10;
    }
    return end;
}

static int digit_count(uint64_t value) {
    int count = 1;
    while (value >= 10) {
        value /= 10;
        count++;
    }
    return count;
}

static uint8_t *put_bytes(uint8_t *out, const uint8_t *from, int count) {
    for (int i = 0; i < count; i++) {
        out[i] = from[i];
    }
    return out + count;
}

static int64_t fixed(uint8_t *out, int negative, int plus, uint64_t m, int e, int precision) {
    u128 scaled;
    if (e >= 0) {
        // m * 2^e is an integer; the digits must still fit 64 bits once scaled.
        if (e > 11) {
            return -1;
        }
        scaled = (u128)(m << e) * POW10[precision];
    } else {
        scaled = round_shift(scale(m, precision), -e);
    }
    if (scaled >> 64) {
        return -1;
    }
    uint64_t q = (uint64_t)scaled;
    uint64_t whole = q / POW10[precision];
    uint64_t fraction = q % POW10[precision];

    uint8_t *cursor = out;
    if (negative) {
        *cursor++ = '-';
    } else if (plus) {
        *cursor++ = '+';
    }
    int whole_digits = digit_count(whole);
    digits_before(cursor + whole_digits, whole, whole_digits);
    cursor += whole_digits;
    if (precision > 0) {
        *cursor++ = '.';
        digits_before(cursor + precision, fraction, precision);
        cursor += precision;
    }
    return cursor - out;
}

static int64_t general(uint8_t *out, int negative, int plus, uint64_t m, int e) {
    uint8_t *cursor = out;
    if (negative) {
        *cursor++ = '-';
    } else if (plus) {
        *cursor++ = '+';
    }
    if (m == 0) {
        *cursor++ = '0';
        return cursor - out;
    }

    // floor(log2(m * 2^e)), then floor(log10) of the value to within one: 78913 / 2^18 is
    // log10(2) to six digits, which gives floor(b * log10(2)) exactly for every binary
    // exponent b reaching here.
    int binary_exponent = 63 - __builtin_clzll(m) + e;
    if (binary_exponent < -24 || binary_exponent > 53) {
        return -1;
    }
    int exponent = (binary_exponent * 78913) >> 18;

    // The 16 significant digits, exact: m * 2^e * 10^(15 - exponent), rounded. The exponent
    // estimate is low by at most one, so one correction step settles it.
    uint64_t digits = 0;
    for (int attempt = 0; attempt < 2; attempt++) {
        int s = GENERAL_PRECISION - 1 - exponent;
        if (s < 0 || s > 22) {
            return -1;
        }
        // e >= 0 only for a value of at least 2^52, so m * 2^e is below 2^54 and s is 0.
        u128 product = e >= 0 ? (u128)(m << e) * POW10[s] : scale(m, s);
        u128 floor_value = e >= 0 ? product : product >> -e;
        if (floor_value >= POW10[GENERAL_PRECISION]) {
            exponent++;
            continue;
        }
        digits = e >= 0 ? (uint64_t)product : (uint64_t)round_shift(product, -e);
        break;
    }
    if (digits == 0) {
        return -1;
    }
    if (digits == POW10[GENERAL_PRECISION]) {
        // Rounding carried into a seventeenth digit: 9.99...95 became 10.
        digits = POW10[GENERAL_PRECISION - 1];
        exponent++;
    }

    // The digits most significant first, then how many survive once trailing zeros go.
    uint8_t text[GENERAL_PRECISION];
    digits_before(text + GENERAL_PRECISION, digits, GENERAL_PRECISION);
    int kept = GENERAL_PRECISION;
    while (kept > 1 && text[kept - 1] == '0') {
        kept--;
    }

    if (exponent < -4 || exponent >= GENERAL_PRECISION) {
        // `%e` style: d.ddd, then e, a sign and at least two exponent digits.
        *cursor++ = text[0];
        if (kept > 1) {
            *cursor++ = '.';
            cursor = put_bytes(cursor, text + 1, kept - 1);
        }
        *cursor++ = 'e';
        int magnitude = exponent;
        if (exponent < 0) {
            *cursor++ = '-';
            magnitude = -exponent;
        } else {
            *cursor++ = '+';
        }
        int width = magnitude < 10 ? 2 : digit_count((uint64_t)magnitude);
        digits_before(cursor + width, (uint64_t)magnitude, width);
        return cursor + width - out;
    }

    if (exponent < 0) {
        // 0.000ddd: the point, then -exponent - 1 zeros, then the kept digits.
        *cursor++ = '0';
        *cursor++ = '.';
        for (int i = 0; i < -exponent - 1; i++) {
            *cursor++ = '0';
        }
        cursor = put_bytes(cursor, text, kept);
        return cursor - out;
    }

    // ddd.ddd: exponent + 1 digits before the point, the kept rest after it.
    int whole = exponent + 1;
    cursor = put_bytes(cursor, text, whole);
    if (kept > whole) {
        *cursor++ = '.';
        cursor = put_bytes(cursor, text + whole, kept - whole);
    }
    return cursor - out;
}

int64_t __neuro_format_f64(double value, int32_t precision, int32_t plus, uint8_t *out) {
    uint64_t bits;
    __builtin_memcpy(&bits, &value, sizeof bits);
    int negative = (int)(bits >> 63);
    int biased = (int)((bits >> 52) & 0x7ff);
    uint64_t m = bits & ((1ull << 52) - 1);
    if (biased == 0x7ff) {
        return -1;
    }
    int e;
    if (biased == 0) {
        e = -1074;
    } else {
        m |= 1ull << 52;
        e = biased - 1075;
    }

    if (precision == -1) {
        return general(out, negative, plus, m, e);
    }
    if (precision < 0 || precision > MAX_FIXED_PRECISION) {
        return -1;
    }
    if (m == 0) {
        e = -1;
    }
    return fixed(out, negative, plus, m, e, precision);
}
